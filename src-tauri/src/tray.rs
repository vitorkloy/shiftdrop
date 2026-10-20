//! Ícone e menu da bandeja do sistema.
use crate::gesture;
use crate::tools::Tools;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let menu = compose(app.handle(), None)?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ShiftDrop")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => gesture::show_at_cursor(app),
            "settings" => crate::ui::open_settings(app),
            "update" => crate::ui::open_update(app),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Recoloca o item de atualização quando o GitHub anuncia uma versão nova.
pub fn refresh(app: &AppHandle) {
    let ver = crate::update::pending().map(|p| p.version);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(m) = compose(app, ver.as_deref()) {
            let _ = tray.set_menu(Some(m));
        }
    }
}

fn compose(app: &AppHandle, update: Option<&str>) -> tauri::Result<Menu<tauri::Wry>> {
    let t = Tools::detect();
    let status = |name: &str, ok: bool| format!("{} {}", if ok { "✓" } else { "✗" }, name);

    let menu = Menu::new(app)?;
    if let Some(v) = update {
        menu.append(&MenuItem::with_id(app, "update", format!("Atualizar para {v}…"), true, None::<&str>)?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "hint", "Segure Shift enquanto arrasta um arquivo", false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "t1", status("ffmpeg (vídeo/áudio)", t.ffmpeg.is_some()), false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "t2", status("ImageMagick (HEIC/AVIF, WEBP menor)", t.magick.is_some()), false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "t3", status("LibreOffice (DOCX → PDF)", t.soffice.is_some()), false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "show", "Abrir menu aqui (arraste um arquivo nele)", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Configurações…", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?)?;
    Ok(menu)
}
