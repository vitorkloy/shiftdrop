//! Execução das ações. Toda saída vai para a mesma pasta do arquivo original,
//! sem sobrescrever nada (gera "nome (1).ext" se já existir).
use crate::actions;
use crate::pdf;
use crate::tools::{run_tool, Tools};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Rgb, RgbImage};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type R<T> = Result<T, String>;

fn es<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn run_action(id: &str, paths: &[PathBuf], tools: &Tools) -> R<Vec<PathBuf>> {
    if paths.is_empty() {
        return Err("nenhum arquivo".into());
    }
    for p in paths {
        if !p.is_file() {
            return Err(format!("arquivo não encontrado: {}", p.display()));
        }
    }
    let resolved = actions::resolve_id(id, paths);
    let id = resolved.as_str();
    let list = actions::list_actions(paths, tools);
    let a = list
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("a ação '{id}' não se aplica a esses arquivos"))?;
    if !a.available {
        return Err(a.reason.clone().unwrap_or_else(|| "indisponível".into()));
    }

    match id {
        "to_png" => each(paths, |p| convert_image(p, "png", tools)),
        "to_jpg" => each(paths, |p| convert_image(p, "jpg", tools)),
        "to_webp" => each(paths, |p| convert_image(p, "webp", tools)),
        "to_pdf" => images_to_pdf(paths, tools).map(|p| vec![p]),
        "compress_image" => each(paths, |p| {
            let img = load_image(p, tools)?;
            let img = if img.width().max(img.height()) > 2560 {
                img.resize(2560, 2560, FilterType::Lanczos3)
            } else {
                img
            };
            let out = unique_out(p, "-comprimido", "jpg");
            save_jpeg(&img, &out, 72)?;
            Ok(out)
        }),
        "crop_square" => each(paths, |p| {
            let img = load_image(p, tools)?;
            let s = img.width().min(img.height());
            let c = img.crop_imm((img.width() - s) / 2, (img.height() - s) / 2, s, s);
            let e = writable_ext(p);
            let out = unique_out(p, "-quadrado", e);
            save_image(&c, &out, e, tools)?;
            Ok(out)
        }),
        "resize_half" => each(paths, |p| {
            let img = load_image(p, tools)?;
            let r = img.resize_exact((img.width() / 2).max(1), (img.height() / 2).max(1), FilterType::Lanczos3);
            let e = writable_ext(p);
            let out = unique_out(p, "-50", e);
            save_image(&r, &out, e, tools)?;
            Ok(out)
        }),
        "to_mp4" => each(paths, |p| ff_one(tools, p, "", "mp4", MP4)),
        "to_webm" => each(paths, |p| ff_one(tools, p, "", "webm", WEBM)),
        "to_gif" => each(paths, |p| ff_one(tools, p, "", "gif", GIF)),
        "extract_mp3" => each(paths, |p| ff_one(tools, p, "", "mp3", MP3)),
        "compress_video" => each(paths, |p| ff_one(tools, p, "-comprimido", "mp4", VIDEO_COMPRESS)),
        "merge_video" => merge_video(paths, tools).map(|p| vec![p]),
        "to_mp3" => each(paths, |p| ff_one(tools, p, "", "mp3", MP3)),
        "to_wav" => each(paths, |p| ff_one(tools, p, "", "wav", WAV)),
        "to_ogg" => each(paths, |p| ff_one(tools, p, "", "ogg", OGG)),
        "compress_audio" => each(paths, |p| ff_one(tools, p, "-comprimido", "mp3", AUDIO_COMPRESS)),
        "merge_audio" => merge_audio(paths, tools).map(|p| vec![p]),
        "doc_to_pdf" => each(paths, |p| doc_to_pdf(p, tools)),
        _ => Err(format!("ação desconhecida: {id}")),
    }
}

fn each<F: FnMut(&Path) -> R<PathBuf>>(paths: &[PathBuf], mut f: F) -> R<Vec<PathBuf>> {
    paths
        .iter()
        .map(|p| f(p).map_err(|e| format!("{}: {e}", file_name(p))))
        .collect()
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------- nomes de saída ----------

pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut c = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while c.exists() {
        c = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    c
}

pub fn unique_out(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "arquivo".into());
    unique_path(dir, &format!("{stem}{suffix}"), ext)
}

fn temp_file(ext: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "shiftdrop-{}-{}-{}.{ext}",
        std::process::id(),
        nanos,
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

fn os(s: impl AsRef<OsStr>) -> OsString {
    s.as_ref().to_owned()
}

// ---------- imagens ----------

fn load_image(p: &Path, tools: &Tools) -> R<DynamicImage> {
    let native = ImageReader::open(p)
        .map_err(es)?
        .with_guessed_format()
        .map_err(es)?
        .decode();
    match native {
        Ok(img) => Ok(img),
        Err(e) => {
            let Some(magick) = &tools.magick else {
                return Err(format!("não foi possível ler a imagem ({e}); instale o ImageMagick"));
            };
            let tmp = temp_file("png");
            let mut src = os(p);
            src.push("[0]");
            let r = run_tool(magick, &[src, os(&tmp)]).and_then(|_| image::open(&tmp).map_err(es));
            let _ = std::fs::remove_file(&tmp);
            r
        }
    }
}

/// Achata transparência sobre fundo branco (para JPG/PDF).
fn flatten(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbImage::new(w, h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as u32;
        let bl = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        out.put_pixel(x, y, Rgb([bl(p[0]), bl(p[1]), bl(p[2])]));
    }
    out
}

fn save_jpeg(img: &DynamicImage, out: &Path, quality: u8) -> R<()> {
    let f = BufWriter::new(File::create(out).map_err(es)?);
    DynamicImage::ImageRgb8(flatten(img))
        .write_with_encoder(JpegEncoder::new_with_quality(f, quality))
        .map_err(es)
}

fn save_webp(img: &DynamicImage, out: &Path, tools: &Tools) -> R<()> {
    if let Some(magick) = &tools.magick {
        // ImageMagick gera WEBP com perdas (bem menor).
        let tmp = temp_file("png");
        DynamicImage::ImageRgba8(img.to_rgba8()).save_with_format(&tmp, ImageFormat::Png).map_err(es)?;
        let r = run_tool(magick, &[os(&tmp), os("-quality"), os("82"), os(out)]).map(|_| ());
        let _ = std::fs::remove_file(&tmp);
        if r.is_ok() {
            return r;
        }
    }
    let f = BufWriter::new(File::create(out).map_err(es)?);
    DynamicImage::ImageRgba8(img.to_rgba8())
        .write_with_encoder(WebPEncoder::new_lossless(f))
        .map_err(es)
}

fn save_image(img: &DynamicImage, out: &Path, ext: &str, tools: &Tools) -> R<()> {
    match ext {
        "jpg" => save_jpeg(img, out, 90),
        "webp" => save_webp(img, out, tools),
        "bmp" => DynamicImage::ImageRgba8(img.to_rgba8()).save_with_format(out, ImageFormat::Bmp).map_err(es),
        "tiff" => DynamicImage::ImageRgba8(img.to_rgba8()).save_with_format(out, ImageFormat::Tiff).map_err(es),
        _ => DynamicImage::ImageRgba8(img.to_rgba8()).save_with_format(out, ImageFormat::Png).map_err(es),
    }
}

fn writable_ext(p: &Path) -> &'static str {
    match actions::ext_of(p).as_str() {
        "jpg" | "jpeg" | "jfif" => "jpg",
        "webp" => "webp",
        "bmp" => "bmp",
        "tif" | "tiff" => "tiff",
        _ => "png",
    }
}

fn convert_image(p: &Path, ext: &str, tools: &Tools) -> R<PathBuf> {
    let img = load_image(p, tools)?;
    let out = unique_out(p, "", ext);
    save_image(&img, &out, ext, tools).inspect_err(|_| {
        let _ = std::fs::remove_file(&out);
    })?;
    Ok(out)
}

fn images_to_pdf(paths: &[PathBuf], tools: &Tools) -> R<PathBuf> {
    let mut pages = Vec::with_capacity(paths.len());
    for p in paths {
        let img = load_image(p, tools).map_err(|e| format!("{}: {e}", file_name(p)))?;
        let rgb = flatten(&img);
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgb8(rgb.clone())
            .write_with_encoder(JpegEncoder::new_with_quality(&mut jpeg, 90))
            .map_err(es)?;
        pages.push(pdf::Page { width_px: rgb.width(), height_px: rgb.height(), jpeg });
    }
    let suffix = if paths.len() > 1 { "-juntado" } else { "" };
    let out = unique_out(&paths[0], suffix, "pdf");
    pdf::write_pdf(&pages, &out).map_err(es)?;
    Ok(out)
}

/// Miniatura PNG (para o centro do menu): imagens nativamente, vídeos via ffmpeg.
pub fn thumbnail_png(p: &Path, tools: &Tools, size: u32) -> Option<Vec<u8>> {
    let img = match actions::kind_of(p) {
        actions::Kind::Image => load_image(p, tools).ok()?,
        actions::Kind::Video => {
            let ff = tools.ffmpeg.as_ref()?;
            let tmp = temp_file("png");
            let grab = |ss: &str| {
                let mut a = ff_base();
                a.extend([os("-ss"), os(ss), os("-i"), os(p), os("-frames:v"), os("1"), os("-vf"), os(format!("scale={}:-2", size * 2)), os(&tmp)]);
                run_tool(ff, &a).is_ok() && tmp.is_file()
            };
            let ok = grab("1") || grab("0");
            let r = if ok { image::open(&tmp).ok() } else { None };
            let _ = std::fs::remove_file(&tmp);
            r?
        }
        _ => return None,
    };
    let t = img.thumbnail(size, size);
    let mut buf = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(t.to_rgba8()).write_to(&mut buf, ImageFormat::Png).ok()?;
    Some(buf.into_inner())
}

// ---------- vídeo / áudio (ffmpeg) ----------

const MP4: &[&str] = &[
    "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p",
    "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart",
];
const WEBM: &[&str] = &[
    "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "33", "-deadline", "realtime", "-cpu-used", "8",
    "-row-mt", "1", "-c:a", "libopus", "-b:a", "128k",
];
const GIF: &[&str] = &[
    "-an", "-vf",
    "fps=12,scale='min(640,iw)':-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse",
    "-loop", "0",
];
const VIDEO_COMPRESS: &[&str] = &[
    "-c:v", "libx264", "-preset", "medium", "-crf", "28", "-pix_fmt", "yuv420p",
    "-vf", "scale='trunc(min(1920,iw)/2)*2':-2", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart",
];
const MP3: &[&str] = &["-vn", "-c:a", "libmp3lame", "-q:a", "2"];
const WAV: &[&str] = &["-vn", "-c:a", "pcm_s16le"];
const OGG: &[&str] = &["-vn", "-c:a", "libvorbis", "-q:a", "5"];
const AUDIO_COMPRESS: &[&str] = &["-vn", "-c:a", "libmp3lame", "-b:a", "96k"];

fn ff_base() -> Vec<OsString> {
    ["-y", "-hide_banner", "-loglevel", "error", "-nostdin"].iter().map(os).collect()
}

fn ffmpeg_run(tools: &Tools, args: Vec<OsString>, out: &Path) -> R<PathBuf> {
    let ff = tools.ffmpeg.as_ref().ok_or("ffmpeg não encontrado")?;
    match run_tool(ff, &args) {
        Ok(_) if out.is_file() => Ok(out.to_path_buf()),
        Ok(_) => Err("ffmpeg não gerou o arquivo".into()),
        Err(e) => {
            let _ = std::fs::remove_file(out);
            Err(e)
        }
    }
}

fn ff_one(tools: &Tools, p: &Path, suffix: &str, ext: &str, opts: &[&str]) -> R<PathBuf> {
    let out = unique_out(p, suffix, ext);
    let mut args = ff_base();
    args.push(os("-i"));
    args.push(os(p));
    args.extend(opts.iter().map(os));
    args.push(os(&out));
    ffmpeg_run(tools, args, &out)
}

fn ffprobe(tools: &Tools, p: &Path, args: &[&str]) -> Option<String> {
    let probe = tools.ffprobe.as_ref()?;
    let mut a: Vec<OsString> = ["-v", "error"].iter().map(os).collect();
    a.extend(args.iter().map(os));
    a.push(os(p));
    run_tool(probe, &a).ok().map(|s| s.trim().to_string())
}

pub fn probe_size(tools: &Tools, p: &Path) -> Option<(u32, u32)> {
    let s = ffprobe(tools, p, &["-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0:s=x"])?;
    let line = s.lines().next()?;
    let (w, h) = line.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

pub fn probe_duration(tools: &Tools, p: &Path) -> Option<f64> {
    ffprobe(tools, p, &["-show_entries", "format=duration", "-of", "csv=p=0"])?.lines().next()?.trim().parse().ok()
}

fn has_audio(tools: &Tools, p: &Path) -> bool {
    ffprobe(tools, p, &["-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

fn merge_video(paths: &[PathBuf], tools: &Tools) -> R<PathBuf> {
    let (w, h) = probe_size(tools, &paths[0]).unwrap_or((1280, 720));
    let (w, h) = ((w / 2 * 2).max(2), (h / 2 * 2).max(2));
    let audio = paths.iter().all(|p| has_audio(tools, p));
    let n = paths.len();
    let mut args = ff_base();
    for p in paths {
        args.push(os("-i"));
        args.push(os(p));
    }
    let mut graph = String::new();
    let mut cat = String::new();
    for i in 0..n {
        graph += &format!(
            "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30,format=yuv420p[v{i}];"
        );
        cat += &format!("[v{i}]");
        if audio {
            graph += &format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];");
            cat += &format!("[a{i}]");
        }
    }
    graph += &format!("{cat}concat=n={n}:v=1:a={}[v]{}", audio as u8, if audio { "[a]" } else { "" });
    args.extend([os("-filter_complex"), os(graph), os("-map"), os("[v]")]);
    if audio {
        args.extend(["-map", "[a]", "-c:a", "aac", "-b:a", "160k"].iter().map(os));
    }
    args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-movflags", "+faststart"].iter().map(os));
    let out = unique_out(&paths[0], "-juntado", "mp4");
    args.push(os(&out));
    ffmpeg_run(tools, args, &out)
}

fn merge_audio(paths: &[PathBuf], tools: &Tools) -> R<PathBuf> {
    let n = paths.len();
    let mut args = ff_base();
    for p in paths {
        args.push(os("-i"));
        args.push(os(p));
    }
    let mut graph = String::new();
    let mut cat = String::new();
    for i in 0..n {
        graph += &format!("[{i}:a:0]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];");
        cat += &format!("[a{i}]");
    }
    graph += &format!("{cat}concat=n={n}:v=0:a=1[a]");
    args.extend([os("-filter_complex"), os(graph), os("-map"), os("[a]")]);
    args.extend(["-c:a", "libmp3lame", "-q:a", "2"].iter().map(os));
    let out = unique_out(&paths[0], "-juntado", "mp3");
    args.push(os(&out));
    ffmpeg_run(tools, args, &out)
}

// ---------- documentos (LibreOffice) ----------

fn doc_to_pdf(p: &Path, tools: &Tools) -> R<PathBuf> {
    let soffice = tools.soffice.as_ref().ok_or("LibreOffice não encontrado")?;
    let tmpdir = temp_file("lo");
    std::fs::create_dir_all(&tmpdir).map_err(es)?;
    // Perfil separado para não conflitar com um LibreOffice já aberto.
    let profile = std::env::temp_dir().join("shiftdrop-lo-profile");
    let url = format!("file:///{}", profile.display().to_string().replace('\\', "/").replace(' ', "%20"));
    let args = vec![
        os(format!("-env:UserInstallation={url}")),
        os("--headless"),
        os("--norestore"),
        os("--convert-to"),
        os("pdf"),
        os("--outdir"),
        os(&tmpdir),
        os(p),
    ];
    let r = run_tool(soffice, &args);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let produced = tmpdir.join(format!("{stem}.pdf"));
    let result = match r {
        Ok(_) if produced.is_file() => {
            let out = unique_out(p, "", "pdf");
            std::fs::rename(&produced, &out)
                .or_else(|_| std::fs::copy(&produced, &out).map(|_| ()))
                .map_err(es)
                .map(|_| out)
        }
        Ok(_) => Err("LibreOffice não gerou o PDF".into()),
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&tmpdir);
    result
}
