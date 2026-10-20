//! Detecção do gesto "arrastar arquivo segurando uma tecla".
//!
//! No Windows, hooks de baixo nível (WH_MOUSE_LL + WH_KEYBOARD_LL) numa thread
//! com loop de mensagens: ocioso, a CPU fica em zero. No macOS e no Linux, uma
//! thread consulta mouse e modificadores (device_query) — 15 ms com o botão
//! pressionado, 120 ms ocioso. A tecla de cada roda vem da config
//! (SHIFTDROP_KEYS ainda ganha). A janela da roda é um drop target: o usuário
//! solta o arquivo em cima da opção desejada.
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

pub fn show_radial(app: &AppHandle, x: i32, y: i32, manual: bool) -> bool {
    // O gesto não pode bloquear na criação do WebView: o mouse-up ficaria na fila
    // e a roda abria e fechava sozinha. manual (bandeja/preview) espera a janela.
    let Some(win) = (if manual { crate::ui::window(app) } else { crate::ui::try_window(app) }) else { return false };
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
    crate::ui::emit(app, "radial-show", serde_json::json!({ "manual": manual }));
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
    true
}

/// Avisa a thread do gesto que a janela da roda ficou pronta.
pub fn poke() {
    imp::poke();
}

/// Escala da roda: SHIFTDROP_SCALE (0.4–2.0), padrão 0.75 do tamanho original (290 px).
pub fn ui_scale() -> f64 {
    std::env::var("SHIFTDROP_SCALE")
        .ok()
        .and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or_else(|| crate::config::get().scale)
        .clamp(0.4, 2.0)
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
    crate::ui::schedule_unload(app);
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
        let _ = crate::ui::window(&app);
        show_radial(&app, x, y, false);
        std::thread::sleep(std::time::Duration::from_millis(300));
        crate::ui::emit(&app, "radial-preview", serde_json::json!({ "paths": paths, "hover": hover, "mode": mode }));
    });
}

/// Pelo menu da bandeja/2ª instância (thread principal): cria a janela em outra thread.
pub fn show_at_cursor(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let (x, y) = imp::to_screen(&app, imp::cursor_pos());
        show_radial(&app, x, y, true);
    });
}

pub fn spawn(app: AppHandle) {
    imp::spawn(app);
}

/// Windows: hooks de baixo nível (WH_MOUSE_LL + WH_KEYBOARD_LL) numa thread com
/// loop de mensagens — zero CPU enquanto nada acontece (nada de polling). O
/// callback só repassa o evento por um canal; a lógica roda noutra thread para
/// o hook responder rápido.
#[cfg(windows)]
mod imp {
    use super::show_radial;
    use std::sync::mpsc::{channel, Sender};
    use std::sync::OnceLock;
    use tauri::AppHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetAncestor, GetCursorPos, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
        TranslateMessage, WindowFromPoint, GA_ROOT, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_SYSKEYDOWN,
    };

    const SOURCES: &[&str] = &[
        "explorer.exe", "files.exe", "onecommander.exe", "totalcmd.exe", "totalcmd64.exe", "doublecmd.exe", "explorer++.exe",
        "xyplorer.exe", "directoryopus.exe", "dopus.exe",
    ];
    const THRESHOLD_PX: i32 = 12;
    const MODES: [(&str, &str); 3] = [("convert", "Shift"), ("tools", "Ctrl"), ("actions", "Alt")];

    enum Ev {
        Down(i32, i32),
        Move(i32, i32),
        Up,
        Key(u16),
        /// A janela da roda terminou de ser criada.
        Tick,
    }

    pub fn poke() {
        if let Some(tx) = TX.get() {
            let _ = tx.send(Ev::Tick);
        }
    }
    static TX: OnceLock<Sender<Ev>> = OnceLock::new();
    static DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn vk_of(n: &str) -> u16 {
        match n {
            "ctrl" | "control" => VK_CONTROL,
            "alt" => VK_MENU,
            _ => VK_SHIFT,
        }
    }
    fn wheel_keys() -> [u16; 3] {
        let k = crate::config::keys();
        [vk_of(&k[0]), vk_of(&k[1]), vk_of(&k[2])]
    }
    fn generic(vk: u16) -> u16 {
        match vk {
            VK_LSHIFT | VK_RSHIFT => VK_SHIFT,
            VK_LCONTROL | VK_RCONTROL => VK_CONTROL,
            VK_LMENU | VK_RMENU => VK_MENU,
            v => v,
        }
    }

    pub fn cursor_pos() -> (i32, i32) {
        let mut p = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut p) };
        (p.x, p.y)
    }
    /// Coordenadas já são pixels físicos.
    pub fn to_screen(_app: &AppHandle, pos: (i32, i32)) -> (i32, i32) {
        pos
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
        std::env::var_os("SHIFTDROP_ANY_APP").is_some() || process_name_at(x, y).map(|n| SOURCES.contains(&n.as_str())).unwrap_or(false)
    }

    unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if code >= 0 {
            let m = &*(lp as *const MSLLHOOKSTRUCT);
            let (x, y) = (m.pt.x, m.pt.y);
            let ev = match wp as u32 {
                WM_LBUTTONDOWN => {
                    DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                    Some(Ev::Down(x, y))
                }
                WM_LBUTTONUP => {
                    DOWN.store(false, std::sync::atomic::Ordering::Relaxed);
                    Some(Ev::Up)
                }
                // Movimento só interessa com o botão pressionado.
                WM_MOUSEMOVE if DOWN.load(std::sync::atomic::Ordering::Relaxed) => Some(Ev::Move(x, y)),
                _ => None,
            };
            if let (Some(ev), Some(tx)) = (ev, TX.get()) {
                let _ = tx.send(ev);
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wp, lp)
    }

    unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if code >= 0 && (wp as u32 == WM_KEYDOWN || wp as u32 == WM_SYSKEYDOWN) && DOWN.load(std::sync::atomic::Ordering::Relaxed) {
            let k = &*(lp as *const KBDLLHOOKSTRUCT);
            if let Some(tx) = TX.get() {
                let _ = tx.send(Ev::Key(generic(k.vkCode as u16)));
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wp, lp)
    }

    pub fn spawn(app: AppHandle) {
        let (tx, rx) = channel::<Ev>();
        let _ = TX.set(tx);
        // Thread dos hooks: só bloqueia em GetMessageW.
        std::thread::Builder::new()
            .name("shiftdrop-hooks".into())
            .spawn(|| unsafe {
                let m = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), std::ptr::null_mut(), 0);
                let k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), std::ptr::null_mut(), 0);
                crate::logx::log(format!("gesto: hooks de evento ativos (mouse={} teclado={})", !m.is_null(), !k.is_null()));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("thread dos hooks");
        // Lógica do gesto: acorda só quando chega um evento.
        std::thread::Builder::new()
            .name("shiftdrop-gesture".into())
            .spawn(move || {
                let (mut armed, mut shown, mut origin) = (false, false, (0, 0));
                let mut keys = wheel_keys();
                let reveal = |shown: &mut bool, x: i32, y: i32, k: usize, how: &str| {
                    if *shown {
                        return;
                    }
                    if show_radial(&app, x, y, false) {
                        *shown = true;
                        crate::logx::log(how);
                        crate::ui::emit(&app, "wheel-mode", serde_json::json!({ "mode": MODES[k].0 }));
                    }
                };
                while let Ok(ev) = rx.recv() {
                    match ev {
                        Ev::Down(x, y) => {
                            origin = (x, y);
                            armed = is_file_source(x, y);
                            shown = false;
                            keys = wheel_keys();
                            if armed {
                                crate::ui::warm(&app);
                            }
                        }
                        Ev::Move(x, y) if armed && !shown => {
                            let (dx, dy) = (x - origin.0, y - origin.1);
                            if dx * dx + dy * dy <= THRESHOLD_PX * THRESHOLD_PX {
                                continue;
                            }
                            if let Some(k) = (0..3).find(|&i| pressed(keys[i])) {
                                reveal(
                                    &mut shown,
                                    x,
                                    y,
                                    k,
                                    &format!("gesto: arrastar + {} ({:?} -> {:?})", MODES[k].1, origin, (x, y)),
                                );
                            } else {
                                crate::ui::warm(&app);
                            }
                        }
                        Ev::Key(vk) if armed => {
                            let Some(k) = (0..3).find(|&i| keys[i] == vk) else { continue };
                            if shown {
                                crate::ui::emit(&app, "wheel-mode", serde_json::json!({ "mode": MODES[k].0 }));
                            } else {
                                let (x, y) = cursor_pos();
                                let (dx, dy) = (x - origin.0, y - origin.1);
                                if dx * dx + dy * dy > THRESHOLD_PX * THRESHOLD_PX {
                                    reveal(&mut shown, x, y, k, &format!("gesto: arrastar, depois {}", MODES[k].1));
                                }
                            }
                        }
                        Ev::Tick if armed && !shown => {
                            let (x, y) = cursor_pos();
                            let (dx, dy) = (x - origin.0, y - origin.1);
                            if dx * dx + dy * dy <= THRESHOLD_PX * THRESHOLD_PX {
                                continue;
                            }
                            if let Some(k) = (0..3).find(|&i| pressed(keys[i])) {
                                reveal(&mut shown, x, y, k, &format!("gesto: roda pronta + {}", MODES[k].1));
                            }
                        }
                        Ev::Up => {
                            armed = false;
                            if shown {
                                shown = false;
                                crate::ui::emit(&app, "gesture-end", serde_json::json!(null));
                            }
                        }
                        _ => {}
                    }
                }
            })
            .expect("thread do gesto");
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
    use tauri::{AppHandle, Manager};

    const THRESHOLD: i32 = 12;
    const MODES: [&str; 3] = ["convert", "tools", "actions"];

    fn modifier_down(name: &str, pressed: &[Keycode]) -> bool {
        let has = |a: &[Keycode]| a.iter().any(|x| pressed.contains(x));
        match name {
            "ctrl" | "control" => {
                let mut ks = vec![Keycode::LControl, Keycode::RControl];
                if cfg!(target_os = "macos") {
                    ks.extend([Keycode::Command, Keycode::LMeta, Keycode::RMeta]);
                }
                has(&ks)
            }
            "alt" => has(&[Keycode::LAlt, Keycode::RAlt, Keycode::LOption, Keycode::ROption]),
            _ => has(&[Keycode::LShift, Keycode::RShift]),
        }
    }

    /// Uma entrada por roda (convert, tools, actions), na ordem da config.
    fn keyset(pressed: &[Keycode]) -> [bool; 3] {
        let k = crate::config::keys();
        [modifier_down(&k[0], pressed), modifier_down(&k[1], pressed), modifier_down(&k[2], pressed)]
    }

    fn state() -> Option<DeviceState> {
        if cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        DeviceState::checked_new()
    }

    /// CoreGraphics devolve pontos (lógicos); a janela trabalha em pixels físicos.
    /// Sem a roda criada, usa a escala do monitor principal (no Linux o valor já é físico).
    pub fn to_screen(app: &AppHandle, (x, y): (i32, i32)) -> (i32, i32) {
        if cfg!(target_os = "macos") {
            let s = app
                .get_webview_window("radial")
                .and_then(|w| w.scale_factor().ok())
                .or_else(|| app.primary_monitor().ok().flatten().map(|m| m.scale_factor()))
                .unwrap_or(1.0);
            ((x as f64 * s) as i32, (y as f64 * s) as i32)
        } else {
            (x, y)
        }
    }

    pub fn cursor_pos() -> (i32, i32) {
        state().map(|d| d.get_mouse().coords).unwrap_or((400, 400))
    }

    pub fn poke() {}

    pub fn spawn(app: AppHandle) {
        std::thread::spawn(move || {
            let Some(dev) = state() else {
                crate::logx::log("gesto: sem leitura global de teclado/mouse (Wayland?) — use o menu da bandeja");
                return;
            };
            crate::logx::log("gesto: thread de detecção iniciada (device_query, 15 ms)");
            let (mut down, mut shown, mut origin, mut prev) = (false, false, (0, 0), [false; 3]);
            loop {
                // 15 ms só com o botão pressionado; ocioso, 120 ms (quase zero CPU).
                std::thread::sleep(Duration::from_millis(if down { 15 } else { 120 }));
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
                                let (x, y) = to_screen(&app, m.coords);
                                if show_radial(&app, x, y, false) {
                                    shown = true;
                                    crate::ui::emit(&app, "wheel-mode", serde_json::json!({ "mode": MODES[k] }));
                                }
                            }
                        }
                    } else if let Some(k) = tapped {
                        crate::ui::emit(&app, "wheel-mode", serde_json::json!({ "mode": MODES[k] }));
                    }
                } else if !left && down {
                    down = false;
                    if shown {
                        shown = false;
                        crate::ui::emit(&app, "gesture-end", serde_json::json!(null));
                    }
                }
            }
        });
    }
}
