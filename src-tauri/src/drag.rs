//! Eventos de arrastar → JS (`sd-drag`). Origem principal: o alvo de drop nativo
//! do ShiftDrop (catcher.rs). Os eventos do Tauri/wry também são aceitos (com
//! deduplicação), mas no gesto real o WebView2 não os entregava ao JS.
use crate::logx::log;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, DragDropEvent};

struct Last {
    sig: String,
    at: Instant,
    enter_paths: Vec<PathBuf>,
}
static LAST: Mutex<Option<Last>> = Mutex::new(None);

fn s(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

/// Encaminha um evento (kind = enter/over/leave/drop; pos em px físicos relativos à janela).
pub fn dispatch(app: &AppHandle, source: &str, kind: &str, paths: Vec<PathBuf>, pos: Option<(f64, f64)>) {
    let sig = format!("{kind}|{paths:?}|{pos:?}");
    let mut guard = LAST.lock().unwrap();
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

    if kind != "over" {
        log(format!("drag[{source}] {kind} arquivos={:?} pos={:?}", s(&paths), pos));
    }
    crate::ui::emit(
        app,
        "sd-drag",
        json!({
            "type": kind,
            "paths": s(&paths),
            "position": pos.map(|(x, y)| json!({ "x": x, "y": y })),
        }),
    );
}

pub fn forward(app: &AppHandle, label: &str, ev: &DragDropEvent) {
    if label != "radial" {
        return;
    }
    match ev {
        DragDropEvent::Enter { paths, position } => dispatch(app, "tauri", "enter", paths.clone(), Some((position.x, position.y))),
        DragDropEvent::Over { position } => dispatch(app, "tauri", "over", vec![], Some((position.x, position.y))),
        DragDropEvent::Drop { paths, position } => dispatch(app, "tauri", "drop", paths.clone(), Some((position.x, position.y))),
        DragDropEvent::Leave => dispatch(app, "tauri", "leave", vec![], None),
        _ => {}
    }
}
