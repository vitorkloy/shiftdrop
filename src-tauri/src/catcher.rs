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
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use std::sync::OnceLock;
    use tauri::AppHandle;
    use windows::core::{implement, w, Ref, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINTL, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateEllipticRgn, SetWindowRgn};
    use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Ole::{
        IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, LoadCursorW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos,
        ShowWindow, HWND_TOPMOST, IDC_ARROW, LWA_ALPHA, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_LBUTTONUP,
        WM_MOUSEACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    static HWND_VAL: AtomicIsize = AtomicIsize::new(0);
    static CENTER_R: AtomicU32 = AtomicU32::new(50);
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// Esc durante o arraste: o drop seguinte não vira ação, e a janela-alvo
    /// fica até o botão soltar (senão o Explorer recebe o Shift e move o arquivo).
    static SUPPRESS_DROP: AtomicBool = AtomicBool::new(false);
    static HOLD_CATCHER: AtomicBool = AtomicBool::new(false);

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
            if SUPPRESS_DROP.swap(false, Ordering::Relaxed) {
                log(format!("catcher: Drop em ({},{}) cancelado pelo Esc", pt.x, pt.y));
                unsafe { *effect = DROPEFFECT_COPY };
                return Ok(());
            }
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
                        crate::ui::emit(app, "sd-cancel", serde_json::json!(null));
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }

    /// Cria a janela e registra o drop target. Precisa rodar na thread principal (STA + OleInitialize).
    ///
    /// `RegisterDragDrop` devolve E_OUTOFMEMORY (0x8007000E) se o thread só tiver
    /// `CoInitialize` — o caso desde que a roda deixou de existir na abertura.
    pub fn init(app: &AppHandle) {
        let _ = APP.set(app.clone());
        install();
    }

    fn install() {
        if hwnd().is_some() {
            return;
        }
        let r: windows::core::Result<HWND> = unsafe {
            (|| {
                if let Err(e) = OleInitialize(None) {
                    log(format!("catcher: OleInitialize: {e}"));
                }
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
                if let Err(e) = RegisterDragDrop(h, &target) {
                    let _ = DestroyWindow(h);
                    return Err(e);
                }
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
            if hwnd().is_none() {
                install();
            }
            let Some(hw) = hwnd() else {
                log("catcher: show sem janela");
                return;
            };
            CENTER_R.store((50.0 * scale * crate::gesture::ui_scale()) as u32, Ordering::Relaxed);
            let ok = SetWindowPos(hw, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let rgn = CreateEllipticRgn(0, 0, w + 1, h + 1);
            let r = SetWindowRgn(hw, Some(rgn), true);
            log(format!("catcher: visível em ({x},{y}) {w}x{h} pos={ok:?} região={r}"));
        });
    }

    fn hide_hwnd() {
        unsafe {
            if let Some(hw) = hwnd() {
                let _ = ShowWindow(hw, SW_HIDE);
            }
        }
    }

    pub fn hide(app: &AppHandle) {
        let _ = app.run_on_main_thread(|| {
            if HOLD_CATCHER.load(Ordering::Relaxed) {
                return;
            }
            hide_hwnd();
        });
    }

    pub fn arm_escape_cancel() {
        SUPPRESS_DROP.store(true, Ordering::Relaxed);
        HOLD_CATCHER.store(true, Ordering::Relaxed);
    }

    pub fn clear_suppress() {
        SUPPRESS_DROP.store(false, Ordering::Relaxed);
    }

    pub fn finish_hold(app: &AppHandle) {
        if HOLD_CATCHER.swap(false, Ordering::Relaxed) {
            let _ = app.run_on_main_thread(hide_hwnd);
        }
    }
}

#[cfg(windows)]
pub use imp::{arm_escape_cancel, clear_suppress, finish_hold, hide, init, show};

#[cfg(not(windows))]
pub fn init(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn show(_app: &AppHandle, _x: i32, _y: i32, _w: i32, _h: i32, _scale: f64) {}
#[cfg(not(windows))]
pub fn hide(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn arm_escape_cancel() {}
#[cfg(not(windows))]
pub fn clear_suppress() {}
#[cfg(not(windows))]
pub fn finish_hold(_app: &AppHandle) {}
