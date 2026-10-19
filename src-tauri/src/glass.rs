//! Vidro "acrylic" recortado em círculo para a janela do menu.
//!
//! Usa a política de accent do DWM (SetWindowCompositionAttribute,
//! ACCENT_ENABLE_ACRYLICBLURBEHIND), que continua desfocando mesmo com a janela
//! sem foco (o backdrop "Mica/Acrylic" do Win11 fica opaco quando inativo, e o
//! menu nunca recebe foco durante o arraste). A região da janela é uma elipse,
//! então fora do disco o clique/drop passa para quem estiver embaixo.

#[cfg(windows)]
pub fn apply(window: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Gdi::{CreateEllipticRgn, SetWindowRgn};
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as *mut c_void;
    let Ok(size) = window.outer_size() else { return };

    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        gradient: u32,
        animation: u32,
    }
    #[repr(C)]
    struct CompAttrData {
        attrib: u32,
        data: *mut c_void,
        size: usize,
    }
    type SetWca = unsafe extern "system" fn(*mut c_void, *mut CompAttrData) -> i32;

    unsafe {
        let rgn = CreateEllipticRgn(0, 0, size.width as i32 + 1, size.height as i32 + 1);
        let r = SetWindowRgn(hwnd, rgn, 1);
        crate::logx::log(format!("vidro: região circular {}x{} aplicada={}", size.width, size.height, r != 0));

        let user32: Vec<u16> = "user32.dll\0".encode_utf16().collect();
        let module = GetModuleHandleW(user32.as_ptr());
        if module.is_null() {
            return;
        }
        let Some(f) = GetProcAddress(module, b"SetWindowCompositionAttribute\0".as_ptr()) else { return };
        let set: SetWca = std::mem::transmute(f);
        // Cor em ABGR: cinza-claro do Tangerine (#e5e7e9, ~60%); o relevo vem do CSS/SVG.
        let mut accent = AccentPolicy { state: 4, flags: 0, gradient: 0x99E9E7E5, animation: 0 };
        let mut data = CompAttrData {
            attrib: 19, // WCA_ACCENT_POLICY
            data: &mut accent as *mut _ as *mut c_void,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        set(hwnd, &mut data);
    }
}

#[cfg(not(windows))]
pub fn apply(_window: &tauri::WebviewWindow) {}
