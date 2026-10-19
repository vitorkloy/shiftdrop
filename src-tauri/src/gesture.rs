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
    let size = win.outer_size().unwrap_or(PhysicalSize::new(290, 290));
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
    crate::logx::log(format!("menu: mostrar em ({px},{py}) {w}x{h} cursor=({x},{y}) manual={manual}"));
    let _ = app.emit("radial-show", serde_json::json!({ "manual": manual }));
    let _ = win.set_position(PhysicalPosition::new(px, py));
    crate::glass::apply(&win);
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    // Reaplica o recorte depois de exibir (o tamanho final pode mudar com o DPI do monitor).
    crate::glass::apply(&win);
    let size = win.outer_size().unwrap_or(size);
    let scale = win.scale_factor().unwrap_or(1.0);
    crate::catcher::show(app, px, py, size.width as i32, size.height as i32, scale);
    if manual {
        let _ = win.set_focus();
    }
}

/// Escala da roda: SHIFTDROP_SCALE (0.4–2.0), padrão 0.75 do tamanho original (290 px).
pub fn ui_scale() -> f64 {
    std::env::var("SHIFTDROP_SCALE")
        .ok()
        .and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.4, 2.0))
        .unwrap_or(0.75)
}

/// Lado da janela da roda em px lógicos.
pub fn wheel_px() -> f64 {
    (290.0 * ui_scale()).round()
}

pub fn hide_radial(app: &AppHandle) {
    crate::catcher::hide(app);
    if let Some(w) = app.get_webview_window("radial") {
        let _ = w.hide();
    }
}

/// Modo de demonstração: `shiftdrop.exe preview <arquivos...> [--mode convert|tools|actions] [--hover N]` abre o menu
/// no centro da tela já com os arquivos "arrastados" (usado para capturas/QA).
pub fn preview(app: &AppHandle, paths: Vec<String>, hover: i32, mode: String) {
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
        let _ = app.emit("radial-preview", serde_json::json!({ "paths": paths, "hover": hover, "mode": mode }));
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
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LBUTTON, VK_MENU, VK_SHIFT};
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
    /// Roda de cada tecla: Shift = converter, Ctrl = ferramentas, Alt = ações.
    const MODES: [(&str, &str); 3] = [("convert", "Shift"), ("tools", "Ctrl"), ("actions", "Alt")];

    /// Teclas das rodas. Trocáveis por SHIFTDROP_KEYS=convert,tools,actions com
    /// nomes shift/ctrl/alt (ex. "ctrl,shift,alt").
    fn wheel_keys() -> [u16; 3] {
        let code = |n: &str| match n.trim().to_ascii_lowercase().as_str() {
            "shift" => Some(VK_SHIFT),
            "ctrl" | "control" => Some(VK_CONTROL),
            "alt" => Some(VK_MENU),
            _ => None,
        };
        let def = [VK_SHIFT, VK_CONTROL, VK_MENU];
        let Ok(v) = std::env::var("SHIFTDROP_KEYS") else { return def };
        let ks: Vec<u16> = v.split(',').filter_map(code).collect();
        if ks.len() == 3 { [ks[0], ks[1], ks[2]] } else { def }
    }

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
                crate::logx::log("gesto: thread de detecção iniciada (15 ms)");
                let mut down = false;
                let mut armed = false;
                let mut shown = false;
                let mut origin = (0, 0);
                // Estado anterior de Shift/Ctrl/Alt (para detectar o "toque" na tecla).
                let mut prev = [false; 3];
                let keys = wheel_keys();
                loop {
                    std::thread::sleep(Duration::from_millis(15));
                    let left = pressed(VK_LBUTTON);
                    let now = [pressed(keys[0]), pressed(keys[1]), pressed(keys[2])];
                    let tapped = (0..3).find(|&i| now[i] && !prev[i]);
                    let held = (0..3).find(|&i| now[i]);
                    prev = now;
                    if left && !down {
                        down = true;
                        origin = cursor_pos();
                        armed = is_file_source(origin.0, origin.1);
                    } else if left && down {
                        if armed && !shown {
                            // "Start dragging, then tap a key": abre quando há arraste e uma das teclas está pressionada.
                            if let Some(k) = held {
                                let (x, y) = cursor_pos();
                                let (dx, dy) = (x - origin.0, y - origin.1);
                                if dx * dx + dy * dy > THRESHOLD_PX * THRESHOLD_PX {
                                    shown = true;
                                    crate::logx::log(format!(
                                        "gesto: arrastar + {} detectado (origem {:?} -> {:?}, processo {:?})",
                                        MODES[k].1,
                                        origin,
                                        (x, y),
                                        process_name_at(origin.0, origin.1)
                                    ));
                                    show_radial(&app, x, y, false);
                                    let _ = app.emit("wheel-mode", serde_json::json!({ "mode": MODES[k].0 }));
                                }
                            }
                        } else if shown {
                            if let Some(k) = tapped {
                                crate::logx::log(format!("gesto: tecla {} → roda {}", MODES[k].1, MODES[k].0));
                                let _ = app.emit("wheel-mode", serde_json::json!({ "mode": MODES[k].0 }));
                            }
                        }
                    } else if !left && down {
                        down = false;
                        armed = false;
                        if shown {
                            shown = false;
                            crate::logx::log("gesto: botão solto (fim do arraste)");
                            let _ = app.emit("gesture-end", ());
                        }
                    }
                }
            })
            .expect("não foi possível iniciar a thread do gesto");
    }
}

/// macOS / Linux: consulta o estado global de mouse e teclado a cada 15 ms
/// (device_query: CoreGraphics no macOS, X11/XWayland no Linux). Sem fonte de
/// processo (abre de qualquer app). Em Wayland puro sem XWayland não há leitura
/// global: fica só o atalho da bandeja ("Abrir menu aqui").
#[cfg(not(windows))]
mod imp {
    use super::show_radial;
    use device_query::{DeviceQuery, DeviceState, Keycode};
    use std::time::Duration;
    use tauri::{AppHandle, Emitter, Manager};

    const THRESHOLD: i32 = 12;
    const MODES: [&str; 3] = ["convert", "tools", "actions"];

    fn keyset(k: &[Keycode]) -> [bool; 3] {
        let has = |a: &[Keycode]| a.iter().any(|x| k.contains(x));
        // Shift = converter; Ctrl/Command = ferramentas; Alt/Option = ações.
        let mut tools = vec![Keycode::LControl, Keycode::RControl];
        if cfg!(target_os = "macos") {
            tools.extend([Keycode::Command, Keycode::LMeta, Keycode::RMeta]);
        }
        [
            has(&[Keycode::LShift, Keycode::RShift]),
            has(&tools),
            has(&[Keycode::LAlt, Keycode::RAlt, Keycode::LOption, Keycode::ROption]),
        ]
    }

    fn state() -> Option<DeviceState> {
        if cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        DeviceState::checked_new()
    }

    fn to_physical(app: &AppHandle, (x, y): (i32, i32)) -> (i32, i32) {
        // CoreGraphics devolve pontos (lógicos); a janela trabalha em pixels físicos.
        if cfg!(target_os = "macos") {
            let s = app
                .get_webview_window("radial")
                .and_then(|w| w.scale_factor().ok())
                .unwrap_or(1.0);
            ((x as f64 * s) as i32, (y as f64 * s) as i32)
        } else {
            (x, y)
        }
    }

    pub fn cursor_pos() -> (i32, i32) {
        state().map(|d| d.get_mouse().coords).unwrap_or((400, 400))
    }

    pub fn spawn(app: AppHandle) {
        std::thread::spawn(move || {
            let Some(dev) = state() else {
                crate::logx::log("gesto: sem leitura global de teclado/mouse (Wayland?) — use o menu da bandeja");
                return;
            };
            crate::logx::log("gesto: thread de detecção iniciada (device_query, 15 ms)");
            let (mut down, mut shown, mut origin, mut prev) = (false, false, (0, 0), [false; 3]);
            loop {
                std::thread::sleep(Duration::from_millis(15));
                let m = dev.get_mouse();
                let left = m.button_pressed.get(1).copied().unwrap_or(false);
                let now = keyset(&dev.get_keys());
                let tapped = (0..3).find(|&i| now[i] && !prev[i]);
                let held = (0..3).find(|&i| now[i]);
                prev = now;
                if left && !down {
                    down = true;
                    origin = m.coords;
                } else if left && down {
                    if !shown {
                        if let Some(k) = held {
                            let (dx, dy) = (m.coords.0 - origin.0, m.coords.1 - origin.1);
                            if dx * dx + dy * dy > THRESHOLD * THRESHOLD {
                                shown = true;
                                let (x, y) = to_physical(&app, m.coords);
                                show_radial(&app, x, y, false);
                                let _ = app.emit("wheel-mode", serde_json::json!({ "mode": MODES[k] }));
                            }
                        }
                    } else if let Some(k) = tapped {
                        let _ = app.emit("wheel-mode", serde_json::json!({ "mode": MODES[k] }));
                    }
                } else if !left && down {
                    down = false;
                    if shown {
                        shown = false;
                        let _ = app.emit("gesture-end", ());
                    }
                }
            }
        });
    }
}
