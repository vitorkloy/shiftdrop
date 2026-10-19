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
}

impl Tools {
    pub fn detect() -> Self {
        let ffmpeg = find("ffmpeg", &ffmpeg_fallbacks());
        let ffprobe = ffmpeg
            .as_ref()
            .map(|p| p.with_file_name(exe("ffprobe")))
            .filter(|p| p.is_file())
            .or_else(|| find("ffprobe", &[]));
        let magick = find("magick", &program_files_glob("ImageMagick", "magick.exe"));
        let soffice = find("soffice", &soffice_fallbacks());
        Tools { ffmpeg, ffprobe, magick, soffice }
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
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    fallbacks.iter().find(|p| p.is_file()).cloned()
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

fn soffice_fallbacks() -> Vec<PathBuf> {
    program_files()
        .into_iter()
        .map(|pf| pf.join(r"LibreOffice\program\soffice.exe"))
        .collect()
}

/// Cria um Command que não abre janela de console no Windows.
pub fn command(program: &Path) -> Command {
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
