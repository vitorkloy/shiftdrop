pub mod actions;
pub mod cli;
pub mod convert;
pub mod pdf;
pub mod tools;

mod catcher;
mod drag;
mod logx;
mod gesture;
mod selftest;
mod glass;
mod tray;

use std::path::PathBuf;
use tauri::Manager;

fn to_paths(v: Vec<String>) -> Vec<PathBuf> {
    v.into_iter().map(PathBuf::from).collect()
}

#[tauri::command]
fn list_actions(paths: Vec<String>) -> Vec<actions::ActionInfo> {
    actions::list_actions(&to_paths(paths), &tools::Tools::detect())
}

/// Setores genéricos (quando o tipo do arquivo ainda não é conhecido).
#[tauri::command]
fn generic_actions() -> Vec<actions::ActionInfo> {
    actions::generic_actions()
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

/// Console do JS → log em arquivo.
#[tauri::command]
fn js_log(level: String, msg: String) {
    logx::log(format!("js[{level}] {msg}"));
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
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            logx::log(format!("segunda instância chamada com {:?}", args));
            // `shiftdrop.exe debug-show [x y]`: abre o menu como o gesto (sem arquivo) na instância que já roda.
            if args.get(1).map(|a| a == "debug-show").unwrap_or(false) {
                let xy = (args.get(2).and_then(|v| v.parse().ok()), args.get(3).and_then(|v| v.parse().ok()));
                match xy {
                    (Some(x), Some(y)) => gesture::show_radial(app, x, y, false),
                    _ => gesture::show_at_cursor(app),
                }
            }
        }))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(e) = event {
                drag::forward(window.app_handle(), window.label(), e);
            }
        })
        .on_webview_event(|webview, event| {
            if let tauri::WebviewEvent::DragDrop(e) = event {
                drag::forward(webview.app_handle(), webview.label(), e);
            }
        })
        .invoke_handler(tauri::generate_handler![js_log, list_actions, generic_actions, run_action, hide_radial, thumbnail])
        .setup(|app| {
            logx::log(format!(
                "==== ShiftDrop {} iniciado: exe={:?} pid={} args={:?}",
                env!("CARGO_PKG_VERSION"),
                std::env::current_exe().ok(),
                std::process::id(),
                std::env::args().collect::<Vec<_>>()
            ));
            logx::log(format!("ferramentas: {:?}", tools::Tools::detect()));
            catcher::init(app.handle());
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
            } else if args.first().map(|a| a == "selftest-drag").unwrap_or(false) && args.len() >= 2 {
                selftest::start(app.handle(), args[1].clone(), args.iter().any(|a| a == "--drop"));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o ShiftDrop");
}
