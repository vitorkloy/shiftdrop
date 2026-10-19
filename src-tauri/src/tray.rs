//! Ícone e menu da bandeja do sistema.
use crate::gesture;
use crate::tools::Tools;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let t = Tools::detect();
    let status = |name: &str, ok: bool| format!("{} {}", if ok { "✓" } else { "✗" }, name);

    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "hint", "Segure Shift enquanto arrasta um arquivo", false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "t1", status("ffmpeg (vídeo/áudio)", t.ffmpeg.is_some()), false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "t2", status("ImageMagick (HEIC/AVIF, WEBP menor)", t.magick.is_some()), false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "t3", status("LibreOffice (DOCX → PDF)", t.soffice.is_some()), false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "show", "Abrir menu aqui (arraste um arquivo nele)", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?)?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ShiftDrop")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => gesture::show_at_cursor(app),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
