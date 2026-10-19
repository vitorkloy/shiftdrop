// Esconde o console no build de release (app de bandeja).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = args.first() {
        if matches!(first.as_str(), "tools" | "actions" | "convert" | "help" | "--help" | "-h") {
            std::process::exit(shiftdrop_lib::cli::run(&args));
        }
    }
    shiftdrop_lib::run();
}
