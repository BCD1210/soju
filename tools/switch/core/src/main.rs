use soju_switch_core::nro::Nro;
use std::{fs::File, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: soju-switch-core <homebrew.nro> (validate only)");
        return ExitCode::from(2);
    }
    let result = File::open(&args[0])
        .map_err(|e| e.to_string())
        .and_then(|mut file| Nro::read(&mut file).map_err(|e| e.to_string()));
    match result {
        Ok(nro) => {
            println!(
                "Valid NRO: {} image bytes, {} BSS bytes; execution is not implemented",
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
