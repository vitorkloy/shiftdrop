//! Janela da roda sob demanda.
//!
//! Para não pesar no PC, o WebView (WebView2 no Windows) só existe enquanto
//! a roda é usada: é criado no início de um arraste e destruído alguns
//! segundos depois de esconder (config `unload_secs`). Eventos emitidos antes
//! do JS carregar ficam numa fila e são entregues quando ele chama `ui_ready`.
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

static READY: AtomicBool = AtomicBool::new(false);
static GEN: AtomicU64 = AtomicU64::new(0);
static QUEUE: Mutex<Vec<(String, Value)>> = Mutex::new(Vec::new());
static BUILD: Mutex<()> = Mutex::new(());

pub fn is_ready() -> bool {
    READY.load(Ordering::SeqCst)
}

/// Emite para o JS; se a página ainda não carregou, guarda na fila.
/// O lock cobre o envio: um evento novo não passa na frente dos que já estavam na fila.
pub fn emit(app: &AppHandle, ev: &str, payload: Value) {
    let mut q = QUEUE.lock().unwrap();
    if READY.load(Ordering::SeqCst) {
        let _ = app.emit(ev, payload);
    } else {
        if q.len() > 200 {
            q.remove(0);
        }
        q.push((ev.to_string(), payload));
    }
}

/// Chamado pelo JS quando todos os listeners estão registrados.
pub fn ready(app: &AppHandle) {
    let mut q = QUEUE.lock().unwrap();
    READY.store(true, Ordering::SeqCst);
    let items: Vec<_> = q.drain(..).collect();
    let n = items.len();
    for (e, p) in items {
        let _ = app.emit(&e, p);
    }
    drop(q);
    crate::logx::log(format!("ui: página pronta ({n} eventos na fila)"));
}

fn reset_page() {
    let mut q = QUEUE.lock().unwrap();
    READY.store(false, Ordering::SeqCst);
    q.clear();
}

/// Janela da roda; cria se não existir. Não chamar da thread principal (o build do Tauri espera por ela).
pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    let _g = BUILD.lock().unwrap();
    GEN.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("radial") {
        return Some(w);
    }
    reset_page();
    let conf = app.config().app.windows.iter().find(|w| w.label == "radial")?.clone();
    let t = std::time::Instant::now();
    let px = crate::gesture::wheel_px();
    let w = tauri::WebviewWindowBuilder::from_config(app, &conf).ok()?.inner_size(px, px).build().ok()?;
    crate::logx::log(format!("ui: janela da roda criada em {} ms ({px}px)", t.elapsed().as_millis()));
    Some(w)
}

/// Depois de esconder: descarrega o WebView se ficar sem uso por `unload_secs`.
pub fn schedule_unload(app: &AppHandle) {
    let secs = crate::config::get().unload_secs;
    if secs == 0 {
        return;
    }
    let gen = {
        let _g = BUILD.lock().unwrap();
        GEN.fetch_add(1, Ordering::SeqCst) + 1
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        let _g = BUILD.lock().unwrap();
        if GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        if let Some(w) = app.get_webview_window("radial") {
            if !w.is_visible().unwrap_or(false) {
                reset_page();
                let _ = w.destroy();
                crate::logx::log("ui: WebView da roda descarregado (ocioso)");
            }
        }
    });
}

/// Janela de configurações (também sob demanda; fechar a destrói).
pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_focus();
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings.html".into()))
            .title("ShiftDrop — Configurações")
            .inner_size(380.0, 370.0)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .center()
            .build();
    });
}

/// Primeira abertura: pergunta se o app deve avisar atualizações.
pub fn open_notify(app: &AppHandle) {
    if app.get_webview_window("notify").is_some() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = tauri::WebviewWindowBuilder::new(&app, "notify", tauri::WebviewUrl::App("notify.html".into()))
            .title("ShiftDrop")
            .inner_size(400.0, 210.0)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .always_on_top(true)
            .center()
            .build();
    });
}

/// Notas da versão nova e botões Atualizar agora / Depois.
pub fn open_update(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("update") {
        let _ = w.set_focus();
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = tauri::WebviewWindowBuilder::new(&app, "update", tauri::WebviewUrl::App("update.html".into()))
            .title("ShiftDrop — Atualização")
            .inner_size(420.0, 340.0)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .always_on_top(true)
            .center()
            .build();
    });
}
