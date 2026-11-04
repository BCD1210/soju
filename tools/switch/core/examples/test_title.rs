//! Execute locally built probes; debug output is confined to this explicit harness.
use soju_switch_core::{ipc::router::Hub, nro::Nro, runtime::{Runtime, Stop}};
use std::{path::PathBuf, time::{Duration, Instant}};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("probe path required")?);
    let home = PathBuf::from(args.next().ok_or("isolated home required")?);
    let nro = Nro::read(&mut std::fs::File::open(&path)?)?;
    let mut runtime = Runtime::new(&nro.protected_homebrew()?)?;
    let mut hub = Hub::new(&path, &home)?;
    runtime.kernel_mut().program_id = hub.program_id();
    let start = Instant::now();
    let mut transcript = String::new();
    loop {
        let result = runtime.run(&mut hub, Duration::from_millis(8), 10_000);
        for bytes in runtime.kernel_mut().take_debug_output() {
            let line = String::from_utf8_lossy(&bytes);
            print!("{line}");
            transcript.push_str(&line);
        }
        match result? {
            Stop::ProcessExited(0) | Stop::AllThreadsExited => {
                hub.finish()?;
                if !transcript.contains("TOTAL|") || transcript.contains("FAIL|") {
                    return Err("probe incomplete or failed".into());
                }
                println!("exit0|probe-complete");
                return Ok(());
            }
            Stop::BudgetExpired if start.elapsed() < Duration::from_secs(30) => {}
            other => return Err(format!("probe stopped: {other:?}").into()),
        }
    }
}
