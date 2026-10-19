//! Autoteste do arrastar: `shiftdrop.exe selftest-drag <arquivo> [--drop]`.
//!
//! Dentro do próprio app, abre o overlay como o gesto faria (sem caminho), cria o
//! IDataObject do Shell para o arquivo (o mesmo tipo que o Explorer entrega, com
//! CF_HDROP) e chama o IDropTarget registrado na janela do overlay:
//! DragEnter → DragOver → (Drop). Tudo é registrado em %TEMP%\shiftdrop-drag.log.
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

#[cfg(not(windows))]
pub fn start(_app: &AppHandle, _file: String, _drop: bool) {}

#[cfg(windows)]
pub fn start(app: &AppHandle, file: String, do_drop: bool) {
    use std::time::Duration;
    use tauri::Manager;
    ACTIVE.store(true, Ordering::Relaxed);
    let _ = std::fs::remove_file(std::env::temp_dir().join("shiftdrop-drag.log"));
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let (x, y) = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| (m.position().x + m.size().width as i32 / 2, m.position().y + m.size().height as i32 / 2))
            .unwrap_or((800, 450));
        crate::gesture::show_radial(&app, x, y, false);
        std::thread::sleep(Duration::from_millis(500));

        // Ponto alvo: 140 px (lógicos) acima do centro do overlay = setor do topo.
        let (px, py) = app
            .get_webview_window("radial")
            .and_then(|w| {
                let pos = w.outer_position().ok()?;
                let size = w.outer_size().ok()?;
                let sc = w.scale_factor().unwrap_or(1.0);
                Some((pos.x + size.width as i32 / 2, pos.y + size.height as i32 / 2 - (140.0 * sc) as i32))
            })
            .unwrap_or((x, y - 140));

        let step = |name: &'static str, f: Box<dyn FnOnce() -> String + Send>| {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = app.run_on_main_thread(move || {
                let _ = tx.send(f());
            });
            let r = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| "timeout".into());
            crate::drag::log(&format!("selftest {name}: {r}"));
        };
        let f = file.clone();
        step("DragEnter", Box::new(move || ole::enter(&f, px, py)));
        std::thread::sleep(Duration::from_millis(150));
        step("DragOver", Box::new(move || ole::over(px, py)));
        std::thread::sleep(Duration::from_millis(150));
        step("DragOver", Box::new(move || ole::over(px, py)));
        std::thread::sleep(Duration::from_millis(2500));
        if do_drop {
            step("Drop", Box::new(move || ole::drop(px, py)));
        } else {
            step("DragLeave", Box::new(ole::leave));
        }
    });
}

#[cfg(windows)]
mod ole {
    use std::cell::RefCell;
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, POINTL};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{IDropTarget, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE};
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_SHIFT};
    use windows::Win32::UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetClassNameW, GetPropW, GetWindow, GW_CHILD, GW_HWNDNEXT};

    thread_local! {
        static STATE: RefCell<Option<(IDropTarget, IDataObject)>> = const { RefCell::new(None) };
    }

    /// Coleta todas as janelas com IDropTarget registrado (como o OLE faz, o alvo
    /// real é a janela filha mais profunda sob o cursor: a do WebView2).
    fn collect(hwnd: HWND, depth: u32, out: &mut Vec<(u32, HWND, String)>) {
        unsafe {
            let raw = GetPropW(hwnd, &HSTRING::from("OleDropTargetInterface"));
            if !raw.is_invalid() && !raw.0.is_null() {
                let mut buf = [0u16; 128];
                let n = GetClassNameW(hwnd, &mut buf) as usize;
                out.push((depth, hwnd, String::from_utf16_lossy(&buf[..n])));
            }
            if depth > 8 {
                return;
            }
            let mut child = GetWindow(hwnd, GW_CHILD).ok();
            while let Some(c) = child {
                if c.0.is_null() {
                    break;
                }
                collect(c, depth + 1, out);
                child = GetWindow(c, GW_HWNDNEXT).ok();
            }
        }
    }

    fn find_target(top: HWND) -> (Option<IDropTarget>, String) {
        let mut all = Vec::new();
        collect(top, 0, &mut all);
        let desc = all.iter().map(|(d, h, c)| format!("{c}@{d}({:?})", h.0)).collect::<Vec<_>>().join(", ");
        let best = all.iter().max_by_key(|(d, _, _)| *d).and_then(|(_, h, _)| unsafe {
            let raw = GetPropW(*h, &HSTRING::from("OleDropTargetInterface"));
            IDropTarget::from_raw_borrowed(&raw.0).cloned()
        });
        (best, desc)
    }

    const KEYS: u32 = MK_LBUTTON.0 | MK_SHIFT.0;

    pub fn enter(file: &str, x: i32, y: i32) -> String {
        unsafe {
            let top = match FindWindowW(PCWSTR::null(), &HSTRING::from("ShiftDrop")) {
                Ok(h) => h,
                Err(e) => return format!("janela não encontrada: {e}"),
            };
            let (target, desc) = find_target(top);
            let Some(target) = target else { return format!("nenhum IDropTarget registrado ({desc})") };
            let item: IShellItem = match SHCreateItemFromParsingName(&HSTRING::from(file), None) {
                Ok(i) => i,
                Err(e) => return format!("SHCreateItemFromParsingName: {e}"),
            };
            let data: IDataObject = match item.BindToHandler(None, &BHID_DataObject) {
                Ok(d) => d,
                Err(e) => return format!("BindToHandler: {e}"),
            };
            let mut effect = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0);
            let r = target.DragEnter(&data, windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS(KEYS), POINTL { x, y }, &mut effect);
            STATE.with(|s| *s.borrow_mut() = Some((target, data)));
            format!("{r:?} effect={} at ({x},{y}); alvos: [{desc}] -> usando o mais profundo", effect.0)
        }
    }

    pub fn over(x: i32, y: i32) -> String {
        STATE.with(|s| {
            let Some((t, _)) = s.borrow().clone() else { return "sem estado".into() };
            let mut effect = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0);
            let r = unsafe { t.DragOver(windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS(KEYS), POINTL { x, y }, &mut effect) };
            format!("{r:?} effect={}", effect.0)
        })
    }

    pub fn drop(x: i32, y: i32) -> String {
        STATE.with(|s| {
            let Some((t, d)) = s.borrow_mut().take() else { return "sem estado".into() };
            let mut effect = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0);
            let r = unsafe { t.Drop(&d, windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS(KEYS), POINTL { x, y }, &mut effect) };
            format!("{r:?} effect={} (1=cópia, 2=mover)", effect.0)
        })
    }

    pub fn leave() -> String {
        STATE.with(|s| {
            let Some((t, _)) = s.borrow_mut().take() else { return "sem estado".into() };
            format!("{:?}", unsafe { t.DragLeave() })
        })
    }
}
