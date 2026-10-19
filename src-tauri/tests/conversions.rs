//! Testes de ponta a ponta das conversões (geram arquivos reais num diretório temporário).
//! Rodar: `cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture`
use shiftdrop_lib::actions::{list_actions, ActionInfo};
use shiftdrop_lib::convert::{probe_duration, probe_size, run_action, unique_path};
use shiftdrop_lib::tools::{run_tool, Tools};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("shiftdrop-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, if x < w / 4 { 100 } else { 255 }])
    });
    let p = dir.join(name);
    img.save(&p).unwrap();
    p
}

fn ffmpeg_gen(t: &Tools, args: &[&str], out: &Path) {
    let mut a: Vec<OsString> = ["-y", "-hide_banner", "-loglevel", "error"].iter().map(OsString::from).collect();
    a.extend(args.iter().map(OsString::from));
    a.push(out.into());
    run_tool(t.ffmpeg.as_ref().unwrap(), &a).unwrap();
}

fn ids(v: &[ActionInfo]) -> Vec<&str> {
    v.iter().map(|a| a.id).collect()
}

fn ok_file(p: &Path) {
    let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    assert!(len > 0, "arquivo vazio ou ausente: {}", p.display());
    println!("  ✓ {} ({} bytes)", p.file_name().unwrap().to_string_lossy(), len);
}

fn one(id: &str, input: &Path, t: &Tools) -> PathBuf {
    let outs = run_action(id, &[input.to_path_buf()], t).unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_eq!(outs.len(), 1);
    ok_file(&outs[0]);
    assert_eq!(outs[0].parent(), input.parent(), "saída deve ficar na mesma pasta");
    outs[0].clone()
}

#[test]
fn tools_detected() {
    let t = Tools::detect();
    println!("{}", serde_json::to_string_pretty(&t).unwrap());
}

#[test]
fn catalog_by_type_and_availability() {
    let none = Tools::default();
    let png = vec![PathBuf::from("a.png")];
    let a = list_actions(&png, &none);
    assert!(ids(&a).contains(&"to_jpg") && !ids(&a).contains(&"to_png"));
    assert!(a.iter().all(|x| x.available), "imagem comum funciona sem ferramentas externas");

    let heic = list_actions(&[PathBuf::from("a.heic")], &none);
    assert!(heic.iter().all(|x| !x.available && x.reason.as_deref() == Some("Requer ImageMagick")));

    let mp4 = list_actions(&[PathBuf::from("a.mp4")], &none);
    assert!(!ids(&mp4).contains(&"to_mp4") && ids(&mp4).contains(&"to_webm"));
    assert!(mp4.iter().all(|x| !x.available), "sem ffmpeg, vídeo fica desabilitado");

    let docx = list_actions(&[PathBuf::from("a.docx")], &none);
    assert_eq!(ids(&docx), vec!["doc_to_pdf"]);
    assert!(!docx[0].available);

    let two = list_actions(&[PathBuf::from("a.mp3"), PathBuf::from("b.wav")], &none);
    assert!(ids(&two).contains(&"merge_audio"));

    let mixed = list_actions(&[PathBuf::from("a.png"), PathBuf::from("b.mp3")], &none);
    assert!(mixed.is_empty(), "tipos misturados não têm ação");
}

#[test]
fn unique_names_never_overwrite() {
    let d = tmpdir("unique");
    std::fs::write(d.join("x.jpg"), b"1").unwrap();
    std::fs::write(d.join("x (1).jpg"), b"1").unwrap();
    assert_eq!(unique_path(&d, "x", "jpg"), d.join("x (2).jpg"));
}

#[test]
fn image_conversions() {
    let t = Tools::detect();
    let d = tmpdir("img");
    let png = make_png(&d, "foto.png", 640, 400);
    let jpg = one("to_jpg", &png, &t);
    assert_eq!(image::open(&jpg).unwrap().width(), 640);
    let webp = one("to_webp", &png, &t);
    assert_eq!(image::open(&webp).unwrap().height(), 400);
    let back = one("to_png", &jpg, &t);
    assert!(back.ends_with("foto (1).png"), "não sobrescreve o original: {}", back.display());
    let c = one("compress_image", &png, &t);
    assert!(c.ends_with("foto-comprimido.jpg"));
    let sq = image::open(one("crop_square", &png, &t)).unwrap();
    assert_eq!((sq.width(), sq.height()), (400, 400));
    let half = image::open(one("resize_half", &png, &t)).unwrap();
    assert_eq!((half.width(), half.height()), (320, 200));
}

#[test]
fn images_to_pdf() {
    let t = Tools::detect();
    let d = tmpdir("pdf");
    let a = make_png(&d, "p1.png", 300, 200);
    let b = make_png(&d, "p2.png", 200, 300);
    let outs = run_action("to_pdf", &[a.clone(), b], &t).unwrap();
    assert_eq!(outs.len(), 1);
    ok_file(&outs[0]);
    let bytes = std::fs::read(&outs[0]).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("/Count 2"));
    assert!(s.trim_end().ends_with("%%EOF"));
    let single = run_action("to_pdf", &[a], &t).unwrap();
    assert!(String::from_utf8_lossy(&std::fs::read(&single[0]).unwrap()).contains("/Count 1"));
    if let Some(m) = &t.magick {
        // Validação independente: o ImageMagick lê o PDF? (precisa de Ghostscript; só informativo)
        match run_tool(m, &["identify".into(), outs[0].clone().into_os_string()]) {
            Ok(o) => println!("  identify: {}", o.trim()),
            Err(e) => println!("  (identify indisponível: {e})"),
        }
    }
}

#[test]
fn magick_only_formats() {
    let t = Tools::detect();
    let Some(m) = t.magick.clone() else {
        println!("ImageMagick ausente, pulando");
        return;
    };
    let d = tmpdir("magick");
    let src = make_png(&d, "base.png", 120, 80);
    let tga = d.join("img.tga");
    run_tool(&m, &[src.into_os_string(), tga.clone().into_os_string()]).unwrap();
    let png = one("to_png", &tga, &t);
    assert_eq!(image::open(png).unwrap().width(), 120);
}

#[test]
fn video_conversions() {
    let t = Tools::detect();
    if t.ffmpeg.is_none() {
        println!("ffmpeg ausente, pulando");
        return;
    }
    let d = tmpdir("video");
    let v1 = d.join("clipe.mp4");
    ffmpeg_gen(&t, &["-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=15", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"], &v1);
    let v2 = d.join("outro.mov");
    ffmpeg_gen(&t, &["-f", "lavfi", "-i", "testsrc2=duration=1:size=640x360:rate=25", "-f", "lavfi", "-i", "sine=frequency=880:duration=1", "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"], &v2);

    let webm = one("to_webm", &v1, &t);
    assert_eq!(probe_size(&t, &webm), Some((320, 240)));
    one("to_gif", &v1, &t);
    let mp3 = one("extract_mp3", &v1, &t);
    assert!(probe_duration(&t, &mp3).unwrap() > 1.5);
    one("compress_video", &v1, &t);
    one("to_mp4", &v2, &t);
    let merged = run_action("merge_video", &[v1.clone(), v2.clone()], &t).unwrap();
    ok_file(&merged[0]);
    let dur = probe_duration(&t, &merged[0]).unwrap();
    assert!((2.6..3.6).contains(&dur), "duração juntada {dur}");
    assert_eq!(probe_size(&t, &merged[0]), Some((320, 240)));
}

#[test]
fn audio_conversions() {
    let t = Tools::detect();
    if t.ffmpeg.is_none() {
        println!("ffmpeg ausente, pulando");
        return;
    }
    let d = tmpdir("audio");
    let wav = d.join("tom.wav");
    ffmpeg_gen(&t, &["-f", "lavfi", "-i", "sine=frequency=440:duration=2"], &wav);
    let mp3 = one("to_mp3", &wav, &t);
    one("to_ogg", &wav, &t);
    let wav2 = one("to_wav", &mp3, &t);
    assert!(wav2.ends_with("tom (1).wav"));
    one("compress_audio", &wav, &t);
    let merged = run_action("merge_audio", &[wav.clone(), mp3.clone()], &t).unwrap();
    ok_file(&merged[0]);
    let dur = probe_duration(&t, &merged[0]).unwrap();
    assert!((3.5..4.6).contains(&dur), "duração juntada {dur}");
}

#[test]
fn docx_to_pdf_if_libreoffice() {
    let t = Tools::detect();
    let d = tmpdir("doc");
    let txt = d.join("nota.txt");
    std::fs::write(&txt, "Olá do ShiftDrop").unwrap();
    if t.soffice.is_none() {
        let e = run_action("doc_to_pdf", &[txt], &t).unwrap_err();
        assert!(e.contains("LibreOffice"));
        println!("LibreOffice ausente: ação desabilitada corretamente");
        return;
    }
    let pdf = one("doc_to_pdf", &txt, &t);
    assert!(std::fs::read(pdf).unwrap().starts_with(b"%PDF"));
}
