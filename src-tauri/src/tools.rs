//! Detecção das ferramentas externas (ffmpeg, ImageMagick, LibreOffice).
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub magick: Option<PathBuf>,
    pub soffice: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    /// Ghostscript (PDF → imagem, comprimir PDF).
    pub gs: Option<PathBuf>,
    /// tar.exe do Windows (bsdtar): zip/tar/gz.
    pub tar: Option<PathBuf>,
}

impl Tools {
    pub fn detect() -> Self {
        let ffmpeg = find("ffmpeg", &ffmpeg_fallbacks());
        let ffprobe = ffmpeg
            .as_ref()
            .map(|p| p.with_file_name(exe("ffprobe")))
            .filter(|p| p.is_file() || crate::deps::planned("ffprobe").as_ref() == Some(p))
            .or_else(|| find("ffprobe", &[]));
        let magick = find("magick", &program_files_glob("ImageMagick", "magick.exe"));
        let soffice = find("soffice", &soffice_fallbacks());
        let pandoc = find("pandoc", &pandoc_fallbacks());
        let gs = find("gswin64c", &gs_fallbacks()).or_else(|| find("gs", &[]));
        let tar = find("tar", &[PathBuf::from(r"C:\Windows\System32\tar.exe")]);
        Tools { ffmpeg, ffprobe, magick, soffice, pandoc, gs, tar }
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn find(name: &str, fallbacks: &[PathBuf]) -> Option<PathBuf> {
    // Permite forçar um caminho, ex.: SHIFTDROP_FFMPEG=C:\ffmpeg\bin\ffmpeg.exe
    if let Some(p) = std::env::var_os(format!("SHIFTDROP_{}", name.to_uppercase())) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    // Embutidas no pacote: <raiz>/<nome> ou <raiz>/<subpasta>/<nome>.
    let file = exe(name);
    for root in crate::deps::bundled_roots() {
        let mut dirs = vec![root.clone()];
        dirs.extend(subdirs(&root));
        dirs.extend(subdirs(&root).iter().map(|d| d.join("bin")));
        if let Some(p) = dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file()) {
            return Some(p);
        }
    }
    // SHIFTDROP_NO_SYSTEM=1 ignora o que está instalado no sistema (simula uma máquina limpa).
    if std::env::var_os("SHIFTDROP_NO_SYSTEM").is_some() {
        return crate::deps::planned(name);
    }
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    if let Some(p) = fallbacks.iter().find(|p| p.is_file()) {
        return Some(p.clone());
    }
    // Apps GUI no macOS/Linux não herdam o PATH do shell (Homebrew, etc.).
    #[cfg(not(windows))]
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/Applications/LibreOffice.app/Contents/MacOS"] {
        let p = Path::new(d).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    // Baixada antes (ou a baixar no 1º uso).
    crate::deps::planned(name)
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

fn starts_with(p: &Path, prefix: &str) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn program_files() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .collect()
}

fn program_files_glob(prefix: &str, file: &str) -> Vec<PathBuf> {
    program_files()
        .iter()
        .flat_map(|pf| subdirs(pf))
        .filter(|d| starts_with(d, prefix))
        .map(|d| d.join(file))
        .collect()
}

fn ffmpeg_fallbacks() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        v.push(local.join(r"Microsoft\WinGet\Links\ffmpeg.exe"));
        for pkg in subdirs(&local.join(r"Microsoft\WinGet\Packages")) {
            if starts_with(&pkg, "Gyan.FFmpeg") || starts_with(&pkg, "BtbN.FFmpeg") {
                for sub in subdirs(&pkg) {
                    v.push(sub.join(r"bin\ffmpeg.exe"));
                }
            }
        }
    }
    v.push(PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"));
    v
}

fn pandoc_fallbacks() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = program_files().into_iter().map(|pf| pf.join(r"Pandoc\pandoc.exe")).collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        v.push(local.join(r"Pandoc\pandoc.exe"));
        v.push(local.join(r"Programs\Pandoc\pandoc.exe"));
        v.push(local.join(r"Microsoft\WinGet\Links\pandoc.exe"));
    }
    v
}

/// Ghostscript instala em `Program Files\gs\gs<versão>\bin\gswin64c.exe` (fora do PATH).
fn gs_fallbacks() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for pf in program_files() {
        let mut vers = subdirs(&pf.join("gs"));
        vers.sort();
        for d in vers.into_iter().rev() {
            v.push(d.join(r"bin\gswin64c.exe"));
            v.push(d.join(r"bin\gswin32c.exe"));
        }
    }
    v
}

fn soffice_fallbacks() -> Vec<PathBuf> {
    program_files()
        .into_iter()
        .map(|pf| pf.join(r"LibreOffice\program\soffice.exe"))
        .collect()
}

/// Cria um Command que não abre janela de console no Windows.
pub fn command(program: &Path) -> Command {
    crate::deps::ensure(program);
    raw_command(program)
}

/// Como `command`, sem disparar download.
pub fn raw_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Executa uma ferramenta e devolve o stdout; em erro devolve o fim do stderr.
pub fn run_tool(program: &Path, args: &[OsString]) -> Result<String, String> {
    let out = command(program)
        .args(args)
        .output()
        .map_err(|e| format!("falha ao executar {}: {e}", program.display()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        let tail: String = err.chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
        Err(format!(
            "{} falhou ({}): {}",
            program.file_name().and_then(|n| n.to_str()).unwrap_or("ferramenta"),
            out.status,
            tail
        ))
    }
}
