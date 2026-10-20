//! Checagem de atualização no GitHub (tauri-plugin-updater).
//!
//! Consulta `releases/latest/download/latest.json` alguns segundos depois de
//! abrir e de novo a cada 6 horas. Só baixa e instala depois da confirmação.
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

const RELEASES: &str = "https://github.com/vitorkloy/shiftdrop/releases/latest";
const FIRST_WAIT: Duration = Duration::from_secs(8);
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
pub struct Pending {
    pub version: String,
    pub notes: String,
    pub can_install: bool,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static TOASTED: Mutex<Option<String>> = Mutex::new(None);

/// Windows, macOS e AppImage trocam o próprio binário. O `.deb` abre a página do release.
pub fn can_self_update() -> bool {
    cfg!(windows) || cfg!(target_os = "macos") || std::env::var_os("APPIMAGE").is_some()
}

pub fn pending() -> Option<Pending> {
    PENDING.lock().unwrap().clone()
}

pub fn spawn(app: AppHandle) {
    let _ = app.notification().on_action({
        let app = app.clone();
        move |a| {
            if a.action_id() == "tap" {
                crate::ui::open_update(&app);
            }
        }
    });
    std::thread::spawn(move || {
        if crate::config::get().notify_updates.is_none() {
            crate::ui::open_notify(&app);
        }
        std::thread::sleep(FIRST_WAIT);
        loop {
            let app2 = app.clone();
            tauri::async_runtime::block_on(async move {
                if let Err(e) = check(&app2).await {
                    crate::logx::log(format!("update: {e}"));
                }
            });
            std::thread::sleep(INTERVAL);
        }
    });
}

async fn check(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(u)) => {
            let info = Pending {
                version: u.version.clone(),
                notes: u.body.clone().unwrap_or_default(),
                can_install: can_self_update(),
            };
            crate::logx::log(format!("update: disponível {}", info.version));
            *PENDING.lock().unwrap() = Some(info.clone());
            refresh_tray(app);
            maybe_toast(app, &info);
        }
        Ok(None) => {
            crate::logx::log("update: já está na última versão");
            *PENDING.lock().unwrap() = None;
            refresh_tray(app);
        }
        Err(e) => crate::logx::log(format!("update: checagem falhou: {e}")),
    }
    Ok(())
}

fn refresh_tray(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::tray::refresh(&handle));
}

fn maybe_toast(app: &AppHandle, info: &Pending) {
    if crate::config::get().notify_updates != Some(true) {
        return;
    }
    {
        let mut t = TOASTED.lock().unwrap();
        if t.as_deref() == Some(info.version.as_str()) {
            return;
        }
        *t = Some(info.version.clone());
    }
    let _ = app
        .notification()
        .builder()
        .title("Nova versão do ShiftDrop")
        .body(format!("A versão {} está disponível.", info.version))
        .show();
}

/// Depois da primeira pergunta: se já houver versão nova e o aviso estiver ligado, tosta.
pub fn after_notify_choice(app: &AppHandle) {
    if let Some(info) = pending() {
        maybe_toast(app, &info);
    }
}

pub async fn install(app: AppHandle) -> Result<(), String> {
    if !can_self_update() {
        open_releases();
        return Ok(());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(u) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("nenhuma atualização disponível".into());
    };
    crate::logx::log(format!("update: instalando {}", u.version));
    u.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart();
}

pub fn open_releases() {
    let url = RELEASES;
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
