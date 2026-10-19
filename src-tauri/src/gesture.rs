//! Detecção do gesto "arrastar arquivo segurando Shift".
//!
//! Uma thread consulta o estado global do botão esquerdo e do Shift
//! (GetAsyncKeyState) a cada 15 ms. Quando o botão é pressionado sobre uma
//! janela do Explorer (ou outro gerenciador de arquivos) e o cursor se move com
//! Shift pressionado, a janela transparente do menu radial aparece centrada no
//! cursor. Ela é um drop target normal (OLE): o usuário solta o arquivo em cima
//! da opção desejada. Nada de hook de baixo nível nem DLL injetada.
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

pub fn show_radial(app: &AppHandle, x: i32, y: i32, manual: bool) {
    let Some(win) = app.get_webview_window("radial") else { return };
    let size = win.outer_size().unwrap_or(PhysicalSize::new(380, 380));
    let (w, h) = (size.width as i32, size.height as i32);
    let (mut px, mut py) = (x - w / 2, y - h / 2);
    if let Ok(mons) = win.available_monitors() {
        for m in mons {
            let (p, s) = (m.position(), m.size());
            let (mw, mh) = (s.width as i32, s.height as i32);
            if x >= p.x && x < p.x + mw && y >= p.y && y < p.y + mh && mw >= w && mh >= h {
                px = px.clamp(p.x, p.x + mw - w);
                py = py.clamp(p.y, p.y + mh - h);
            }
        }
    }
    let _ = app.emit("radial-show", serde_json::json!({ "manual": manual }));
    // Sombra suave: janela maior, transparente e "fantasma" (não recebe mouse nem drop) atrás do disco.
    if let Some(sh) = app.get_webview_window("shadow") {
        if let Ok(ss) = sh.outer_size() {
            let (cx, cy) = (px + w / 2, py + h / 2);
            let _ = sh.set_position(PhysicalPosition::new(cx - ss.width as i32 / 2, cy - ss.height as i32 / 2));
        }
        let _ = sh.set_ignore_cursor_events(true);
        let _ = sh.show();
    }
    let _ = win.set_position(PhysicalPosition::new(px, py));
    crate::glass::apply(&win);
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    if manual {
        let _ = win.set_focus();
    }
}

pub fn hide_radial(app: &AppHandle) {
    for label in ["radial", "shadow"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
}

/// Modo de demonstração: `shiftdrop.exe preview <arquivos...> [--hover N]` abre o menu
/// no centro da tela já com os arquivos "arrastados" (usado para capturas/QA).
pub fn preview(app: &AppHandle, paths: Vec<String>, hover: i32) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let (x, y) = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| (m.position().x + m.size().width as i32 / 2, m.position().y + m.size().height as i32 / 2))
            .unwrap_or((800, 450));
        show_radial(&app, x, y, false);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = app.emit("radial-preview", serde_json::json!({ "paths": paths, "hover": hover }));
    });
}

pub fn show_at_cursor(app: &AppHandle) {
    let (x, y) = imp::cursor_pos();
    show_radial(app, x, y, true);
}

pub fn spawn(app: AppHandle) {
    imp::spawn(app);
}

#[cfg(windows)]
mod imp {
    use super::show_radial;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};
    use windows_sys::Win32::Foundation::{CloseHandle, POINT};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_SHIFT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GetCursorPos, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT};

    /// Processos de onde um arraste de arquivo pode começar.
    const SOURCES: &[&str] = &[
        "explorer.exe",
        "files.exe",
        "onecommander.exe",
        "totalcmd.exe",
        "totalcmd64.exe",
        "doublecmd.exe",
        "explorer++.exe",
        "xyplorer.exe",
        "directoryopus.exe",
        "dopus.exe",
    ];
    const THRESHOLD_PX: i32 = 12;

    pub fn cursor_pos() -> (i32, i32) {
        let mut p = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut p) };
        (p.x, p.y)
    }

    fn pressed(vk: u16) -> bool {
        (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
    }

    fn process_name_at(x: i32, y: i32) -> Option<String> {
        unsafe {
            let hwnd = WindowFromPoint(POINT { x, y });
            if hwnd.is_null() {
                return None;
            }
            let root = GetAncestor(hwnd, GA_ROOT);
            let target = if root.is_null() { hwnd } else { root };
            let mut pid = 0u32;
            GetWindowThreadProcessId(target, &mut pid);
            if pid == 0 {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            Some(full.rsplit('\\').next().unwrap_or("").to_ascii_lowercase())
        }
    }

    fn is_file_source(x: i32, y: i32) -> bool {
        if std::env::var_os("SHIFTDROP_ANY_APP").is_some() {
            return true;
        }
        process_name_at(x, y).map(|n| SOURCES.contains(&n.as_str())).unwrap_or(false)
    }

    pub fn spawn(app: AppHandle) {
        std::thread::Builder::new()
            .name("shiftdrop-gesture".into())
            .spawn(move || {
                let mut down = false;
                let mut armed = false;
                let mut shown = false;
                let mut origin = (0, 0);
                loop {
                    std::thread::sleep(Duration::from_millis(15));
                    let left = pressed(VK_LBUTTON);
                    if left && !down {
                        down = true;
                        origin = cursor_pos();
                        armed = is_file_source(origin.0, origin.1);
                    } else if left && down {
                        if armed && !shown && pressed(VK_SHIFT) {
                            let (x, y) = cursor_pos();
                            let (dx, dy) = (x - origin.0, y - origin.1);
                            if dx * dx + dy * dy > THRESHOLD_PX * THRESHOLD_PX {
                                shown = true;
                                show_radial(&app, x, y, false);
                            }
                        }
                    } else if !left && down {
                        down = false;
                        armed = false;
                        if shown {
                            shown = false;
                            let _ = app.emit("gesture-end", ());
                        }
                    }
                }
            })
            .expect("não foi possível iniciar a thread do gesto");
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::AppHandle;
    pub fn cursor_pos() -> (i32, i32) {
        (400, 400)
    }
    pub fn spawn(_app: AppHandle) {}
}
