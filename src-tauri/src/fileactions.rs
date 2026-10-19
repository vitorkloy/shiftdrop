//! Roda "File actions" (Alt): vale para qualquer arquivo ou pasta.
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

fn joined(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")
}

/// Roda um script PowerShell oculto; os caminhos vão em $env:SD_PATHS (um por linha).
#[cfg(windows)]
fn ps(script: &str, paths: &[PathBuf]) -> R<String> {
    let exe = PathBuf::from("powershell.exe");
    let mut c = crate::tools::command(&exe);
    c.args(["-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-Command", script]);
    c.env("SD_PATHS", joined(paths));
    let out = c.output().map_err(|e| format!("powershell: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("falhou").to_string())
    }
}

#[cfg(windows)]
const LIST: &str = r#"$p = $env:SD_PATHS -split "`n" | Where-Object { $_ }"#;

#[cfg(windows)]
fn pick_folder() -> R<Option<PathBuf>> {
    let script = r#"Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true; ShowInTaskbar = $false }
$d = New-Object System.Windows.Forms.FolderBrowserDialog
$d.Description = 'ShiftDrop: mover para...'
$d.ShowNewFolderButton = $true
if ($d.ShowDialog($f) -eq 'OK') { [Console]::OutputEncoding = [Text.Encoding]::UTF8; $d.SelectedPath }"#;
    let s = ps(script, &[])?;
    Ok(if s.is_empty() { None } else { Some(PathBuf::from(s)) })
}

fn move_one(p: &Path, dest_dir: &Path) -> R<PathBuf> {
    let name = p.file_name().ok_or("nome inválido")?;
    let mut dest = dest_dir.join(name);
    let mut n = 1;
    while dest.exists() {
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        dest = match p.extension() {
            Some(e) if p.is_file() => dest_dir.join(format!("{stem} ({n}).{}", e.to_string_lossy())),
            _ => dest_dir.join(format!("{} ({n})", name.to_string_lossy())),
        };
        n += 1;
    }
    if std::fs::rename(p, &dest).is_ok() {
        return Ok(dest);
    }
    if p.is_file() {
        std::fs::copy(p, &dest).map_err(|e| e.to_string())?;
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
        Ok(dest)
    } else {
        Err("não dá para mover pastas entre unidades diferentes".into())
    }
}

#[cfg(windows)]
fn open_terminal(p: &Path) -> R<()> {
    let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_default() };
    if std::process::Command::new("wt.exe").arg("-d").arg(&dir).spawn().is_ok() {
        return Ok(());
    }
    let mut c = std::process::Command::new("powershell.exe");
    c.arg("-NoExit").current_dir(&dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE
    }
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn run(action: &str, paths: &[PathBuf]) -> R<Vec<PathBuf>> {
    match action {
        "copy" => {
            ps(&format!("{LIST}; Set-Clipboard -LiteralPath $p"), paths)?;
            Ok(paths.to_vec())
        }
        "copyPath" => {
            ps(&format!("{LIST}; Set-Clipboard -Value ($p -join \"`r`n\")"), paths)?;
            Ok(paths.to_vec())
        }
        "move" => {
            let Some(dest) = pick_folder()? else { return Err("cancelado".into()) };
            paths.iter().map(|p| move_one(p, &dest)).collect()
        }
        "openTerminal" => {
            open_terminal(&paths[0])?;
            Ok(vec![])
        }
        _ => Err("Em breve no ShiftDrop".into()),
    }
}

/// macOS/Linux: clipboard e diálogos com as ferramentas do sistema.
#[cfg(not(windows))]
fn pipe(cmds: &[&[&str]], input: &str) -> R<()> {
    use std::io::Write;
    for c in cmds {
        if let Ok(mut ch) = std::process::Command::new(c[0]).args(&c[1..]).stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut i) = ch.stdin.take() {
                let _ = i.write_all(input.as_bytes());
            }
            if ch.wait().map(|s| s.success()).unwrap_or(false) {
                return Ok(());
            }
        }
    }
    Err("sem utilitário de clipboard (instale wl-clipboard ou xclip)".into())
}

#[cfg(not(windows))]
fn clip_text(t: &str) -> R<()> {
    pipe(&[&["pbcopy"], &["wl-copy"], &["xclip", "-selection", "clipboard"], &["xsel", "-ib"]], t)
}

#[cfg(not(windows))]
fn pick_folder_unix() -> R<Option<PathBuf>> {
    let out = if cfg!(target_os = "macos") {
        std::process::Command::new("osascript")
            .args(["-e", r#"POSIX path of (choose folder with prompt "ShiftDrop: mover para...")"#])
            .output()
    } else {
        std::process::Command::new("zenity")
            .args(["--file-selection", "--directory", "--title=ShiftDrop: mover para..."])
            .output()
            .or_else(|_| std::process::Command::new("kdialog").arg("--getexistingdirectory").output())
    }
    .map_err(|e| format!("diálogo de pasta: {e}"))?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if out.status.success() && !s.is_empty() { Some(PathBuf::from(s)) } else { None })
}

#[cfg(not(windows))]
pub fn run(action: &str, paths: &[PathBuf]) -> R<Vec<PathBuf>> {
    let dir_of = |p: &Path| if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_default() };
    match action {
        "copy" => {
            if cfg!(target_os = "macos") {
                let list = paths.iter().map(|p| format!("POSIX file \"{}\"", p.display())).collect::<Vec<_>>().join(", ");
                std::process::Command::new("osascript")
                    .args(["-e", &format!("set the clipboard to {{{list}}}")])
                    .status()
                    .map_err(|e| e.to_string())?;
            } else {
                let uris = paths.iter().map(|p| format!("file://{}", p.display())).collect::<Vec<_>>().join("\n");
                pipe(&[&["wl-copy", "-t", "text/uri-list"], &["xclip", "-selection", "clipboard", "-t", "text/uri-list"]], &uris)?;
            }
            Ok(paths.to_vec())
        }
        "copyPath" => {
            clip_text(&joined(paths))?;
            Ok(paths.to_vec())
        }
        "move" => {
            let Some(dest) = pick_folder_unix()? else { return Err("cancelado".into()) };
            paths.iter().map(|p| move_one(p, &dest)).collect()
        }
        "openTerminal" => {
            let d = dir_of(&paths[0]);
            let ok = if cfg!(target_os = "macos") {
                std::process::Command::new("open").args(["-a", "Terminal"]).arg(&d).spawn().is_ok()
            } else {
                ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"]
                    .iter()
                    .any(|t| std::process::Command::new(t).current_dir(&d).spawn().is_ok())
            };
            if ok { Ok(vec![]) } else { Err("nenhum terminal encontrado".into()) }
        }
        _ => Err("Em breve no ShiftDrop".into()),
    }
}
