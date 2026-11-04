//! Execute only locally compiled dynamic-linker fixtures through the Program loader.
use soju_switch_core::{
    program::Program,
    runtime::{NoIpc, Runtime, Stop},
};
use soju_switch_fs::{nca::fixture, npdm, pfs0};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("fixture directory required")?,
    );
    let mut caps = vec![7 | (63 << 4) | (3 << 24)];
    for group in 0..8 {
        caps.push(15 | (0xffffff << 5) | (group << 29));
    }
    let mut metadata = npdm::build("Soju dynamic probe", 0x0100000000010000, &[], &caps);
    metadata[12] = 7;
    let mut modules = Vec::new();
    for name in ["rtld", "main", "subsdk0", "sdk"] {
        modules.push((name, std::fs::read(directory.join(format!("{name}.nso")))?));
    }
    let mut files: Vec<_> = modules
        .iter()
        .map(|(name, bytes)| (*name, bytes.as_slice()))
        .collect();
    files.push(("main.npdm", &metadata));
    let made = fixture::program_nca(&files, false);
    let package = pfs0::build(&[("own-dynamic.nca", &made.image)]);
    let program = Program::load(&package, "nsp", &made.keys)?;
    for (name, address) in &program.modules {
        println!("module|{name}|{address:#x}");
    }
    let mut runtime = Runtime::from_program(&program)?;
    let start = Instant::now();
    let mut transcript = String::new();
    loop {
        let result = runtime.run(&mut NoIpc, Duration::from_millis(8), 10_000)?;
        for bytes in runtime.kernel_mut().take_debug_output() {
            let line = std::str::from_utf8(&bytes)?;
            print!("{line}");
            transcript.push_str(line);
        }
        match result {
            Stop::ProcessExited(0)
                if transcript.contains("TOTAL|dynamic-modules|failures=0")
                    && !transcript.contains("FAIL|") =>
            {
                return Ok(());
            }
            Stop::BudgetExpired if start.elapsed() < Duration::from_secs(30) => {}
            other => return Err(format!("dynamic probe incomplete: {other:?}").into()),
        }
    }
}
