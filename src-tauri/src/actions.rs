//! Catálogo de ações por tipo de arquivo (o que aparece no menu radial).
use crate::tools::Tools;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Pdf,
    Doc,
    Other,
}

/// Formatos lidos nativamente (crate `image`), sem ferramenta externa.
pub const NATIVE_IMAGE: &[&str] = &["png", "jpg", "jpeg", "jfif", "webp", "bmp", "gif", "tif", "tiff", "ico"];
/// Formatos que precisam do ImageMagick para leitura.
pub const MAGICK_IMAGE: &[&str] = &["heic", "heif", "avif", "svg", "psd", "tga", "jxl", "dds", "cr2", "nef", "arw", "dng"];
pub const VIDEO: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpeg", "mpg", "3gp", "ts"];
pub const AUDIO: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac", "opus", "wma", "aiff", "aif"];
pub const DOC: &[&str] = &["doc", "docx", "odt", "rtf", "ppt", "pptx", "odp", "xls", "xlsx", "ods", "txt"];

pub fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn norm_ext(e: &str) -> &str {
    match e {
        "jpeg" | "jfif" => "jpg",
        "tif" => "tiff",
        other => other,
    }
}

pub fn kind_of(p: &Path) -> Kind {
    let e = ext_of(p);
    let e = e.as_str();
    if NATIVE_IMAGE.contains(&e) || MAGICK_IMAGE.contains(&e) {
        Kind::Image
    } else if VIDEO.contains(&e) {
        Kind::Video
    } else if AUDIO.contains(&e) {
        Kind::Audio
    } else if e == "pdf" {
        Kind::Pdf
    } else if DOC.contains(&e) {
        Kind::Doc
    } else {
        Kind::Other
    }
}

/// Tipo comum a todos os arquivos (None se forem de tipos diferentes).
pub fn common_kind(paths: &[PathBuf]) -> Option<Kind> {
    let mut it = paths.iter().map(|p| kind_of(p));
    let first = it.next()?;
    if it.all(|k| k == first) {
        Some(first)
    } else {
        None
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ActionInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub available: bool,
    pub reason: Option<String>,
}

pub fn list_actions(paths: &[PathBuf], tools: &Tools) -> Vec<ActionInfo> {
    let Some(kind) = common_kind(paths) else { return vec![] };
    let multi = paths.len() > 1;
    let exts: Vec<String> = paths.iter().map(|p| norm_ext(&ext_of(p)).to_string()).collect();
    let all_ext = |e: &str| exts.iter().all(|x| x == e);
    let gate = |ok: bool, msg: &str| if ok { (true, None) } else { (false, Some(msg.to_string())) };
    let mut v = Vec::new();
    let mut push = |id, label, icon, (available, reason): (bool, Option<String>)| {
        v.push(ActionInfo { id, label, icon, available, reason })
    };

    match kind {
        Kind::Image => {
            let native = paths.iter().all(|p| NATIVE_IMAGE.contains(&ext_of(p).as_str()));
            let g = gate(native || tools.magick.is_some(), "Requer ImageMagick");
            for (id, label, icon, e) in [
                ("to_png", "PNG", "🖼️", "png"),
                ("to_jpg", "JPG", "📷", "jpg"),
                ("to_webp", "WEBP", "🌐", "webp"),
            ] {
                if !all_ext(e) {
                    push(id, label, icon, g.clone());
                }
            }
            push("to_pdf", if multi { "Juntar PDF" } else { "PDF" }, "📄", g.clone());
            push("compress_image", "Comprimir", "🗜️", g.clone());
            push("crop_square", "Quadrado", "⬛", g.clone());
            push("resize_half", "50%", "↘️", g);
        }
        Kind::Video => {
            let g = gate(tools.ffmpeg.is_some(), "Requer ffmpeg");
            for (id, label, icon, e) in [
                ("to_mp4", "MP4", "🎬", "mp4"),
                ("to_webm", "WEBM", "🌐", "webm"),
                ("to_gif", "GIF", "🎞️", "gif"),
            ] {
                if !all_ext(e) {
                    push(id, label, icon, g.clone());
                }
            }
            push("extract_mp3", "MP3", "🎵", g.clone());
            push("compress_video", "Comprimir", "🗜️", g.clone());
            if multi {
                push("merge_video", "Juntar", "🔗", g);
            }
        }
        Kind::Audio => {
            let g = gate(tools.ffmpeg.is_some(), "Requer ffmpeg");
            for (id, label, icon, e) in [
                ("to_mp3", "MP3", "🎵", "mp3"),
                ("to_wav", "WAV", "🎧", "wav"),
                ("to_ogg", "OGG", "🔊", "ogg"),
            ] {
                if !all_ext(e) {
                    push(id, label, icon, g.clone());
                }
            }
            push("compress_audio", "Comprimir", "🗜️", g.clone());
            if multi {
                push("merge_audio", "Juntar", "🔗", g);
            }
        }
        Kind::Doc => {
            push("doc_to_pdf", "PDF", "📄", gate(tools.soffice.is_some(), "Requer LibreOffice"));
        }
        Kind::Pdf | Kind::Other => {}
    }
    v
}
