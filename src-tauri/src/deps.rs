//! Ferramentas embutidas e baixadas sob demanda.
//!
//! - Windows: ffmpeg, ImageMagick, pandoc e Ghostscript vêm no instalador
//!   (`<exe>/tools`). O LibreOffice (~350 MB) é baixado no 1º uso para
//!   `%LOCALAPPDATA%\ShiftDrop\tools\LibreOffice` (MSI oficial extraído com
//!   `msiexec /a`, sem admin).
//! - macOS/Linux: se não houver no sistema, ffmpeg/ffprobe (estáticos) e pandoc
//!   são baixados no 1º uso para a pasta de dados do app.
//!
//! Enquanto baixa, o progresso vai para a UI no evento `dep-progress`.
use std::cell::Cell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

static APP: OnceLock<AppHandle> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());
thread_local!(static ALLOW: Cell<bool> = const { Cell::new(false) });

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// Libera downloads só para a thread que executa uma ação escolhida pelo usuário
/// (miniaturas/listagem nunca disparam download).
pub fn allow_downloads<T>(f: impl FnOnce() -> T) -> T {
    ALLOW.with(|a| a.set(true));
    let r = f();
    ALLOW.with(|a| a.set(false));
    r
}

/// Pastas com ferramentas embutidas no pacote (Windows/AppImage/.deb/.app).
pub fn bundled_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        v.push(d.join("tools"));
        v.push(d.join("../Resources/tools"));
        v.push(d.join("../lib/ShiftDrop/tools"));
        v.push(d.join("../lib/shiftdrop/tools"));
    }
    if let Some(a) = std::env::var_os("APPDIR") {
        v.push(PathBuf::from(a).join("usr/lib/ShiftDrop/tools"));
    }
    if let Some(r) = std::env::var_os("SHIFTDROP_TOOLS") {
        v.insert(0, PathBuf::from(r));
    }
    v
}

pub fn data_root() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join("ShiftDrop");
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("Library/Application Support/ShiftDrop");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".local/share"))
        .join("shiftdrop");
    base.join("tools")
}

/// Onde a ferramenta ficará quando baixada (só se houver download para esta plataforma).
pub fn planned(name: &str) -> Option<PathBuf> {
    let r = data_root();
    if cfg!(windows) {
        return (name == "soffice").then(|| r.join(r"LibreOffice\program\soffice.exe"));
    }
    match name {
        "ffmpeg" | "ffprobe" if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") || cfg!(target_os = "macos") => {
            Some(r.join("ffmpeg").join(name))
        }
        "pandoc" => Some(r.join("pandoc").join("pandoc")),
        _ => None,
    }
}

fn progress(tool: &str, done: u64, total: u64, phase: &str) {
    if let Some(a) = APP.get() {
        let pct = if total > 0 { (done * 100 / total) as u32 } else { 0 };
        let _ = a.emit("dep-progress", serde_json::json!({ "tool": tool, "pct": pct, "phase": phase, "done": done, "total": total }));
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().user_agent("ShiftDrop").build()
}

fn get_text(url: &str) -> Result<String, String> {
    agent().get(url).call().map_err(|e| format!("{url}: {e}"))?.into_string().map_err(|e| e.to_string())
}

fn download(tool: &str, url: &str, dest: &Path) -> Result<(), String> {
    crate::logx::log(format!("deps: baixando {tool} de {url}"));
    let resp = agent().get(url).call().map_err(|e| format!("download {tool}: {e}"))?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut r = resp.into_reader();
    let tmp = dest.with_extension("part");
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let (mut buf, mut done, mut last) = (vec![0u8; 1 << 16], 0u64, 0u64);
    loop {
        let n = r.read(&mut buf).map_err(|e| format!("download {tool}: {e}"))?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if done - last > 1 << 20 {
            last = done;
            progress(tool, done, total, "download");
        }
    }
    drop(f);
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

fn run(cmd: &mut std::process::Command) -> Result<(), String> {
    let s = cmd.status().map_err(|e| e.to_string())?;
    if s.success() { Ok(()) } else { Err(format!("{cmd:?} falhou ({s})")) }
}

/// Procura `name` recursivamente em `dir` e copia para `dest`.
fn pick(dir: &Path, name: &str, dest: &Path) -> Result<(), String> {
    fn walk(d: &Path, name: &str) -> Option<PathBuf> {
        for e in std::fs::read_dir(d).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(f) = walk(&p, name) {
                    return Some(f);
                }
            } else if p.file_name().map(|n| n == name).unwrap_or(false) {
                return Some(p);
            }
        }
        None
    }
    let f = walk(dir, name).ok_or(format!("{name} não encontrado no pacote"))?;
    std::fs::copy(&f, dest).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

fn latest_lo_version() -> Result<String, String> {
    let html = get_text("https://download.documentfoundation.org/libreoffice/stable/")?;
    let mut vs: Vec<Vec<u32>> = html
        .split("href=\"")
        .filter_map(|s| s.split('/').next())
        .filter(|s| s.split('.').count() == 3 && s.split('.').all(|p| p.parse::<u32>().is_ok()))
        .map(|s| s.split('.').map(|p| p.parse().unwrap()).collect())
        .collect();
    vs.sort();
    vs.last().map(|v| v.iter().map(u32::to_string).collect::<Vec<_>>().join(".")).ok_or("versão do LibreOffice não encontrada".into())
}

fn gh_asset(repo: &str, pred: impl Fn(&str) -> bool) -> Result<String, String> {
    let j: serde_json::Value =
        serde_json::from_str(&get_text(&format!("https://api.github.com/repos/{repo}/releases/latest"))?).map_err(|e| e.to_string())?;
    j["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().map(&pred).unwrap_or(false))
        .and_then(|a| a["browser_download_url"].as_str().map(String::from))
        .ok_or(format!("asset não encontrado em {repo}"))
}

fn install(name: &str, target: &Path) -> Result<(), String> {
    let root = data_root();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let work = root.join(format!(".{name}-tmp"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let res = (|| -> Result<(), String> {
        match name {
            "soffice" => {
                let v = latest_lo_version()?;
                let msi = work.join("lo.msi");
                download("LibreOffice", &format!("https://download.documentfoundation.org/libreoffice/stable/{v}/win/x86_64/LibreOffice_{v}_Win_x86-64.msi"), &msi)?;
                progress("LibreOffice", 1, 1, "install");
                let dir = root.join("LibreOffice");
                let mut c = crate::tools::raw_command("msiexec.exe");
                c.arg("/a").arg(&msi).arg("/qn");
                // msiexec exige PROPRIEDADE="valor" (aspas só no valor); o quoting padrão do Rust dá erro 1639.
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    c.raw_arg(format!("TARGETDIR=\"{}\"", dir.display()));
                }
                run(&mut c)?;
                // msiexec /a pode criar "LibreOffice\program" ou "LibreOffice\LibreOffice\program".
                if !target.is_file() {
                    let nested = dir.join("LibreOffice");
                    if nested.join(r"program\soffice.exe").is_file() {
                        let _ = std::fs::rename(dir.join("program"), work.join("old"));
                        for e in std::fs::read_dir(&nested).map_err(|e| e.to_string())?.flatten() {
                            let _ = std::fs::rename(e.path(), dir.join(e.file_name()));
                        }
                    }
                }
                Ok(())
            }
            "ffmpeg" | "ffprobe" => {
                if cfg!(target_os = "macos") {
                    for t in ["ffmpeg", "ffprobe"] {
                        let z = work.join(format!("{t}.zip"));
                        download(t, &format!("https://evermeet.cx/ffmpeg/getrelease/{}zip", if t == "ffmpeg" { "" } else { "ffprobe/" }), &z)?;
                        run(std::process::Command::new("ditto").arg("-xk").arg(&z).arg(&work))?;
                        pick(&work, t, &target.with_file_name(t))?;
                    }
                } else {
                    let a = work.join("ff.tar.xz");
                    download("ffmpeg", "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz", &a)?;
                    progress("ffmpeg", 1, 1, "install");
                    run(std::process::Command::new("tar").arg("-xJf").arg(&a).arg("-C").arg(&work))?;
                    for t in ["ffmpeg", "ffprobe"] {
                        pick(&work, t, &target.with_file_name(t))?;
                    }
                }
                Ok(())
            }
            "pandoc" => {
                let mac = cfg!(target_os = "macos");
                let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" };
                let url = gh_asset("jgm/pandoc", |n| {
                    if mac { n.ends_with(&format!("{arch}-macOS.zip")) } else { n.ends_with(if arch == "arm64" { "linux-arm64.tar.gz" } else { "linux-amd64.tar.gz" }) }
                })?;
                let a = work.join(if mac { "p.zip" } else { "p.tgz" });
                download("pandoc", &url, &a)?;
                if mac {
                    run(std::process::Command::new("ditto").arg("-xk").arg(&a).arg(&work))?;
                } else {
                    run(std::process::Command::new("tar").arg("-xzf").arg(&a).arg("-C").arg(&work))?;
                }
                pick(&work, "pandoc", target)
            }
            _ => Err(format!("sem download para {name}")),
        }
    })();
    let _ = std::fs::remove_dir_all(&work);
    progress(name, 1, 1, if res.is_ok() { "done" } else { "error" });
    crate::logx::log(format!("deps: {name} → {res:?}"));
    res
}

/// Chamado antes de executar uma ferramenta: se ela é um "planejado" que ainda
/// não existe e a thread pode baixar, baixa e instala (bloqueante).
pub fn ensure(program: &Path) {
    if program.is_file() || !ALLOW.with(|a| a.get()) {
        return;
    }
    let Some(stem) = program.file_stem().and_then(|s| s.to_str()) else { return };
    let Some(target) = planned(stem) else { return };
    if target != program {
        return;
    }
    let _g = LOCK.lock();
    if !program.is_file() {
        if let Err(e) = install(stem, &target) {
            crate::logx::log(format!("deps: falha ao instalar {stem}: {e}"));
        }
    }
}
