//! Shared homebrew and packaged-title entry path for desktop and headless execution.
use crate::{ipc::router::Hub, nro::Nro, program::Program, runtime::Runtime};
use soju_switch_fs::keys::KeySet;
use std::path::{Path, PathBuf};

pub fn load(path: &Path, home: &Path) -> Result<(Runtime, Hub), String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let (mut runtime, hub) = match extension.as_str() {
        "nro" => {
            let nro = Nro::read(&mut file).map_err(|e| e.to_string())?;
            (
                Runtime::new(&nro.protected_homebrew().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
                Hub::new(path, home).map_err(|e| e.to_string())?,
            )
        }
        "nsp" | "xci" => {
            let directory = std::env::var_os("SOJU_SWITCH_KEY_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("keys"));
            let title_keys = directory.join("title.keys");
            let keys = KeySet::load(
                &directory.join("prod.keys"),
                title_keys.is_file().then_some(title_keys.as_path()),
            )
            .map_err(|e| e.to_string())?;
            let program = Program::load(&file, &extension, &keys)?;
            (
                Runtime::from_program(&program).map_err(|e| e.to_string())?,
                Hub::new_program(&program, home).map_err(|e| e.to_string())?,
            )
        }
        _ => return Err("expected .nro, .nsp or .xci title".into()),
    };
    runtime.kernel_mut().program_id = hub.program_id();
    Ok((runtime, hub))
}
