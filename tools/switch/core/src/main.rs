use soju_switch_core::nro::Nro;
use std::{fs::File, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
    if args.first().is_some_and(|arg| arg == "--headless") {
        return match soju_switch_core::headless::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("soju-switch-core: {error}");
                ExitCode::FAILURE
            }
        };
    }
    #[cfg(all(feature = "desktop", target_os = "macos", target_arch = "aarch64"))]
    if args.len() == 2 && args[0] == "--play" {
        let home = std::env::var_os("SOJU_SWITCH_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| std::path::PathBuf::from(h).join(".local/share/soju/switch"))
            });
        let result = home
            .ok_or_else(|| "HOME is not set".to_string())
            .and_then(|home| {
                soju_switch_core::desktop::play(std::path::Path::new(&args[1]), &home)
            });
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("soju-switch-core: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if args.len() != 1 {
        eprintln!(
            "usage: soju-switch-core <homebrew.nro> (validate); --headless <nro> --frames N --capture <png> [--inputs script.json] (hv); --play <nro> (desktop)"
        );
        return ExitCode::from(2);
    }
    let result = File::open(&args[0])
        .map_err(|e| e.to_string())
        .and_then(|mut file| Nro::read(&mut file).map_err(|e| e.to_string()));
    match result {
        Ok(nro) => {
            println!(
                "Valid NRO: {} image bytes, {} BSS bytes; validation passed",
                nro.image.len(),
                nro.bss
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("soju-switch-core: {error}");
            ExitCode::FAILURE
        }
    }
}
