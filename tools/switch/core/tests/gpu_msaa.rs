//! Source-derived acceptance for the public deko3d multisample cube at tick zero.
use std::{fs, io::BufReader, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example06_resolves_source_cube_with_four_sample_edge_coverage() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko examples");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("proof directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("msaa-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let mut inputs = Vec::new();
    for i in 0..5 {
        inputs.push(format!(r#"{{"at_ms":{},"buttons":32768}}"#, 100 + i * 200));
        inputs.push(format!(r#"{{"at_ms":{},"buttons":0}}"#, 200 + i * 200));
    }
    inputs.push(r#"{"at_ms":1100,"buttons":1}"#.into());
    let input = home.join("inputs.json");
    fs::write(&input, format!("[{}]", inputs.join(","))).unwrap();
    let capture = output.join("deko-example06.png");
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
        output.join("deko-example06.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = String::from_utf8_lossy(&result.stderr);
    assert!(log.contains("gpu-status|0xb197|0x15d0|0x00000002|handled"));
    assert!(log.contains("gpu-status|0x902d|0x08dc|0x00000000|handled"));
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1280, 720, png::ColorType::Rgba)
    );
    // Example06's tick-zero front face: scale .5, translation Z=-3, FOV 40°.
    // The four source sample locations are packed in 0xeaa26e26 (sixteenths).
    let half = 72. / 20f64.to_radians().tan();
    let positions = [(6., 2.), (14., 6.), (2., 10.), (10., 14.)];
    let mut edge_pixels = 0;
    for (i, p) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = ((i % 1280) as f64, (i / 1280) as f64);
        let coverage = positions
            .iter()
            .filter(|(sx, sy)| {
                (640. - half..640. + half).contains(&(x + sx / 16.))
                    && (360. - half..360. + half).contains(&(y + sy / 16.))
            })
            .count();
        if (1..4).contains(&coverage) {
            edge_pixels += 1;
        }
        let u = (x + 0.5 - (640. - half)) / (2. * half);
        let v = (y + 0.5 - (360. - half)) / (2. * half);
        let colors = [1. - v, (u - v).abs(), u.min(v), 1.];
        let expected =
            colors.map(|c| (c.clamp(0., 1.) * 255. * coverage as f64 / 4.).round() as u8);
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 4),
            "pixel ({x},{y}) {p:?} != {expected:?}, coverage={coverage}"
        );
        assert!(
            p[3].abs_diff(expected[3]) <= 1,
            "coverage alpha at ({x},{y})"
        );
    }
    assert!(edge_pixels > 1000, "no multisample silhouette");
    eprintln!(
        "Example06: 921600 source-derived pixel checks; {edge_pixels} partial-coverage edge pixels"
    );
    fs::remove_dir_all(home).unwrap();
}
