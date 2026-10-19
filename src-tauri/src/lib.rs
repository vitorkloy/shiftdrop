pub mod actions;
pub mod cli;
pub mod convert;
pub mod pdf;
pub mod tools;

mod gesture;
mod glass;
mod tray;

use std::path::PathBuf;

fn to_paths(v: Vec<String>) -> Vec<PathBuf> {
    v.into_iter().map(PathBuf::from).collect()
}

#[tauri::command]
fn list_actions(paths: Vec<String>) -> Vec<actions::ActionInfo> {
    actions::list_actions(&to_paths(paths), &tools::Tools::detect())
}

#[tauri::command]
async fn run_action(action_id: String, paths: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        convert::run_action(&action_id, &to_paths(paths), &tools::Tools::detect())
            .map(|v| v.into_iter().map(|p| p.display().to_string()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn hide_radial(app: tauri::AppHandle) {
    gesture::hide_radial(&app);
}

/// Miniatura do arquivo como data URL (ou null se não houver).
#[tauri::command]
async fn thumbnail(path: String) -> Option<String> {
    use base64::Engine;
    tauri::async_runtime::spawn_blocking(move || {
        let png = convert::thumbnail_png(std::path::Path::new(&path), &tools::Tools::detect(), 72)?;
        Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
    })
    .await
    .ok()
    .flatten()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .invoke_handler(tauri::generate_handler![list_actions, run_action, hide_radial, thumbnail])
        .setup(|app| {
            tray::build(app)?;
            gesture::spawn(app.handle().clone());
            let args: Vec<String> = std::env::args().skip(1).collect();
            if args.first().map(|a| a == "preview").unwrap_or(false) {
                let mut hover = -1;
                let mut paths = Vec::new();
                let mut it = args.into_iter().skip(1);
                while let Some(a) = it.next() {
                    if a == "--hover" {
                        hover = it.next().and_then(|n| n.parse().ok()).unwrap_or(-1);
                    } else {
                        paths.push(a);
                    }
                }
                gesture::preview(app.handle(), paths, hover);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o ShiftDrop");
}
