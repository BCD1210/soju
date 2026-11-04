//! Windowless guest execution and first-frame capture. Input never reaches the host UI.
use crate::{
    ipc::router::Hub,
    nro::Nro,
    runtime::{Runtime, Stop},
};
use serde::Deserialize;
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    /// Milliseconds since guest boot; state persists until the next entry.
    at_ms: u64,
    buttons: u64,
    #[serde(default)]
    left: [i32; 2],
    #[serde(default)]
    right: [i32; 2],
}

pub fn run(args: &[OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.first().ok_or("missing NRO path")?;
    let mut frames = None;
    let mut capture = None;
    let mut capture_frame = 1;
    let mut inputs = Vec::<Input>::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut options = args[1..].chunks_exact(2);
    for pair in &mut options {
        if !seen.insert(&pair[0]) {
            return Err("duplicate headless option".into());
        }
        match pair[0].to_str().ok_or("invalid option")? {
            "--frames" => {
                frames = Some(
                    pair[1]
                        .to_str()
                        .ok_or("invalid frame count")?
                        .parse::<std::num::NonZeroU64>()?,
                )
            }
            "--capture-frame" => {
                capture_frame = pair[1]
                    .to_str()
                    .ok_or("invalid capture frame")?
                    .parse::<std::num::NonZeroU64>()?
                    .get();
            }
            "--capture" => capture = Some(PathBuf::from(&pair[1])),
            "--inputs" => inputs = serde_json::from_reader(std::fs::File::open(&pair[1])?)?,
            _ => return Err("unknown headless option".into()),
        }
    }
    if !options.remainder().is_empty() {
        return Err("missing option value".into());
    }
    let frames = frames.ok_or("--frames is required")?.get();
    if capture_frame > frames {
        return Err("capture frame exceeds frame budget".into());
    }
    let capture = capture.ok_or("--capture is required")?;
    if inputs.windows(2).any(|p| p[0].at_ms >= p[1].at_ms)
        || inputs.iter().any(|i| {
            i.left
                .iter()
                .chain(&i.right)
                .any(|v| !(-32767..=32767).contains(v))
        })
    {
        return Err("inputs require increasing at_ms and sticks in -32767..32767".into());
    }
    let home = std::env::var_os("SOJU_SWITCH_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/soju/switch"))
        })
        .ok_or("HOME is not set")?;
    let nro = Nro::read(&mut std::fs::File::open(path)?)?;
    let mut runtime = Runtime::new(&nro.protected_homebrew()?)?;
    let mut hub = Hub::new(std::path::Path::new(path), &home)?;
    runtime.kernel_mut().program_id = hub.program_id();
    runtime.yield_on_frame = true;
    let start = Instant::now();
    let mut input = inputs.iter().peekable();
    let mut count = 0;
    loop {
        while input
            .peek()
            .is_some_and(|i| u128::from(i.at_ms) <= start.elapsed().as_millis())
        {
            if let Some(i) = input.next() {
                hub.set_input(
                    soju_switch_input::NpadState {
                        buttons: i.buttons,
                        left: soju_switch_input::Stick {
                            x: i.left[0],
                            y: i.left[1],
                        },
                        right: soju_switch_input::Stick {
                            x: i.right[0],
                            y: i.right[1],
                        },
                    }
                    .with_stick_buttons(),
                );
            }
        }
        let stop = runtime.run(&mut hub, Duration::from_millis(8), 10_000)?;
        // Consume audio packets without opening an output device in the headless harness.
        while hub.take_audio().is_some() {}
        if let Some(frame) = hub.take_frame() {
            count += 1;
            if count == capture_frame {
                let mut encoder = png::Encoder::new(
                    std::io::BufWriter::new(std::fs::File::create(&capture)?),
                    frame.width,
                    frame.height,
                );
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header()?;
                writer.write_image_data(&frame.pixels)?;
                writer.finish()?;
                println!(
                    "frame|{}x{}|{}",
                    frame.width,
                    frame.height,
                    capture.display()
                );
            }
        }
        match stop {
            Stop::ProcessExited(0) | Stop::AllThreadsExited => {
                hub.finish()?;
                println!("exit0|frames={count}");
                return Ok(());
            }
            Stop::BudgetExpired => {}
            other => return Err(format!("guest stopped: {other:?}").into()),
        }
        if count >= frames {
            hub.finish()?;
            println!(
                "frames|{count}|seconds={:.6}",
                start.elapsed().as_secs_f64()
            );
            return Ok(());
        }
    }
}
