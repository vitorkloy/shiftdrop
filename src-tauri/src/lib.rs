pub mod actions;
pub mod cli;
pub mod convert;
pub mod pdf;
pub mod tools;

mod gesture;
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
fn hide_radial(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .invoke_handler(tauri::generate_handler![list_actions, run_action, hide_radial])
        .setup(|app| {
            tray::build(app)?;
            gesture::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o ShiftDrop");
}
