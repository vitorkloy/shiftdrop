pub mod actions;
pub mod cli;
pub mod convert;
pub mod pdf;
pub mod tools;
pub mod wheels;
pub mod fileactions;
pub mod deps;
pub mod config;
mod ui;
mod update;

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

/// As três rodas (convert/tools/actions) para os arquivos arrastados (vazio = genéricas).
#[tauri::command]
async fn wheels(paths: Vec<String>) -> wheels::Wheels {
    tauri::async_runtime::spawn_blocking(move || wheels::wheels(&to_paths(paths), &tools::Tools::detect()))
        .await
        .unwrap_or_else(|_| wheels::wheels(&[], &tools::Tools::default()))
}

/// Setores genéricos (quando o tipo do arquivo ainda não é conhecido).
#[tauri::command]
fn generic_actions() -> Vec<actions::ActionInfo> {
    actions::generic_actions()
}

#[tauri::command]
async fn run_action(action_id: String, paths: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deps::allow_downloads(|| convert::run_action(&action_id, &to_paths(paths), &tools::Tools::detect()))
            .map(|v| v.into_iter().map(|p| p.display().to_string()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(windows))]
#[tauri::command]
fn accent_color() -> Option<String> {
    None
}

/// Cor de acento do Windows (#rrggbb), lida de HKCU\Software\Microsoft\Windows\DWM\AccentColor (ABGR).
/// A interface não consulta este comando: as telas seguem a paleta da logo.
#[cfg(windows)]
#[tauri::command]
fn accent_color() -> Option<String> {
    let out = tools::command(std::path::Path::new("reg.exe"))
        .args(["query", r"HKCU\Software\Microsoft\Windows\DWM", "/v", "AccentColor"])
        .output()
        .ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    let hex = txt.split_whitespace().find(|w| w.starts_with("0x"))?;
    let v = u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok()?;
    let (r, g, b) = (v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff);
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Console do JS → log em arquivo.
#[tauri::command]
fn js_log(level: String, msg: String) {
    logx::log(format!("js[{level}] {msg}"));
}

#[tauri::command]
fn ui_ready(app: tauri::AppHandle) {
    ui::ready(&app);
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> config::Config {
    use tauri_plugin_autostart::ManagerExt;
    let mut c = config::get();
    c.autostart = app.autolaunch().is_enabled().unwrap_or(c.autostart);
    c
}

#[tauri::command]
fn set_config(app: tauri::AppHandle, cfg: config::Config) -> Result<config::Config, String> {
    use tauri_plugin_autostart::ManagerExt;
    let c = config::save(cfg)?;
    let al = app.autolaunch();
    let r = if c.autostart { al.enable() } else { al.disable() };
    if let Err(e) = r {
        logx::log(format!("autostart: {e}"));
    }
    if c.notify_updates == Some(true) {
        #[cfg(target_os = "macos")]
        {
            use tauri_plugin_notification::NotificationExt;
            let _ = app.notification().request_permission();
        }
        update::after_notify_choice(&app);
    }
    // A roda é recriada com o novo tamanho no próximo uso.
    if let Some(w) = app.get_webview_window("radial") {
        if !w.is_visible().unwrap_or(false) {
            let _ = w.destroy();
            let app2 = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                ui::warm(&app2);
            });
        }
    }
    logx::log(format!("config salva: {c:?}"));
    Ok(c)
}

#[tauri::command]
fn set_notify_updates(app: tauri::AppHandle, enabled: bool) -> Result<config::Config, String> {
    let mut c = config::get();
    c.notify_updates = Some(enabled);
    let c = config::save(c)?;
    if enabled {
        #[cfg(target_os = "macos")]
        {
            use tauri_plugin_notification::NotificationExt;
            let _ = app.notification().request_permission();
        }
    }
    if let Some(w) = app.get_webview_window("notify") {
        let _ = w.close();
    }
    update::after_notify_choice(&app);
    logx::log(format!("notify_updates={enabled}"));
    Ok(c)
}

#[tauri::command]
fn pending_update() -> Option<update::Pending> {
    update::pending()
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    update::install(app).await
}

#[tauri::command]
fn open_release_page() {
    update::open_releases();
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
                let app = app.clone();
                std::thread::spawn(move || match xy {
                    (Some(x), Some(y)) => {
                        let _ = ui::window(&app);
                        gesture::show_radial(&app, x, y, false);
                    }
                    _ => gesture::show_at_cursor(&app),
                });
            } else if args.get(1).map(|a| a == "settings").unwrap_or(false) {
                ui::open_settings(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
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
        .invoke_handler(tauri::generate_handler![
            js_log, ui_ready, get_config, set_config, set_notify_updates, pending_update, install_update, open_release_page,
            accent_color, wheels, list_actions, generic_actions, run_action, hide_radial, thumbnail
        ])
        .setup(|app| {
            logx::log(format!(
                "==== ShiftDrop {} iniciado: exe={:?} pid={} args={:?}",
                env!("CARGO_PKG_VERSION"),
                std::env::current_exe().ok(),
                std::process::id(),
                std::env::args().collect::<Vec<_>>()
            ));
            logx::log(format!("ferramentas: {:?}", tools::Tools::detect()));
            logx::log(format!("roda: escala {} (criada sob demanda)", gesture::ui_scale()));
            // 1ª execução: liga "iniciar com o sistema" conforme a config.
            if !config::path().is_file() {
                use tauri_plugin_autostart::ManagerExt;
                let c = config::get();
                if c.autostart {
                    let _ = app.autolaunch().enable();
                }
                let _ = config::save(c);
            }
            deps::init(app.handle());
            deps::prune_old();
            catcher::init(app.handle());
            tray::build(app)?;
            gesture::spawn(app.handle().clone());
            // A roda fica pronta antes do primeiro arraste. Criar no meio do gesto
            // levava vários segundos e o menu abria depois do mouse solto.
            ui::warm(app.handle());
            let args: Vec<String> = std::env::args().skip(1).collect();
            if !args.iter().any(|a| a == "preview" || a == "selftest-drag") {
                update::spawn(app.handle().clone());
            }
            if args.first().map(|a| a == "preview").unwrap_or(false) {
                let mut hover = -1;
                let mut mode = String::from("convert");
                let mut paths = Vec::new();
                let mut it = args.into_iter().skip(1);
                while let Some(a) = it.next() {
                    if a == "--hover" {
                        hover = it.next().and_then(|n| n.parse().ok()).unwrap_or(-1);
                    } else if a == "--mode" {
                        mode = it.next().unwrap_or(mode);
                    } else {
                        paths.push(a);
                    }
                }
                gesture::preview(app.handle(), paths, hover, mode);
            } else if args.first().map(|a| a == "settings").unwrap_or(false) {
                ui::open_settings(app.handle());
            } else if args.first().map(|a| a == "selftest-drag").unwrap_or(false) && args.len() >= 2 {
                selftest::start(app.handle(), args[1].clone(), args.iter().any(|a| a == "--drop"));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o ShiftDrop")
        .run(|_app, event| {
            // App de bandeja: fechar a última janela (roda, aviso, config) não pode encerrar o processo.
            // `run` não devolve Result: quando o app termina, o processo sai direto.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
