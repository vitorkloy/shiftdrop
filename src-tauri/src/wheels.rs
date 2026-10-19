//! As três rodas do estilo Tangerine:
//! Shift = "Convert formats", Ctrl = "File tools", Alt = "File actions".
//! Listas por tipo copiadas do site oficial (tangerineformac.com). Ids:
//! `conv:<formato>`, `tool:<nome>` e `act:<nome>`. O que ainda não existe no
//! Windows aparece desabilitado com o motivo.
use crate::actions::ext_of;
use crate::convert::{self as cv, os, R};
use crate::tools::{run_tool, Tools};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct WheelItem {
    pub id: String,
    /// Rótulo em CAPS (como no Tangerine).
    pub title: String,
    /// Nome do ícone em ui/icons (só nas rodas de ferramentas e ações).
    pub icon: Option<&'static str>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Wheels {
    /// Formato atual (ex. "JPG"), mostrado no centro.
    pub format: String,
    pub kind: &'static str,
    pub convert: Vec<WheelItem>,
    pub tools: Vec<WheelItem>,
    pub actions: Vec<WheelItem>,
}

const IMAGE: &[&str] = &["jpg", "png", "webp", "heic", "tiff", "svg", "avif", "bmp", "ico", "psd", "tga", "jxl"];
const AUDIO: &[&str] = &["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "wma", "aac"];
const VIDEO: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "wmv", "m4v", "flv", "mpg", "3gp", "ts"];
const DOCS: &[&str] = &["docx", "doc", "odt", "pages", "md", "rtf", "html"];
const SLIDES: &[&str] = &["ppt", "pptx", "odp", "key"];
const SHEETS: &[&str] = &["xlsx", "xls", "ods", "csv", "tsv"];
const SUBS: &[&str] = &["srt", "vtt"];
const ARCHIVES: &[&str] = &["zip", "tar", "gz", "rar", "7z"];

/// Extensão normalizada (jpeg→jpg, tif→tiff, tgz→gz…).
pub fn fmt_of(p: &Path) -> String {
    let e = ext_of(p);
    match e.as_str() {
        "jpeg" | "jfif" => "jpg".into(),
        "tif" => "tiff".into(),
        "heif" => "heic".into(),
        "aif" => "aiff".into(),
        "tgz" => "gz".into(),
        "markdown" => "md".into(),
        "htm" => "html".into(),
        _ => e,
    }
}

pub fn kind_of(fmt: &str) -> &'static str {
    let f = fmt;
    if f == "gif" {
        "gif"
    } else if IMAGE.contains(&f) {
        "image"
    } else if AUDIO.contains(&f) {
        "audio"
    } else if VIDEO.contains(&f) {
        "video"
    } else if f == "pdf" {
        "pdf"
    } else if f == "txt" {
        "txt"
    } else if f == "epub" {
        "epub"
    } else if DOCS.contains(&f) {
        "doc"
    } else if SLIDES.contains(&f) {
        "slides"
    } else if SHEETS.contains(&f) {
        "sheet"
    } else if SUBS.contains(&f) {
        "subtitle"
    } else if ARCHIVES.contains(&f) {
        "archive"
    } else {
        "other"
    }
}

fn title_of(fmt: &str) -> String {
    if fmt == "gz" { "GZIP".into() } else { fmt.to_uppercase() }
}

/// Alvos de conversão por tipo (igual à função `z()` do site).
fn targets(kind: &str, fmt: &str) -> Vec<&'static str> {
    let t: Vec<&'static str> = match kind {
        "image" => vec!["jpg", "png", "webp", "heic", "tiff", "avif", "bmp", "pdf"],
        "audio" => vec!["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "wma"],
        "video" => vec!["mp4", "mov", "mkv", "webm", "avi", "wmv", "gif", "mp3"],
        "gif" => vec!["mp4", "mov", "mkv", "webm", "avi", "wmv"],
        "pdf" => vec!["docx", "pages", "pptx", "xlsx", "jpg", "png", "epub", "txt", "md"],
        "slides" => vec!["pdf", "jpg", "png"],
        "sheet" => vec!["xlsx", "ods", "csv", "tsv", "pdf"],
        "doc" => vec!["docx", "pages", "pdf", "txt", "md", "html", "epub", "png", "jpg"],
        "epub" => vec!["pdf", "docx", "pages", "txt", "md", "html"],
        "txt" => vec!["docx", "pages", "pdf", "md", "html", "epub", "jpg", "png"],
        "subtitle" => vec!["srt", "vtt", "txt"],
        "archive" => vec!["zip", "tar", "gz"],
        _ => vec![],
    };
    t.into_iter().filter(|x| *x != fmt).collect()
}

fn tool_title(id: &str) -> &'static str {
    match id {
        "compress" => "COMPRESS",
        "removeMetadata" => "METADATA",
        "editImage" => "EDIT",
        "annotateImage" => "ANNOTATE",
        "frameImage" => "ADD BG",
        "cropImage" | "cropVideo" => "CROP",
        "redactImage" | "redactVideo" => "REDACT",
        "normalizeAudio" => "NORMALIZE",
        "muteVideo" => "MUTE",
        "trimVideo" | "trimAudio" => "TRIM",
        "splitPDF" | "splitVideo" => "SPLIT",
        "organizePDF" => "ORGANIZE",
        "extractArchive" => "EXTRACT",
        "createPDF" => "CREATE PDF",
        "mergePDF" => "MERGE PDF",
        "changeVideoSpeed" => "SPEED",
        "joinVideos" => "JOIN",
        "videoSnapshots" => "SNAPSHOT",
        "audioToVideo" => "VISUALIZE",
        "audioChannels" => "CHANNELS",
        "createCollage" => "COLLAGE",
        "redactAudio" => "BLEEP",
        "airDrop" => "SHARE",
        "float" => "FLOAT",
        "copy" => "COPY",
        "move" => "MOVE",
        "copyPath" => "COPY PATH",
        "openTerminal" => "TERMINAL",
        _ => "?",
    }
}

/// Ferramentas por tipo (igual à função `et()` do site; com vários arquivos só as de lote).
fn tool_ids(kind: &str, fmt: &str, multi: bool) -> Vec<&'static str> {
    match kind {
        "image" if fmt == "svg" => if multi { vec!["createPDF", "createCollage"] } else { vec![] },
        "image" => {
            if multi {
                vec!["compress", "removeMetadata", "cropImage", "createPDF", "createCollage"]
            } else {
                vec!["compress", "removeMetadata", "editImage", "annotateImage", "frameImage", "cropImage", "redactImage"]
            }
        }
        "audio" => {
            if multi {
                vec!["compress", "removeMetadata", "normalizeAudio"]
            } else {
                vec!["compress", "removeMetadata", "normalizeAudio", "audioToVideo", "trimAudio", "audioChannels", "redactAudio"]
            }
        }
        "video" => {
            if multi {
                vec!["compress", "removeMetadata", "muteVideo", "joinVideos"]
            } else {
                vec!["compress", "removeMetadata", "muteVideo", "trimVideo", "cropVideo", "changeVideoSpeed", "videoSnapshots", "splitVideo", "redactVideo"]
            }
        }
        "gif" => vec!["removeMetadata"],
        "pdf" => if multi { vec!["compress", "removeMetadata", "splitPDF", "mergePDF"] } else { vec!["compress", "removeMetadata", "splitPDF", "organizePDF"] },
        "archive" => vec!["extractArchive"],
        _ => vec![],
    }
}

const ACTION_IDS: &[&str] = &["airDrop", "float", "copy", "move", "copyPath", "openTerminal"];

fn ok() -> (bool, Option<String>) {
    (true, None)
}
fn no(msg: &str) -> (bool, Option<String>) {
    (false, Some(msg.to_string()))
}
fn need(t: &Option<PathBuf>, msg: &str) -> (bool, Option<String>) {
    if t.is_some() { ok() } else { no(msg) }
}

const SOON: &str = "Em breve no ShiftDrop";
const NO_PAGES: &str = "Pages é formato da Apple (não existe no Windows)";

fn conv_gate(kind: &str, from: &str, to: &str, t: &Tools) -> (bool, Option<String>) {
    if to == "pages" {
        return no(NO_PAGES);
    }
    match kind {
        "image" => {
            let native_in = cv::NATIVE_READ.contains(&from);
            let native_out = matches!(to, "png" | "jpg" | "webp" | "tiff" | "bmp" | "pdf");
            if (native_in || t.magick.is_some()) && (native_out || t.magick.is_some()) { ok() } else { no("Requer ImageMagick") }
        }
        "audio" | "video" | "gif" => need(&t.ffmpeg, "Requer ffmpeg"),
        "pdf" => match to {
            "jpg" | "png" => need(&t.gs, "Requer Ghostscript"),
            "docx" | "txt" => need(&t.soffice, "Requer LibreOffice"),
            _ => no(SOON),
        },
        "doc" | "txt" | "slides" | "sheet" => match to {
            "md" => need(&t.pandoc, "Requer pandoc"),
            _ if from == "md" => need(&t.pandoc, "Requer pandoc"),
            _ => need(&t.soffice, "Requer LibreOffice"),
        },
        "epub" => match to {
            "pdf" => no(SOON),
            _ => need(&t.pandoc, "Requer pandoc"),
        },
        "subtitle" => ok(),
        "archive" => if from == "rar" || from == "7z" { need(&t.tar, "Requer tar.exe") } else { need(&t.tar, "Requer tar.exe (Windows 10+)") },
        _ => no("Não suportado"),
    }
}

fn tool_gate(id: &str, kind: &str, t: &Tools) -> (bool, Option<String>) {
    match id {
        "compress" => match kind {
            "image" => ok(),
            "pdf" => need(&t.gs, "Requer Ghostscript"),
            _ => need(&t.ffmpeg, "Requer ffmpeg"),
        },
        "removeMetadata" => match kind {
            "image" => ok(),
            "pdf" => no(SOON),
            _ => need(&t.ffmpeg, "Requer ffmpeg"),
        },
        "muteVideo" | "changeVideoSpeed" | "videoSnapshots" | "splitVideo" | "normalizeAudio" | "audioToVideo"
        | "audioChannels" | "joinVideos" => need(&t.ffmpeg, "Requer ffmpeg"),
        "createPDF" => ok(),
        "extractArchive" => need(&t.tar, "Requer tar.exe (Windows 10+)"),
        "copy" | "move" | "copyPath" | "openTerminal" => ok(),
        _ => no(SOON),
    }
}

fn item(id: String, title: String, icon: Option<&'static str>, g: (bool, Option<String>)) -> WheelItem {
    WheelItem { id, title, icon, available: g.0, reason: g.1 }
}

pub fn action_items() -> Vec<WheelItem> {
    ACTION_IDS
        .iter()
        .map(|a| item(format!("act:{a}"), tool_title(a).into(), Some(*a), tool_gate(a, "", &Tools::default())))
        .collect()
}

pub fn wheels(paths: &[PathBuf], t: &Tools) -> Wheels {
    if paths.is_empty() {
        // Ainda sem caminho: setores genéricos soltáveis (resolvidos no drop).
        let conv = ["png", "jpg", "webp", "pdf", "mp4", "mp3"];
        return Wheels {
            format: String::new(),
            kind: "unknown",
            convert: conv.iter().map(|f| item(format!("conv:{f}"), title_of(f), None, ok())).collect(),
            tools: ["compress", "removeMetadata"]
                .iter()
                .map(|id| item(format!("tool:{id}"), tool_title(id).into(), Some(*id), ok()))
                .collect(),
            actions: action_items(),
        };
    }
    let fmts: Vec<String> = paths.iter().map(|p| if p.is_dir() { "folder".into() } else { fmt_of(p) }).collect();
    let kinds: Vec<&str> = fmts.iter().map(|f| if f == "folder" { "folder" } else { kind_of(f) }).collect();
    let multi = paths.len() > 1;
    let same_kind = kinds.iter().all(|k| *k == kinds[0]);
    let same_fmt = fmts.iter().all(|f| *f == fmts[0]);
    let kind = if same_kind { kinds[0] } else { "mixed" };
    let fmt = if same_fmt { fmts[0].clone() } else { String::new() };
    let format = if paths.len() == 1 {
        if kind == "folder" { "PASTA".into() } else { title_of(&fmt) }
    } else {
        format!("{} ITENS", paths.len())
    };
    let convert = if same_kind {
        targets(kind, &fmt)
            .into_iter()
            .map(|to| {
                let g = fmts.iter().map(|f| conv_gate(kind, f, to, t)).find(|g| !g.0).unwrap_or_else(ok);
                let title = if to == "pdf" && multi && kind == "image" { "PDF".into() } else { title_of(to) };
                item(format!("conv:{to}"), title, None, g)
            })
            .collect()
    } else {
        vec![]
    };
    let tools = if same_kind {
        tool_ids(kind, &fmt, multi)
            .into_iter()
            .map(|id| item(format!("tool:{id}"), tool_title(id).into(), Some(id), tool_gate(id, kind, t)))
            .collect()
    } else {
        vec![]
    };
    Wheels { format, kind, convert, tools, actions: action_items() }
}

// ---------------------------------------------------------------- execução

pub fn run(id: &str, paths: &[PathBuf], t: &Tools) -> R<Vec<PathBuf>> {
    if paths.is_empty() {
        return Err("nenhum arquivo".into());
    }
    if let Some(a) = id.strip_prefix("act:") {
        for p in paths {
            if !p.exists() {
                return Err(format!("não encontrado: {}", p.display()));
            }
        }
        return crate::fileactions::run(a, paths);
    }
    for p in paths {
        if !p.is_file() {
            return Err(format!("arquivo não encontrado: {}", p.display()));
        }
    }
    let w = wheels(paths, t);
    let it = w
        .convert
        .iter()
        .chain(w.tools.iter())
        .find(|i| i.id == id)
        .ok_or_else(|| format!("'{id}' não se aplica a {}", if w.format.is_empty() { "esses arquivos".into() } else { w.format.clone() }))?;
    if !it.available {
        return Err(it.reason.clone().unwrap_or_else(|| "indisponível".into()));
    }
    if let Some(to) = id.strip_prefix("conv:") {
        return convert_to(w.kind, to, paths, t);
    }
    let tool = id.strip_prefix("tool:").unwrap_or(id);
    run_tool_action(tool, w.kind, paths, t)
}

fn convert_to(kind: &str, to: &str, paths: &[PathBuf], t: &Tools) -> R<Vec<PathBuf>> {
    match kind {
        "image" => {
            if to == "pdf" {
                return cv::images_to_pdf(paths, t).map(|p| vec![p]);
            }
            cv::each(paths, |p| match to {
                "png" | "jpg" | "webp" | "tiff" | "bmp" => cv::convert_image(p, to, t),
                _ => magick_convert(p, to, t),
            })
        }
        "audio" | "video" | "gif" => cv::each(paths, |p| {
            let opts: &[&str] = match to {
                "mp4" => cv::MP4,
                "mov" => cv::MP4,
                "mkv" => &["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-c:a", "aac", "-b:a", "160k"],
                "webm" => cv::WEBM,
                "avi" => &["-c:v", "mpeg4", "-q:v", "4", "-c:a", "libmp3lame", "-q:a", "3"],
                "wmv" => &["-c:v", "wmv2", "-b:v", "2500k", "-c:a", "wmav2", "-b:a", "160k"],
                "gif" => cv::GIF,
                "mp3" => cv::MP3,
                "m4a" => &["-vn", "-c:a", "aac", "-b:a", "192k"],
                "wav" => cv::WAV,
                "flac" => &["-vn", "-c:a", "flac"],
                "ogg" => cv::OGG,
                "opus" => &["-vn", "-c:a", "libopus", "-b:a", "128k"],
                "aiff" => &["-vn", "-c:a", "pcm_s16be"],
                "wma" => &["-vn", "-c:a", "wmav2", "-b:a", "192k"],
                _ => return Err(format!("formato não suportado: {to}")),
            };
            cv::ff_one(t, p, "", to, opts)
        }),
        "subtitle" => cv::each(paths, |p| subtitle_convert(p, to)),
        "archive" => cv::each(paths, |p| archive_convert(p, to, t)),
        "doc" | "txt" | "slides" | "sheet" | "pdf" | "epub" => cv::each(paths, |p| doc_convert(p, to, t)),
        _ => Err("tipo não suportado".into()),
    }
}

fn magick_convert(p: &Path, to: &str, t: &Tools) -> R<PathBuf> {
    let m = t.magick.as_ref().ok_or("Requer ImageMagick")?;
    let out = cv::unique_out(p, "", to);
    let mut src = os(p);
    src.push("[0]");
    run_tool(m, &[src, os("-quality"), os("85"), os(&out)])?;
    if out.is_file() { Ok(out) } else { Err("ImageMagick não gerou o arquivo".into()) }
}

fn doc_convert(p: &Path, to: &str, t: &Tools) -> R<PathBuf> {
    let from = fmt_of(p);
    if to == "md" || from == "md" || from == "epub" {
        let pd = t.pandoc.as_ref().ok_or("Requer pandoc")?;
        let out = cv::unique_out(p, "", to);
        // pandoc não lê .doc/.rtf antigos: passa antes pelo LibreOffice (→ .docx temporário).
        let mut tmp: Option<PathBuf> = None;
        let src = if matches!(from.as_str(), "doc" | "rtf" | "pages" | "txt") && from != "txt" {
            let d = cv::temp_file("lo-docx");
            std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            let copy = d.join(format!("{}.{}", stem(p), from));
            std::fs::copy(p, &copy).map_err(|e| e.to_string())?;
            let docx = cv::soffice_convert(&copy, None, "docx", "docx", t)?;
            tmp = Some(d);
            docx
        } else {
            p.to_path_buf()
        };
        let mut args = vec![os(&src), os("-o"), os(&out)];
        if from == "txt" {
            args.splice(0..0, [os("-f"), os("markdown")]);
        }
        let r = run_tool(pd, &args);
        if let Some(d) = tmp {
            let _ = std::fs::remove_dir_all(d);
        }
        r?;
        return if out.is_file() { Ok(out) } else { Err("pandoc não gerou o arquivo".into()) };
    }
    let filter = match to {
        "tsv" => "tsv:Text - txt - csv (StarCalc):9,34,76",
        "txt" => "txt:Text",
        other => other,
    };
    if from == "pdf" {
        if to == "jpg" || to == "png" {
            return pdf_to_images(p, to, t);
        }
        // PDF → Writer (texto editável).
        let f = if to == "txt" { "txt:Text" } else { "docx:MS Word 2007 XML" };
        return cv::soffice_convert(p, Some("writer_pdf_import"), f, to, t);
    }
    cv::soffice_convert(p, None, filter, to, t)
}

/// PDF → uma imagem por página (Ghostscript, 200 dpi): `nome-1.png`, `nome-2.png`…
fn pdf_to_images(p: &Path, to: &str, t: &Tools) -> R<PathBuf> {
    let gs = t.gs.as_ref().ok_or("Requer Ghostscript")?;
    let dir = p.parent().unwrap_or(Path::new("."));
    let mut base = stem(p);
    let mut n = 1;
    while dir.join(format!("{base}-1.{to}")).exists() {
        base = format!("{} ({n})", stem(p));
        n += 1;
    }
    let pattern = dir.join(format!("{base}-%d.{to}"));
    let dev = if to == "jpg" { "jpeg" } else { "png16m" };
    run_tool(gs, &[os("-dSAFER"), os("-dBATCH"), os("-dNOPAUSE"), os("-dQUIET"), os(format!("-sDEVICE={dev}")), os("-r200"), os("-dJPEGQ=90"), os("-o"), os(&pattern), os(p)])?;
    let first = dir.join(format!("{base}-1.{to}"));
    if first.is_file() { Ok(first) } else { Err("Ghostscript não gerou imagens".into()) }
}

fn compress_pdf(p: &Path, t: &Tools) -> R<PathBuf> {
    let gs = t.gs.as_ref().ok_or("Requer Ghostscript")?;
    let out = cv::unique_out(p, "-comprimido", "pdf");
    run_tool(gs, &[os("-dSAFER"), os("-dBATCH"), os("-dNOPAUSE"), os("-dQUIET"), os("-sDEVICE=pdfwrite"), os("-dCompatibilityLevel=1.5"), os("-dPDFSETTINGS=/ebook"), os("-o"), os(&out), os(p)])?;
    if out.is_file() { Ok(out) } else { Err("Ghostscript não gerou o PDF".into()) }
}

fn subtitle_convert(p: &Path, to: &str) -> R<PathBuf> {
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let blocks: Vec<&str> = text.split("\n\n").map(|b| b.trim()).filter(|b| !b.is_empty() && *b != "WEBVTT").collect();
    let mut cues: Vec<(String, String)> = Vec::new();
    for b in blocks {
        let lines: Vec<&str> = b.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let times = lines[ti].replace(',', ".");
        let body = lines[ti + 1..].join("\n");
        cues.push((times, body));
    }
    let out_text = match to {
        "vtt" => {
            let mut s = String::from("WEBVTT\n\n");
            for (tm, b) in &cues {
                s += &format!("{tm}\n{b}\n\n");
            }
            s
        }
        "srt" => {
            let mut s = String::new();
            for (i, (tm, b)) in cues.iter().enumerate() {
                let tm: String = tm.split_whitespace().take(3).collect::<Vec<_>>().join(" ").replace('.', ",");
                s += &format!("{}\n{tm}\n{b}\n\n", i + 1);
            }
            s
        }
        _ => cues.iter().map(|(_, b)| b.as_str()).collect::<Vec<_>>().join("\n") + "\n",
    };
    let out = cv::unique_out(p, "", to);
    std::fs::write(&out, out_text).map_err(|e| e.to_string())?;
    Ok(out)
}

fn stem(p: &Path) -> String {
    let n = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let l = n.to_ascii_lowercase();
    for suf in [".tar.gz", ".tgz", ".tar", ".zip", ".gz", ".rar", ".7z"] {
        if l.ends_with(suf) {
            return n[..n.len() - suf.len()].to_string();
        }
    }
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let mut d = parent.join(name);
    let mut n = 1;
    while d.exists() {
        d = parent.join(format!("{name} ({n})"));
        n += 1;
    }
    d
}

fn extract(p: &Path, t: &Tools) -> R<PathBuf> {
    let tar = t.tar.as_ref().ok_or("Requer tar.exe")?;
    let dir = unique_dir(p.parent().unwrap_or(Path::new(".")), &stem(p));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    run_tool(tar, &[os("-xf"), os(p), os("-C"), os(&dir)]).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&dir);
    })?;
    Ok(dir)
}

fn archive_convert(p: &Path, to: &str, t: &Tools) -> R<PathBuf> {
    let tar = t.tar.as_ref().ok_or("Requer tar.exe")?;
    let tmp = cv::temp_file("x");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let r = (|| {
        run_tool(tar, &[os("-xf"), os(p), os("-C"), os(&tmp)])?;
        let dir = p.parent().unwrap_or(Path::new("."));
        let (ext, flags) = match to {
            "zip" => ("zip", "-a -cf"),
            "gz" => ("tar.gz", "-czf"),
            _ => ("tar", "-cf"),
        };
        let out = cv::unique_path(dir, &stem(p), ext);
        let mut args: Vec<_> = flags.split(' ').map(os).collect();
        args.extend([os(&out), os("-C"), os(&tmp), os(".")]);
        run_tool(tar, &args)?;
        if out.is_file() { Ok(out) } else { Err("tar não gerou o arquivo".to_string()) }
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

fn run_tool_action(tool: &str, kind: &str, paths: &[PathBuf], t: &Tools) -> R<Vec<PathBuf>> {
    let ff = |p: &Path, suffix: &str, ext: &str, opts: &[&str]| cv::ff_one(t, p, suffix, ext, opts);
    match (tool, kind) {
        ("compress", "image") => crate::convert::run_action("compress_image", paths, t),
        ("compress", "video") => crate::convert::run_action("compress_video", paths, t),
        ("compress", "audio") => crate::convert::run_action("compress_audio", paths, t),
        ("compress", "pdf") => cv::each(paths, |p| compress_pdf(p, t)),
        ("removeMetadata", "image") => cv::each(paths, |p| cv::strip_image_metadata(p, t)),
        ("removeMetadata", _) => cv::each(paths, |p| {
            let e = fmt_of(p);
            ff(p, "-sem-metadados", &e, &["-map", "0", "-map_metadata", "-1", "-map_chapters", "-1", "-c", "copy"])
        }),
        ("muteVideo", _) => cv::each(paths, |p| ff(p, "-mudo", &fmt_of(p), &["-an", "-c:v", "copy"])),
        ("changeVideoSpeed", _) => cv::each(paths, |p| {
            let a = cv::has_audio(t, p);
            let mut o = vec!["-filter:v", "setpts=0.5*PTS", "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p"];
            if a { o.extend(["-filter:a", "atempo=2.0", "-c:a", "aac"]); } else { o.push("-an"); }
            ff(p, "-2x", "mp4", &o)
        }),
        ("videoSnapshots", _) => cv::each(paths, |p| {
            let d = cv::probe_duration(t, p).unwrap_or(2.0);
            let out = cv::unique_out(p, "-snapshot", "jpg");
            let mut a = cv::ff_base();
            a.extend([os("-ss"), os(format!("{:.2}", d / 2.0)), os("-i"), os(p), os("-frames:v"), os("1"), os("-q:v"), os("2"), os(&out)]);
            cv::ffmpeg_run(t, a, &out)
        }),
        ("splitVideo", _) => {
            let p = &paths[0];
            let d = cv::probe_duration(t, p).ok_or("não consegui ler a duração")?;
            let e = fmt_of(p);
            let half = format!("{:.3}", d / 2.0);
            let a = ff(p, "-parte1", &e, &["-t", &half, "-c", "copy"])?;
            let b = {
                let out = cv::unique_out(p, "-parte2", &e);
                let mut args = cv::ff_base();
                args.extend([os("-ss"), os(&half), os("-i"), os(p), os("-c"), os("copy"), os(&out)]);
                cv::ffmpeg_run(t, args, &out)?
            };
            Ok(vec![a, b])
        }
        ("joinVideos", _) => crate::convert::run_action("merge_video", paths, t),
        ("normalizeAudio", _) => cv::each(paths, |p| {
            let e = fmt_of(p);
            ff(p, "-normalizado", &e, &["-vn", "-af", "loudnorm=I=-16:TP=-1.5:LRA=11"])
        }),
        ("audioToVideo", _) => cv::each(paths, |p| {
            ff(p, "-visualizer", "mp4", &[
                "-filter_complex",
                "[0:a]showwaves=s=1280x720:mode=cline:rate=30:colors=0xFF5200,format=yuv420p[v]",
                "-map", "[v]", "-map", "0:a", "-c:v", "libx264", "-preset", "veryfast", "-crf", "23",
                "-c:a", "aac", "-b:a", "160k", "-shortest",
            ])
        }),
        ("audioChannels", _) => cv::each(paths, |p| {
            let stereo = cv::audio_channels(t, p).unwrap_or(2) >= 2;
            let e = fmt_of(p);
            if stereo { ff(p, "-mono", &e, &["-vn", "-ac", "1"]) } else { ff(p, "-estereo", &e, &["-vn", "-ac", "2"]) }
        }),
        ("createPDF", _) => cv::images_to_pdf(paths, t).map(|p| vec![p]),
        ("extractArchive", _) => cv::each(paths, |p| extract(p, t)),
        _ => Err(SOON.into()),
    }
}
