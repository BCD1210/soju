//! Example07 tick-zero projection and lighting, derived from the public sample.
use std::{fs, io::BufReader, path::PathBuf, process::Command};
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = dot(v, v).sqrt();
    v.map(|x| x / length)
}
#[derive(Clone, Copy)]
struct Vertex {
    screen: [f64; 2],
    position: [f64; 3],
    normal: [f64; 3],
}
fn cross(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

#[test]
#[ignore = "requires a signed runner and local public example mesh"]
fn example07_mesh_lighting_and_background() {
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let nro = std::env::var_os("SOJU_SWITCH_GPU_TEST_NRO").expect("deko examples");
    let mesh = PathBuf::from(
        std::env::var_os("SOJU_SWITCH_GPU_TEST_TEAPOT").expect("local mesh directory"),
    );
    let output =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("proof directory"));
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("mesh-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let mut inputs = Vec::new();
    for i in 0..6 {
        inputs.push(format!(r#"{{"at_ms":{},"buttons":32768}}"#, 100 + i * 200));
        inputs.push(format!(r#"{{"at_ms":{},"buttons":0}}"#, 200 + i * 200));
    }
    inputs.push(r#"{"at_ms":1300,"buttons":1}"#.into());
    let input = home.join("inputs.json");
    fs::write(&input, format!("[{}]", inputs.join(","))).unwrap();
    let capture = output.join("deko-example07.png");
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
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
        output.join("deko-example07.log"),
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
    let bytes = fs::read(mesh.join("teapot-vtx.bin")).unwrap();
    let indices = fs::read(mesh.join("teapot-idx.bin")).unwrap();
    assert_eq!((bytes.len(), indices.len()), (31008, 14784));
    let vertices: Vec<_> = bytes
        .chunks_exact(24)
        .map(|v| {
            let value: Vec<_> = v
                .chunks_exact(4)
                .map(|b| f64::from(f32::from_le_bytes(b.try_into().unwrap())))
                .collect();
            let position = [value[0], value[1] - 0.5, value[2] - 3.];
            let f = 360. / 20f64.to_radians().tan();
            Vertex {
                screen: [
                    640. + f * position[0] / -position[2],
                    360. - f * position[1] / -position[2],
                ],
                position,
                normal: unit([value[3], value[4], value[5]]),
            }
        })
        .collect();
    let mut depth = vec![[f64::INFINITY; 4]; 1280 * 720];
    let mut expected = vec![[[0u8; 3]; 4]; 1280 * 720];
    let mut covered = vec![[false; 4]; 1280 * 720];
    let mut safe = vec![[true; 4]; 1280 * 720];
    for indices in indices.chunks_exact(6) {
        let triangle: Vec<_> = indices
            .chunks_exact(2)
            .map(|b| vertices[usize::from(u16::from_le_bytes(b.try_into().unwrap()))])
            .collect();
        let area = cross(triangle[0].screen, triangle[1].screen, triangle[2].screen);
        if area.abs() < 1e-8 {
            continue;
        }
        let bounds = |axis: usize| {
            let low = triangle
                .iter()
                .map(|v| v.screen[axis])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .max(0.) as usize;
            let high = triangle
                .iter()
                .map(|v| v.screen[axis])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .clamp(0., if axis == 0 { 1279. } else { 719. }) as usize;
            low..=high
        };
        for y in bounds(1) {
            for x in bounds(0) {
                let p = [x as f64 + 0.5, y as f64 + 0.5];
                let weights: [f64; 3] = std::array::from_fn(|i| {
                    cross(
                        triangle[(i + 1) % 3].screen,
                        triangle[(i + 2) % 3].screen,
                        p,
                    ) / area
                });
                let interior = (0..3).all(|i| {
                    let a = triangle[(i + 1) % 3].screen;
                    let b = triangle[(i + 2) % 3].screen;
                    weights[i] * area.abs() / (a[0] - b[0]).hypot(a[1] - b[1]) > 1.5
                });
                let index = y * 1280 + x;
                let mut visible = [false; 4];
                for (sample, (sx, sy)) in [(6., 2.), (14., 6.), (2., 10.), (10., 14.)]
                    .into_iter()
                    .enumerate()
                {
                    let point = [x as f64 + sx / 16., y as f64 + sy / 16.];
                    let sample_weights: [f64; 3] = std::array::from_fn(|i| {
                        cross(
                            triangle[(i + 1) % 3].screen,
                            triangle[(i + 2) % 3].screen,
                            point,
                        ) / area
                    });
                    if sample_weights.iter().any(|w| *w < 0.) {
                        continue;
                    }
                    let z = 1.
                        / (0..3)
                            .map(|i| sample_weights[i] / -triangle[i].position[2])
                            .sum::<f64>();
                    if z < depth[index][sample] {
                        depth[index][sample] = z;
                        covered[index][sample] = true;
                        visible[sample] = true;
                        safe[index][sample] = interior;
                    }
                }
                if !visible.iter().any(|v| *v) {
                    continue;
                }
                let reciprocal: f64 = (0..3).map(|i| weights[i] / -triangle[i].position[2]).sum();
                let perspective: [f64; 3] =
                    std::array::from_fn(|i| weights[i] / -triangle[i].position[2] / reciprocal);
                let position: [f64; 3] = std::array::from_fn(|j| {
                    (0..3)
                        .map(|i| perspective[i] * triangle[i].position[j])
                        .sum()
                });
                let normal = unit(std::array::from_fn(|j| {
                    (0..3).map(|i| perspective[i] * triangle[i].normal[j]).sum()
                }));
                let light = unit([-position[0], 4. - position[1], 1. - position[2]]);
                let view = unit(position.map(|v| -v));
                let halfway = unit(std::array::from_fn(|i| light[i] + view[i]));
                let diffuse = dot(normal, light).max(0.);
                let specular = dot(normal, halfway).max(0.).powi(64);
                let color: [u8; 3] = std::array::from_fn(|c| {
                    let color = [0.046227, 0.028832, 0.003302][c]
                        + [0.564963, 0.367818, 0.051293][c] * diffuse
                        + 24. * [0.394737, 0.308916, 0.134004][c] * specular;
                    let linear = color / (1. + color);
                    let srgb = if linear <= 0.0031308 {
                        12.92 * linear
                    } else {
                        1.055 * linear.powf(1. / 2.4) - 0.055
                    };
                    (srgb * 255.).round() as u8
                });
                for sample in 0..4 {
                    if visible[sample] {
                        expected[index][sample] = color;
                    }
                }
            }
        }
    }
    let mut checked = [0usize; 2];
    for y in 2..718 {
        for x in 2..1278 {
            let index = y * 1280 + x;
            // Compare resolved lighting only when all samples are covered or
            // all are background; silhouettes have a separate MSAA oracle.
            if covered[index].iter().any(|v| *v != covered[index][0]) {
                continue;
            }
            if safe[index].iter().any(|v| !v) {
                continue;
            }
            if !covered[index][0] && (y - 2..=y + 2).any(|yy| (x - 2..=x + 2).any(|xx| covered[yy * 1280 + xx].iter().any(|v| *v))) { continue; }
            let rgb: [u8; 3] = std::array::from_fn(|c| {
                let linear = expected[index]
                    .iter()
                    .map(|sample| {
                        let s = f64::from(sample[c]) / 255.;
                        if s <= 0.04045 {
                            s / 12.92
                        } else {
                            ((s + 0.055) / 1.055).powf(2.4)
                        }
                    })
                    .sum::<f64>()
                    / 4.;
                let s = if linear <= 0.0031308 {
                    linear * 12.92
                } else {
                    1.055 * linear.powf(1. / 2.4) - 0.055
                };
                (s * 255.).round() as u8
            });
            checked[usize::from(covered[index][0])] += 1;
            let actual = &pixels[index * 4..index * 4 + 3];
            assert!(
                actual.iter().zip(rgb).all(|(a, b)| a.abs_diff(b) <= 3),
                "pixel ({x},{y}): {actual:?} != {rgb:?}"
            );
        }
    }
    assert!(
        checked[0] > 500000 && checked[1] > 10000,
        "insufficient source-derived coverage {checked:?}"
    );
    eprintln!(
        "Example07: background={} lighting={} source-derived checks",
        checked[0], checked[1]
    );
    fs::remove_dir_all(home).unwrap();
}
