//! Source-derived front-face and empty-space oracle for the local es2gears sample.
use std::{fs, io::BufReader, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a signed runner and local devkitPro examples"]
fn es2gears_front_faces_match_projected_geometry_and_lighting() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_GEARS_NRO").expect("local es2gears NRO");
    let output = PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("output"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("gears-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let capture = output.join("opengl-es2gears.png");
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
        output.join("opengl-es2gears.log"),
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
        (1280, 720, png::ColorType::Rgba)
    );

    // main.c: T(0,0,-20) Rx(20 degrees) Ry(30 degrees), 60-degree vertical FOV.
    // Intersect camera rays with each front plane. A radius strictly between
    // r0 and the inscribed r1 polygon lies inside the generated front strips
    // regardless of tooth rotation. Edge/side/occlusion pixels are not oracles.
    let (sx, cx) = 20f64.to_radians().sin_cos();
    let (sy, cy) = 30f64.to_radians().sin_cos();
    let inverse_rotation = |[x, y, z]: [f64; 3]| {
        let (y, z) = (cx * y + sx * z, -sx * y + cx * z);
        [cy * x - sy * z, y, sy * x + cy * z]
    };
    let camera = inverse_rotation([0., 0., 20.]);
    let focal = 1. / 30f64.to_radians().tan();
    let diffuse = (5. * sy - 5. * sx * cy + 10. * cx * cy) / 150f64.sqrt();
    // Position, inner/outer radius, width, tooth count and material from main.c.
    let gears = [
        ([-3., -2.], 1., 4., 1., 20., [0.8, 0.1, 0.]),
        ([3.1, -2.], 0.5, 2., 2., 10., [0., 0.8, 0.2]),
        ([-3.1, 4.2], 1.3, 2., 0.5, 10., [0.2, 0.2, 1.]),
    ];
    let mut counts = [0usize; 4];
    for (i, pixel) in pixels[..info.buffer_size()].chunks_exact(4).enumerate() {
        let (x, y) = (i % 1280, i / 1280);
        let ray = inverse_rotation([
            ((x as f64 + 0.5) / 640. - 1.) * (1280. / 720.) / focal,
            (1. - (y as f64 + 0.5) / 360.) / focal,
            -1.,
        ]);
        let mut front = None;
        let mut empty = true;
        for (index, (center, inner, outer, width, teeth, color)) in gears.iter().enumerate() {
            let at = |z| {
                let t = (z - camera[2]) / ray[2];
                [
                    camera[0] + t * ray[0] - center[0],
                    camera[1] + t * ray[1] - center[1],
                ]
            };
            let a = at(width / 2.);
            let b = at(-width / 2.);
            let d = [b[0] - a[0], b[1] - a[1]];
            let t = (-(a[0] * d[0] + a[1] * d[1]) / (d[0] * d[0] + d[1] * d[1])).clamp(0., 1.);
            let nearest = (a[0] + t * d[0]).hypot(a[1] + t * d[1]);
            empty &= nearest > outer + 0.35 + 0.1;
            let radius = a[0].hypot(a[1]);
            if radius > inner + 0.05
                && radius < (outer - 0.35) * (std::f64::consts::PI / teeth).cos() - 0.05
            {
                assert!(front.is_none(), "oracle front surfaces overlap");
                front = Some((index, color.map(|c| (c * diffuse * 255.).round() as u8)));
            }
        }
        let expected = if let Some((index, color)) = front {
            counts[index] += 1;
            Some(color)
        } else if empty {
            counts[3] += 1;
            Some([0, 0, 0])
        } else {
            None
        };
        if let Some(expected) = expected {
            assert!(
                pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 2),
                "pixel ({x},{y}) {pixel:?} != {expected:?}"
            );
        }
    }
    assert!(
        counts[..3].iter().all(|n| *n > 1000),
        "front sample counts {counts:?}"
    );
    assert!(counts[3] > 700_000);
    eprintln!("es2gears source-derived RGB checks: {counts:?}; sides and tooth edges excluded");
    fs::remove_dir_all(home).unwrap();
}
