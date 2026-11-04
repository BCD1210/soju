//! Real, locally installed homebrew acceptance. Never substitutes fixture images.
use std::{fs, io::BufReader, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example01_matches_source_rectangles() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko_examples NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let input = home.join("inputs.json");
    fs::write(&input, r#"[{"at_ms":100,"buttons":1}]"#).unwrap();
    let capture = output.join("deko-example01.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--warmup-ms", "1000", "--capture"])
        .arg(&capture)
        .arg("--inputs")
        .arg(input)
        .output()
        .unwrap();
    fs::write(
        output.join("deko-example01.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("0x19d0"),
        "no Metal clear dispatch"
    );
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1280, 720, png::ColorType::Rgba)
    );
    for (i, p) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = (i % 1280, i / 1280);
        let expected = if (600..680).contains(&x) && (260..360).contains(&y)
            || (520..760).contains(&x) && (360..460).contains(&y)
        {
            [0u8, 128, 0, 255]
        } else if (440..840).contains(&x) && (160..560).contains(&y) {
            [229, 255, 232, 255]
        } else {
            [0, 64, 0, 255]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example02_matches_source_triangle() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko_examples NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("triangle-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let input = home.join("inputs.json");
    fs::write(
        &input,
        r#"[{"at_ms":100,"buttons":32768},{"at_ms":200,"buttons":0},{"at_ms":300,"buttons":1}]"#,
    )
    .unwrap();
    let capture = output.join("deko-example02.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--warmup-ms", "1000", "--capture"])
        .arg(&capture)
        .arg("--inputs")
        .arg(input)
        .output()
        .unwrap();
    fs::write(
        output.join("deko-example02.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("gpu-status|0xb197|0x0d78|0x00000003|handled")
    );
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1280, 720, png::ColorType::Rgba)
    );
    let mut checked = 0;
    for (i, p) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = (i % 1280, i / 1280);
        // Source vertices (0,+1),(-1,-1),(+1,-1), with the programmed negative Y scale.
        let red = 1. - (y as f64 + 0.5) / 720.;
        let blue = ((x as f64 + 0.5) / 1280.) - red * 0.5;
        let green = 1. - red - blue;
        let edge = red.min(green).min(blue);
        if edge.abs() < 0.002 {
            continue;
        }
        let expected = if edge > 0. {
            [
                (red * 255.).round() as u8,
                (green * 255.).round() as u8,
                (blue * 255.).round() as u8,
                255,
            ]
        } else {
            [0, 0, 0, 0]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
        checked += 1;
    }
    assert!(checked > 910_000);
    eprintln!("Example02: {checked} source-derived pixel checks");
    fs::remove_dir_all(home).unwrap();
}
#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example03_matches_source_cube() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko_examples NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("cube-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let input = home.join("inputs.json");
    fs::write(
        &input,
        r#"[{"at_ms":100,"buttons":32768},{"at_ms":200,"buttons":0},{"at_ms":300,"buttons":32768},{"at_ms":400,"buttons":0},{"at_ms":500,"buttons":1}]"#,
    )
    .unwrap();
    let capture = output.join("deko-example03.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--warmup-ms", "1000", "--capture"])
        .arg(&capture)
        .arg("--inputs")
        .arg(input)
        .output()
        .unwrap();
    fs::write(
        output.join("deko-example03.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("gpu-status|0xb197|0x0d78|0x00000018|handled")
    );
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1280, 720, png::ColorType::Rgba)
    );
    let mut checked = 0;
    for (i, p) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = (i % 1280, i / 1280);
        // At the initial source pose: scale .5, translate Z=-3, front Z=.5,
        // vertical FOV 40 degrees. The +Z quad covers this projected square.
        let half = 72. / (20f64.to_radians().tan());
        let u = (x as f64 + 0.5 - (640. - half)) / (2. * half);
        let v = (y as f64 + 0.5 - (360. - half)) / (2. * half);
        let edge = u.min(v).min(1. - u).min(1. - v);
        if edge.abs() < 0.008 {
            continue;
        }
        let expected = if edge > 0. {
            [
                ((1. - v) * 255.).round() as u8,
                ((u - v).abs() * 255.).round() as u8,
                (u.min(v) * 255.).round() as u8,
                255,
            ]
        } else {
            [0, 0, 0, 0]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 4),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
        checked += 1;
    }
    assert!(checked > 900_000);
    eprintln!("Example03: {checked} source-derived pixel checks");
    fs::remove_dir_all(home).unwrap();
}
