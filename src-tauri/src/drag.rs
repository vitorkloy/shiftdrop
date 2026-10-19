//! Encaminha os eventos de arrastar (OLE IDropTarget do wry, que lê CF_HDROP já no
//! DragEnter) para o JS como um evento próprio, `sd-drag`.
//!
//! Causa do anel vazio: o Tauri emite os eventos nativos de drag com alvo "janela",
//! e o listener `getCurrentWebview().onDragDropEvent` (alvo "webview") não os recebia.
//! Aqui pegamos o evento direto no Rust (janela ou webview, com deduplicação) e
//! mandamos os caminhos já no Enter.
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, DragDropEvent, Emitter};

struct Last {
    sig: String,
    at: Instant,
    enter_paths: Vec<PathBuf>,
}
static LAST: Mutex<Option<Last>> = Mutex::new(None);

pub fn log(msg: &str) {
    if std::env::var_os("SHIFTDROP_LOG").is_none() && !crate::selftest::active() {
        return;
    }
    use std::io::Write;
    let path = std::env::temp_dir().join("shiftdrop-drag.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{:?} {msg}", std::time::SystemTime::now());
    }
}

fn s(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

pub fn forward(app: &AppHandle, label: &str, ev: &DragDropEvent) {
    if label != "radial" {
        return;
    }
    let (kind, paths, pos) = match ev {
        DragDropEvent::Enter { paths, position } => ("enter", paths.clone(), Some(*position)),
        DragDropEvent::Over { position } => ("over", vec![], Some(*position)),
        DragDropEvent::Drop { paths, position } => ("drop", paths.clone(), Some(*position)),
        DragDropEvent::Leave => ("leave", vec![], None),
        _ => return,
    };
    let sig = format!("{kind}|{paths:?}|{pos:?}");
    let mut guard = LAST.lock().unwrap();
    // O mesmo evento pode chegar como WindowEvent e como WebviewEvent: ignora a cópia.
    let window = if kind == "drop" { Duration::from_millis(800) } else { Duration::from_millis(60) };
    if let Some(l) = guard.as_ref() {
        if l.sig == sig && l.at.elapsed() < window {
            return;
        }
    }
    let mut paths = paths;
    let enter_paths = match kind {
        "enter" => paths.clone(),
        _ => guard.as_ref().map(|l| l.enter_paths.clone()).unwrap_or_default(),
    };
    if kind == "drop" && paths.is_empty() {
        paths = enter_paths.clone();
    }
    *guard = Some(Last { sig, at: Instant::now(), enter_paths });
    drop(guard);

    log(&format!("{kind} paths={:?} pos={:?}", s(&paths), pos));
    let _ = app.emit(
        "sd-drag",
        json!({
            "type": kind,
            "paths": s(&paths),
            "position": pos.map(|p| json!({ "x": p.x, "y": p.y })),
        }),
    );
}
