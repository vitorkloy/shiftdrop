//! Recorte circular da janela da roda.
//!
//! Sem acrylic/accent do DWM: o `SetWindowCompositionAttribute`
//! (ACCENT_ENABLE_ACRYLICBLURBEHIND) pinta o retângulo inteiro da janela e
//! ignora a região do `SetWindowRgn`, o que gerava o quadrado cinza (antes
//! escuro) atrás do disco. O vidro agora é desenhado só no CSS/SVG sobre a
//! janela transparente, e a região elíptica apenas garante que cliques/drops
//! fora do disco passem para quem está embaixo.

#[cfg(windows)]
pub fn apply(window: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Gdi::{CreateEllipticRgn, SetWindowRgn};

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as *mut c_void;
    let Ok(size) = window.outer_size() else { return };
    unsafe {
        let rgn = CreateEllipticRgn(0, 0, size.width as i32 + 1, size.height as i32 + 1);
        let r = SetWindowRgn(hwnd, rgn, 1);
        crate::logx::log(format!("vidro: região circular {}x{} aplicada={} (sem acrylic)", size.width, size.height, r != 0));
    }
}

#[cfg(not(windows))]
pub fn apply(_window: &tauri::WebviewWindow) {}
