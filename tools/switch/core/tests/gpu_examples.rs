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
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
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
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
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
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
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
#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example04_matches_source_texture() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko_examples NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("texture-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let input = home.join("inputs.json");
    fs::write(
        &input,
        r#"[{"at_ms":100,"buttons":32768},{"at_ms":200,"buttons":0},{"at_ms":300,"buttons":32768},{"at_ms":400,"buttons":0},{"at_ms":500,"buttons":32768},{"at_ms":600,"buttons":0},{"at_ms":700,"buttons":1}]"#,
    )
    .unwrap();
    let capture = output.join("deko-example04.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
        .arg(&capture)
        .arg("--inputs")
        .arg(input)
        .output()
        .unwrap();
    fs::write(
        output.join("deko-example04.log"),
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
    let texture = decode_bc1(
        &fs::read(std::env::var_os("SOJU_SWITCH_GPU_TEST_BC1").expect("local example BC1 path"))
            .unwrap(),
    );
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("gpu-status|0xb0b5|0x0300|0x00000286|handled")
    );
    let mut interior = 0;
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
            let tx = (u * 256. - 0.5).clamp(0., 255.);
            let ty = (v * 256. - 0.5).clamp(0., 255.);
            let (ix, iy) = (tx.floor() as usize, ty.floor() as usize);
            let neighbors = [
                texture[iy * 256 + ix],
                texture[iy * 256 + (ix + 1).min(255)],
                texture[(iy + 1).min(255) * 256 + ix],
                texture[(iy + 1).min(255) * 256 + (ix + 1).min(255)],
            ];
            interior += 1;
            std::array::from_fn(|c| {
                let fx = tx - ix as f64;
                let fy = ty - iy as f64;
                ((neighbors[0][c] as f64 * (1. - fx) + neighbors[1][c] as f64 * fx) * (1. - fy)
                    + (neighbors[2][c] as f64 * (1. - fx) + neighbors[3][c] as f64 * fx) * fy)
                    .round() as u8
            })
        } else {
            [0, 0, 0, 0]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 6),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
        checked += 1;
    }
    assert!(checked > 800_000);
    assert!(interior > 30_000);
    eprintln!("Example04: {checked} source-derived pixel checks");
    fs::remove_dir_all(home).unwrap();
}

fn decode_bc1(bytes: &[u8]) -> Vec<[u8; 4]> {
    assert_eq!(bytes.len(), 32768);
    let mut image = vec![[0; 4]; 256 * 256];
    for (i, block) in bytes.chunks_exact(8).enumerate() {
        let a = u16::from_le_bytes(block[..2].try_into().unwrap());
        let b = u16::from_le_bytes(block[2..4].try_into().unwrap());
        let rgb = |v: u16| {
            let r = (v >> 11) as u8;
            let g = ((v >> 5) & 63) as u8;
            let b = (v & 31) as u8;
            [
                (r << 3) | (r >> 2),
                (g << 2) | (g >> 4),
                (b << 3) | (b >> 2),
                255,
            ]
        };
        let mut palette = [rgb(a), rgb(b), [0; 4], [0; 4]];
        palette[2] = std::array::from_fn(|c| {
            if c == 3 {
                255
            } else if a > b {
                ((2 * palette[0][c] as u16 + palette[1][c] as u16) / 3) as u8
            } else {
                ((palette[0][c] as u16 + palette[1][c] as u16) / 2) as u8
            }
        });
        palette[3] = std::array::from_fn(|c| {
            if a <= b {
                0
            } else if c == 3 {
                255
            } else {
                ((palette[0][c] as u16 + 2 * palette[1][c] as u16) / 3) as u8
            }
        });
        let indices = u32::from_le_bytes(block[4..].try_into().unwrap());
        for y in 0..4 {
            for x in 0..4 {
                image[(i / 64 * 4 + y) * 256 + i % 64 * 4 + x] =
                    palette[((indices >> (2 * (y * 4 + x))) & 3) as usize];
            }
        }
    }
    image
}

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn opengl_simple_triangle_matches_source() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro =
        std::env::var_os("SOJU_SWITCH_GPU_TEST_GL_NRO").expect("local OpenGL simple_triangle NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("gl-triangle-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let capture = output.join("opengl-simple_triangle.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
        .arg(&capture)
        .output()
        .unwrap();
    fs::write(
        output.join("opengl-simple_triangle.log"),
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
        // Original source vertices: (-.5,-.5) red, (+.5,-.5) green, (0,+.5) blue.
        let blue = (540. - (y as f64 + 0.5)) / 360.;
        let green = ((x as f64 + 0.5 - 320.) / 640.) - blue * 0.5;
        let red = 1. - blue - green;
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
            [51, 76, 76, 255]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
        checked += 1;
    }
    assert!(checked > 910_000);
    eprintln!("OpenGL simple_triangle: {checked} source-derived pixel checks");
    fs::remove_dir_all(home).unwrap();
}
#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_basic_matches_source_vertex_id_triangle() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner path");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_BASIC_NRO").expect("local deko_examples NRO");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("capture directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("basic-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let capture = output.join("deko-basic.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_SWITCH_TRACE_GPU", "1")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(nro)
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
        .arg(&capture)
        .output()
        .unwrap();
    fs::write(
        output.join("deko-basic.log"),
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
            [32, 75, 122, 255]
        };
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
            "pixel ({x},{y}) {p:?} != {expected:?}"
        );
        checked += 1;
    }
    assert!(checked > 910_000);
    eprintln!("deko_basic: {checked} source-derived pixel checks");
    fs::remove_dir_all(home).unwrap();
}
