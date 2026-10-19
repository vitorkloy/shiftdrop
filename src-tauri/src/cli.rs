//! Modo linha de comando: `shiftdrop tools | actions <arquivos> | convert <acao> <arquivos>`.
use crate::{actions, convert, tools::Tools};
use std::path::PathBuf;

#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
    unsafe {
        // Só anexa ao console do pai se a saída não foi redirecionada.
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
#[cfg(not(windows))]
fn attach_console() {}

pub fn run(args: &[String]) -> i32 {
    attach_console();
    let tools = Tools::detect();
    let paths = |s: &[String]| s.iter().map(PathBuf::from).collect::<Vec<_>>();
    match args.first().map(|s| s.as_str()) {
        Some("tools") => {
            println!("{}", serde_json::to_string_pretty(&tools).unwrap());
            0
        }
        Some("actions") if args.len() >= 2 => {
            let list = actions::list_actions(&paths(&args[1..]), &tools);
            println!("{}", serde_json::to_string_pretty(&list).unwrap());
            0
        }
        Some("convert") if args.len() >= 3 => match convert::run_action(&args[1], &paths(&args[2..]), &tools) {
            Ok(outs) => {
                for o in outs {
                    println!("{}", o.display());
                }
                0
            }
            Err(e) => {
                eprintln!("erro: {e}");
                1
            }
        },
        _ => {
            eprintln!("uso: shiftdrop tools | actions <arquivos...> | convert <acao> <arquivos...>");
            2
        }
    }
}
