//! Windowless guest execution and first-frame capture. Input never reaches the host UI.
use crate::runtime::Stop;
use serde::Deserialize;
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    /// Milliseconds since guest boot; capture-clock mode preserves intervals after stalls.
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
    let mut warmup_ms = 0u64;
    let mut capture_tick = None;
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
            "--capture-tick" => {
                let tick = pair[1]
                    .to_str()
                    .ok_or("invalid capture tick")?
                    .parse::<u64>()?;
                if tick == u64::MAX {
                    return Err("capture tick is reserved".into());
                }
                capture_tick = Some(tick);
            }
            "--warmup-ms" => warmup_ms = pair[1].to_str().ok_or("invalid warmup")?.parse()?,
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
    let (mut runtime, mut hub) = crate::launch::load(std::path::Path::new(path), &home)?;
    if let Some(tick) = capture_tick {
        runtime.process().set_capture_tick(tick);
        println!("capture-clock|tick={tick}|host-deadlines=real");
    }
    runtime.yield_on_frame = true;
    let start = Instant::now();
    let mut input = inputs.iter().peekable();
    let mut count = 0;
    let mut input_frame = false;
    let mut next_input_at = inputs.first().map_or(0, |i| u128::from(i.at_ms));
    loop {
        // Deliver each state across a presentation boundary. A host stall must
        // never collapse a press and release into one unobserved HID update.
        while (capture_tick.is_none() || input_frame)
            && input.peek().is_some()
            && next_input_at <= start.elapsed().as_millis()
        {
            if let Some(i) = input.next() {
                input_frame = false;
                next_input_at = input.peek().map_or(0, |next| {
                    if capture_tick.is_some() {
                        start.elapsed().as_millis() + u128::from(next.at_ms - i.at_ms)
                    } else {
                        u128::from(next.at_ms)
                    }
                });
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
            input_frame = true;
            if start.elapsed().as_millis() >= u128::from(warmup_ms)
                && (capture_tick.is_none() || input.peek().is_none())
            {
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
