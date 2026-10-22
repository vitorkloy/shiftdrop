//! Janela da roda.
//!
//! O WebView só é criado quando faz falta (em segundo plano, para não travar
//! o gesto) e fica escondido depois. Destruí-lo fechava a última janela e o
//! processo saía da bandeja; recriar levava vários segundos e o arraste já
//! tinha acabado. Eventos emitidos antes do JS carregar ficam numa fila e
//! são entregues quando ele chama `ui_ready`.
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

static READY: AtomicBool = AtomicBool::new(false);
static GEN: AtomicU64 = AtomicU64::new(0);
static CREATING: AtomicBool = AtomicBool::new(false);
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

fn bump_existing(app: &AppHandle) -> Option<WebviewWindow> {
    let _g = BUILD.lock().unwrap();
    let w = app.get_webview_window("radial")?;
    GEN.fetch_add(1, Ordering::SeqCst);
    Some(w)
}

/// Cria a janela. Não segura o lock durante o build: o WebView2 espera a thread principal.
fn build_window(app: &AppHandle) -> Option<WebviewWindow> {
    {
        let _g = BUILD.lock().unwrap();
        if let Some(w) = app.get_webview_window("radial") {
            GEN.fetch_add(1, Ordering::SeqCst);
            return Some(w);
        }
        GEN.fetch_add(1, Ordering::SeqCst);
        reset_page();
    }
    let conf = app.config().app.windows.iter().find(|w| w.label == "radial")?.clone();
    let t = std::time::Instant::now();
    let px = crate::gesture::wheel_px();
    match tauri::WebviewWindowBuilder::from_config(app, &conf).and_then(|b| b.inner_size(px, px).build()) {
        Ok(w) => {
            crate::logx::log(format!("ui: janela da roda criada em {} ms ({px}px)", t.elapsed().as_millis()));
            Some(w)
        }
        Err(e) => {
            crate::logx::log(format!("ui: falha ao criar a roda: {e}"));
            None
        }
    }
}

fn finish_create(app: &AppHandle) -> Option<WebviewWindow> {
    let w = build_window(app);
    CREATING.store(false, Ordering::SeqCst);
    if w.is_some() {
        crate::gesture::poke();
    }
    w
}

/// Começa a criar a roda sem bloquear. Não faz nada se ela já existe ou já está sendo criada.
pub fn warm(app: &AppHandle) {
    if app.get_webview_window("radial").is_some() || CREATING.load(Ordering::SeqCst) {
        return;
    }
    if CREATING.swap(true, Ordering::SeqCst) {
        return;
    }
    if app.get_webview_window("radial").is_some() {
        CREATING.store(false, Ordering::SeqCst);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = finish_create(&app);
    });
}

/// Janela já pronta. Se ainda não existe, dispara a criação e devolve `None`.
pub fn try_window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = bump_existing(app) {
        return Some(w);
    }
    warm(app);
    None
}

/// Bloqueia até a janela existir (bandeja e preview). Não chamar da thread principal.
pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = bump_existing(app) {
        return Some(w);
    }
    if !CREATING.swap(true, Ordering::SeqCst) {
        return finish_create(app);
    }
    for _ in 0..1200 {
        if let Some(w) = bump_existing(app) {
            return Some(w);
        }
        if !CREATING.load(Ordering::SeqCst) {
            return bump_existing(app);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    crate::logx::log("ui: tempo esgotado esperando a roda");
    None
}

/// A roda escondida permanece carregada. Destruir a última janela fazia o
/// processo sair (sumia da bandeja) e o próximo gesto esperava o WebView2 de novo.
pub fn schedule_unload(_app: &AppHandle) {}

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
            .inner_size(420.0, 244.0)
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
