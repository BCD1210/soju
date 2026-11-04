//! Source-derived acceptance for the public deko3d compute-generated smooth line at tick zero.
use std::{fs, io::BufReader, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn deko_example09_draws_compute_generated_sine_with_smooth_coverage() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("local deko examples");
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("proof directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("lines-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let mut inputs = Vec::new();
    for i in 0..8 {
        inputs.push(format!(r#"{{"at_ms":{},"buttons":32768}}"#, 100 + i * 200));
        inputs.push(format!(r#"{{"at_ms":{},"buttons":0}}"#, 200 + i * 200));
    }
    inputs.push(r#"{"at_ms":1700,"buttons":1}"#.into());
    let input = home.join("inputs.json");
    fs::write(&input, format!("[{}]", inputs.join(","))).unwrap();
    let capture = output.join("deko-example09.png");
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
        output.join("deko-example09.log"),
        [&result.stdout[..], &result.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = String::from_utf8_lossy(&result.stderr);
    assert!(log.contains("gpu-status|0xb1c0|0x02bc|0x00000003|handled"));
    assert!(log.contains("gpu-status|0xb197|0x1570|0x00000001|handled"));
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(capture).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1280, 720, png::ColorType::Rgba)
    );
    // Tick zero: 256 vertices, x in [0,1], y=sin(2*pi*x), white-to-black.
    // Integrate each independent width-16 rectangle on a subpixel grid, then
    // apply source-alpha / one-minus-source-alpha blending (separate alpha is off).
    let mut expected = vec![[0f64; 2]; 1280 * 720];
    let vertex = |i: usize| {
        let t = i as f64 / 255.;
        [1280. * t, 360. - 360. * (t * std::f64::consts::TAU).sin()]
    };
    for i in 0..255 {
        let a = vertex(i);
        let b = vertex(i + 1);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]);
        let tangent = d.map(|v| v / len);
        let coordinates = |x: f64, y: f64| {
            let p = [x - a[0], y - a[1]];
            [
                p[0] * tangent[0] + p[1] * tangent[1],
                -p[0] * tangent[1] + p[1] * tangent[0],
            ]
        };
        let inside = |p: [f64; 2]| (0. ..=len).contains(&p[0]) && p[1].abs() <= 8.;
        let x0 = (a[0].min(b[0]) - 9.).floor().max(0.) as usize;
        let x1 = (a[0].max(b[0]) + 9.).ceil().min(1280.) as usize;
        let y0 = (a[1].min(b[1]) - 9.).floor().max(0.) as usize;
        let y1 = (a[1].max(b[1]) + 9.).ceil().min(720.) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let corners = [[0., 0.], [1., 0.], [0., 1.], [1., 1.]]
                    .map(|p| coordinates(x as f64 + p[0], y as f64 + p[1]));
                let coverage = if corners.iter().all(|&p| inside(p)) {
                    1.
                } else if corners.iter().all(|p| p[0] < 0.)
                    || corners.iter().all(|p| p[0] > len)
                    || corners.iter().all(|p| p[1] < -8.)
                    || corners.iter().all(|p| p[1] > 8.)
                {
                    0.
                } else {
                    let mut hits = 0;
                    for sy in 0..64 {
                        for sx in 0..64 {
                            if inside(coordinates(
                                x as f64 + (sx as f64 + 0.5) / 64.,
                                y as f64 + (sy as f64 + 0.5) / 64.,
                            )) {
                                hits += 1;
                            }
                        }
                    }
                    hits as f64 / 4096.
                };
                let t = (coordinates(x as f64 + 0.5, y as f64 + 0.5)[0] / len).clamp(0., 1.);
                let gray = 1. - (i as f64 + t) / 255.;
                let p = &mut expected[y * 1280 + x];
                p[0] = ((gray * coverage + p[0] * (1. - coverage)) * 255.).round() / 255.;
                p[1] = ((coverage * coverage + p[1] * (1. - coverage)) * 255.).round() / 255.;
            }
        }
    }
    let mut partial = 0;
    for (i, p) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let e = expected[i].map(|v| (v * 255.).round() as u8);
        assert!(
            p[..3].iter().all(|v| v.abs_diff(e[0]) <= 4) && p[3].abs_diff(e[1]) <= 4,
            "pixel ({},{}) {p:?}, expected gray/alpha {e:?}",
            i % 1280,
            i / 1280
        );
        if (1..254).contains(&e[1]) {
            partial += 1;
        }
    }
    assert!(partial > 3000);
    eprintln!("Example09: 921600 source-derived pixels, {partial} partial coverage pixels");
    fs::remove_dir_all(home).unwrap();
}
