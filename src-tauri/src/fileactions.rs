//! Roda "File actions" (Alt): vale para qualquer arquivo ou pasta.
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

fn joined(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")
}

/// Roda um script PowerShell oculto; os caminhos vão em $env:SD_PATHS (um por linha).
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

const LIST: &str = r#"$p = $env:SD_PATHS -split "`n" | Where-Object { $_ }"#;

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
