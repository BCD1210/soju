//! Execute locally built probes; debug output is confined to this explicit harness.
use soju_switch_core::runtime::Stop;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("probe path required")?);
    let home = PathBuf::from(args.next().ok_or("isolated home required")?);
    let (mut runtime, mut hub) = soju_switch_core::launch::load(&path, &home)?;
    runtime.yield_on_frame = true;
    let mut presentations = 0;
    let start = Instant::now();
    let mut transcript = String::new();
    loop {
        let result = runtime.run(&mut hub, Duration::from_millis(8), 10_000);
        for bytes in runtime.kernel_mut().take_debug_output() {
            let line = String::from_utf8_lossy(&bytes);
            print!("{line}");
            transcript.push_str(&line);
        }
        if let Some(frame) = hub.take_frame() {
            presentations += 1;
            let mut encoder = png::Encoder::new(
                std::io::BufWriter::new(std::fs::File::create(
                    home.join(format!("frame-{}x{}.png", frame.width, frame.height)),
                )?),
                frame.width,
                frame.height,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&frame.pixels)?;
            writer.finish()?;
            if path.file_stem().is_some_and(|name| name == "media") {
                let expected_size = if presentations == 1 {
                    (1280, 720)
                } else {
                    (1920, 1080)
                };
                if (frame.width, frame.height) != expected_size {
                    return Err("unexpected presentation dimensions".into());
                }
                for (i, pixel) in frame.pixels.chunks_exact(4).enumerate() {
                    let expected = if i % (frame.width as usize) < frame.width as usize / 2 {
                        [0x11, 0x22, 0x33, 0xff]
                    } else {
                        [0xaa, 0xbb, 0xcc, 0xff]
                    };
                    if pixel != expected {
                        return Err("presentation pixel mismatch".into());
                    }
                }
                println!(
                    "PASS|{}x{} all pixels|frame={presentations}",
                    frame.width, frame.height
                );
            }
        }
        match result? {
            Stop::ProcessExited(0) | Stop::AllThreadsExited => {
                hub.finish()?;
                if !transcript.contains("TOTAL|") || transcript.contains("FAIL|") {
                    return Err("probe incomplete or failed".into());
                }
                if path.file_stem().is_some_and(|name| name == "media") && presentations != 2 {
                    return Err("missing media presentations".into());
                }
                println!("exit0|probe-complete");
                return Ok(());
            }
            Stop::BudgetExpired if start.elapsed() < Duration::from_secs(30) => {}
            other => return Err(format!("probe stopped: {other:?}").into()),
        }
    }
}
