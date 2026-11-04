//! First-frame SDL rectangles, derived from the local sample's main.cpp.
use std::{fs, io::BufReader, path::PathBuf, process::Command};
#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn sdl_rectangles_match_source() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_SDL_NRO").expect("local SDL NRO");
    let output = PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("output"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("sdl-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let capture = output.join("sdl2-simple.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
        .arg(&capture)
        .output()
        .unwrap();
    fs::write(
        output.join("sdl2-simple.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1920, 1080, png::ColorType::Rgba)
    );
    // x starts at zero and increments only after presentation. Each row contains
    // adjacent 64x64 red, green and blue rectangles, on a 111-gray background.
    for (i, pixel) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = (i % 1920, i / 1920);
        let rgb = if !(64..1016).contains(&y) && x < 192 {
            [[255, 0, 0], [0, 255, 0], [0, 0, 255]][x / 64]
        } else {
            [111, 111, 111]
        };
        assert_eq!(pixel, [rgb[0], rgb[1], rgb[2], 255], "pixel ({x},{y})");
    }
    eprintln!("sdl2-simple: 2073600 exact RGBA pixel checks");
    fs::remove_dir_all(home).unwrap();
}
