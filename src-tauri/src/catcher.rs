//! Alvo de drop nativo do ShiftDrop.
//!
//! Uma janela Win32 própria (popup em camadas, alpha 1/255: invisível mas
//! "acertável" pelo mouse), recortada no mesmo círculo do menu e posta por cima
//! dele. Ela registra o NOSSO IDropTarget (RegisterDragDrop), lê CF_HDROP já no
//! DragEnter e manda os eventos ao JS. Assim o arraste vindo do Explorer não
//! depende de qual janela interna do WebView2 está sob o cursor.
#[allow(unused_imports)]
use tauri::AppHandle;

#[cfg(windows)]
mod imp {
    use crate::logx::log;
    use std::cell::Cell;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter};
    use windows::core::{implement, w, Ref, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINTL, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateEllipticRgn, SetWindowRgn};
    use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Ole::{
        IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, GetWindowRect, LoadCursorW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos,
        ShowWindow, HWND_TOPMOST, IDC_ARROW, LWA_ALPHA, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_LBUTTONUP,
        WM_MOUSEACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    static HWND_VAL: AtomicIsize = AtomicIsize::new(0);
    static CENTER_R: AtomicU32 = AtomicU32::new(58);
    static APP: OnceLock<AppHandle> = OnceLock::new();

    fn hwnd() -> Option<HWND> {
        let v = HWND_VAL.load(Ordering::Relaxed);
        (v != 0).then(|| HWND(v as *mut _))
    }

    fn rel(pt: &POINTL) -> Option<(f64, f64)> {
        let h = hwnd()?;
        let mut r = RECT::default();
        unsafe { GetWindowRect(h, &mut r).ok()? };
        Some(((pt.x - r.left) as f64, (pt.y - r.top) as f64))
    }

    unsafe fn paths_of(data: Ref<'_, IDataObject>) -> Option<Vec<PathBuf>> {
        let d = data.as_ref()?;
        let fmt = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = match d.GetData(&fmt) {
            Ok(m) => m,
            Err(e) => {
                log(format!("catcher: sem CF_HDROP ({e})"));
                return None;
            }
        };
        let hdrop = HDROP(medium.u.hGlobal.0 as _);
        let n = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
        let mut v = Vec::with_capacity(n as usize);
        for i in 0..n {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            v.push(PathBuf::from(OsString::from_wide(&buf[..len])));
        }
        ReleaseStgMedium(&mut medium);
        Some(v)
    }

    fn dispatch(kind: &str, paths: Vec<PathBuf>, pos: Option<(f64, f64)>) {
        if let Some(app) = APP.get() {
            crate::drag::dispatch(app, "nativo", kind, paths, pos);
        }
    }

    #[implement(IDropTarget)]
    struct Target {
        has_files: Cell<bool>,
    }

    #[allow(non_snake_case)]
    impl IDropTarget_Impl for Target_Impl {
        fn DragEnter(&self, data: Ref<'_, IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
            let paths = unsafe { paths_of(data) };
            self.has_files.set(paths.is_some());
            log(format!("catcher: DragEnter teclas=0x{:x} em ({},{}) arquivos={:?}", keys.0, pt.x, pt.y, paths));
            dispatch("enter", paths.unwrap_or_default(), rel(pt));
            unsafe { *effect = if self.has_files.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            Ok(())
        }
        fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
            dispatch("over", vec![], rel(pt));
            unsafe { *effect = if self.has_files.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            Ok(())
        }
        fn DragLeave(&self) -> windows::core::Result<()> {
            log("catcher: DragLeave");
            dispatch("leave", vec![], None);
            Ok(())
        }
        fn Drop(&self, data: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
            let paths = unsafe { paths_of(data) };
            log(format!("catcher: Drop em ({},{}) arquivos={:?}", pt.x, pt.y, paths));
            dispatch("drop", paths.unwrap_or_default(), rel(pt));
            // Sempre "cópia": o Explorer nunca apaga o original (Shift = mover no Explorer).
            unsafe { *effect = DROPEFFECT_COPY };
            Ok(())
        }
    }

    unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_LBUTTONUP => {
                // Clique no centro = fechar (o catcher cobre o webview).
                let x = (lp.0 & 0xffff) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                let mut r = RECT::default();
                let _ = GetWindowRect(h, &mut r);
                let (cx, cy) = ((r.right - r.left) / 2, (r.bottom - r.top) / 2);
                let d2 = (x - cx).pow(2) + (y - cy).pow(2);
                let cr = CENTER_R.load(Ordering::Relaxed) as i32;
                if d2 < cr * cr {
                    if let Some(app) = APP.get() {
                        let _ = app.emit("sd-cancel", ());
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }

    /// Cria a janela e registra o drop target. Precisa rodar na thread principal (STA/OLE).
    pub fn init(app: &AppHandle) {
        let _ = APP.set(app.clone());
        let r: windows::core::Result<HWND> = unsafe {
            (|| {
                let hinst: HINSTANCE = GetModuleHandleW(PCWSTR::null())?.into();
                let cls = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinst,
                    lpszClassName: w!("ShiftDropCatcher"),
                    hCursor: LoadCursorW(None, IDC_ARROW)?,
                    ..Default::default()
                };
                RegisterClassW(&cls);
                let h = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("ShiftDropCatcher"),
                    w!("ShiftDrop drop"),
                    WS_POPUP,
                    0,
                    0,
                    10,
                    10,
                    None,
                    None,
                    Some(hinst),
                    None,
                )?;
                SetLayeredWindowAttributes(h, COLORREF(0), 1, LWA_ALPHA)?;
                let target: IDropTarget = Target { has_files: Cell::new(false) }.into();
                RegisterDragDrop(h, &target)?;
                std::mem::forget(target); // vive enquanto o app viver
                Ok(h)
            })()
        };
        match r {
            Ok(h) => {
                HWND_VAL.store(h.0 as isize, Ordering::Relaxed);
                log(format!("catcher: janela de drop criada ({:?}) e IDropTarget registrado", h.0));
            }
            Err(e) => log(format!("catcher: ERRO ao criar alvo de drop: {e}")),
        }
    }

    pub fn show(app: &AppHandle, x: i32, y: i32, w: i32, h: i32, scale: f64) {
        let _ = app.run_on_main_thread(move || unsafe {
            let Some(hw) = hwnd() else {
                log("catcher: show sem janela");
                return;
            };
            CENTER_R.store((58.0 * scale) as u32, Ordering::Relaxed);
            let ok = SetWindowPos(hw, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let rgn = CreateEllipticRgn(0, 0, w + 1, h + 1);
            let r = SetWindowRgn(hw, Some(rgn), true);
            log(format!("catcher: visível em ({x},{y}) {w}x{h} pos={ok:?} região={r}"));
        });
    }

    pub fn hide(app: &AppHandle) {
        let _ = app.run_on_main_thread(|| unsafe {
            if let Some(hw) = hwnd() {
                let _ = ShowWindow(hw, SW_HIDE);
            }
        });
    }
}

#[cfg(windows)]
pub use imp::{hide, init, show};

#[cfg(not(windows))]
pub fn init(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn show(_app: &AppHandle, _x: i32, _y: i32, _w: i32, _h: i32, _scale: f64) {}
#[cfg(not(windows))]
pub fn hide(_app: &AppHandle) {}
