//! Independent projection, depth and lighting oracle using the locally supplied mesh.
use std::{fs, io::BufReader, path::PathBuf, process::Command};
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = dot(v, v).sqrt();
    v.map(|x| x / n)
}
#[derive(Clone, Copy)]
struct Vertex {
    screen: [f64; 2],
    position: [f64; 3],
    quat: [f64; 4],
}
fn check(dynamic: bool) {
    let name = if dynamic {
        "dynamic_resolution"
    } else {
        "lenny"
    };
    let runner = std::env::var_os("SOJU_SWITCH_GPU_TEST_RUNNER").expect("signed runner");
    let corpus =
        PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_CORPUS").expect("local corpus"));
    let output = PathBuf::from(std::env::var_os("SOJU_SWITCH_GPU_TEST_OUTPUT").expect("output"));
    let mesh = fs::read_to_string(
        std::env::var_os("SOJU_SWITCH_GPU_TEST_LENNY_MESH").expect("local lenny.c"),
    )
    .unwrap();
    let vertices: Vec<[f64; 6]> = mesh
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("{ ") {
                return None;
            }
            let values: Vec<f64> = line
                .trim_matches(|c: char| c == '{' || c == '}' || c == ',' || c.is_whitespace())
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect();
            Some(values.try_into().unwrap())
        })
        .collect();
    assert_eq!(vertices.len(), 3345);
    fs::create_dir_all(&output).unwrap();
    let home = output.join(format!("{name}-home-{}", std::process::id()));
    fs::create_dir(&home).unwrap();
    let capture = output.join(format!("opengl-{name}.png"));
    let result = Command::new(runner)
        .env("SOJU_SWITCH_HOME", &home)
        .env("SOJU_SWITCH_GPU", "metal")
        .env("SOJU_GPU_ASYNC_PIPELINES", "0")
        .arg("--headless")
        .arg(corpus.join(format!("graphics__opengl__{name}.nro")))
        .args(["--frames", "1", "--capture-tick", "0", "--capture"])
        .arg(&capture)
        .output()
        .unwrap();
    fs::write(
        output.join(format!("opengl-{name}.log")),
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
    let (width, height) = if dynamic { (1920, 1080) } else { (1280, 720) };
    assert_eq!(
        (info.width, info.height, info.color_type),
        (width as u32, height as u32, png::ColorType::Rgba)
    );
    // Both samples use a 40-degree projection. The dynamic sample starts in
    // handheld mode, drawing into the top-left 1280x720 area of a 1080p buffer.
    let mut depth = vec![f64::INFINITY; width * height];
    let mut expected = vec![Some([104u8, 176, 216]); width * height];
    let mut owners = vec![None; width * height];
    let instances = if dynamic { 64 } else { 1 };
    for instance in 0..instances {
        let t = instance as f64 / 63.;
        let a = 2. * t - 1.;
        let translation = if dynamic {
            [4. * a, 1.5 * (t * std::f64::consts::TAU).sin(), a - 3.]
        } else {
            [0., 0., -3.]
        };
        let transformed: Vec<_> = vertices
            .iter()
            .map(|v| {
                let position = std::array::from_fn(|i| 2. * v[i] + translation[i]);
                let n = unit([v[3], v[4], v[5]]);
                let z = (1. + n[2]) / 2.;
                let quat = if z > 0. {
                    let q = z.sqrt();
                    [n[0] / (2. * q), n[1] / (2. * q), q, 0.]
                } else {
                    [1., 0., 0., 0.]
                };
                let f = 360. / 20f64.to_radians().tan();
                Vertex {
                    screen: [
                        640. + position[0] * f / -position[2],
                        360. - position[1] * f / -position[2],
                    ],
                    position,
                    quat,
                }
            })
            .collect();
        for triangle in transformed.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            let cross = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
                (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
            };
            let area = cross(a.screen, b.screen, c.screen);
            if area.abs() < 1e-8 {
                continue;
            }
            let minx = triangle
                .iter()
                .map(|v| v.screen[0])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .max(0.) as usize;
            let maxx = triangle
                .iter()
                .map(|v| v.screen[0])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .clamp(0., 1279.) as usize;
            let miny = triangle
                .iter()
                .map(|v| v.screen[1])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .max(0.) as usize;
            let maxy = triangle
                .iter()
                .map(|v| v.screen[1])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .clamp(0., 719.) as usize;
            for y in miny..=maxy {
                for x in minx..=maxx {
                    let p = [x as f64 + 0.5, y as f64 + 0.5];
                    let weights = [
                        cross(b.screen, c.screen, p) / area,
                        cross(c.screen, a.screen, p) / area,
                        cross(a.screen, b.screen, p) / area,
                    ];
                    if weights.iter().any(|w| *w < 0.) {
                        continue;
                    }
                    let reciprocal: f64 = weights
                        .iter()
                        .zip(triangle)
                        .map(|(w, v)| w / -v.position[2])
                        .sum();
                    let z = 1. / reciprocal;
                    let index = y * width + x;
                    if z >= depth[index] {
                        continue;
                    }
                    depth[index] = z;
                    owners[index] = Some(instance);
                    let safe = weights.iter().enumerate().all(|(i, w)| {
                        let p = triangle[(i + 1) % 3].screen;
                        let q = triangle[(i + 2) % 3].screen;
                        w * area.abs() / (p[0] - q[0]).hypot(p[1] - q[1]) > 1.5
                    });
                    if !safe {
                        expected[index] = None;
                        continue;
                    }
                    let perspective: [f64; 3] =
                        std::array::from_fn(|i| weights[i] / -triangle[i].position[2] / reciprocal);
                    let q: [f64; 4] = std::array::from_fn(|j| {
                        (0..3).map(|i| perspective[i] * triangle[i].quat[j]).sum()
                    });
                    let length = q.iter().map(|v| v * v).sum::<f64>().sqrt();
                    let q = q.map(|v| v / length);
                    let n = [
                        2. * (q[0] * q[2] + q[3] * q[1]),
                        2. * (q[1] * q[2] - q[3] * q[0]),
                        1. - 2. * (q[0] * q[0] + q[1] * q[1]),
                    ];
                    let view: [f64; 3] = std::array::from_fn(|j| {
                        -(0..3)
                            .map(|i| perspective[i] * triangle[i].position[j])
                            .sum::<f64>()
                    });
                    let light = unit([view[0], view[1], view[2] - 0.5]);
                    let view = unit(view);
                    let half = unit(std::array::from_fn(|i| view[i] + light[i]));
                    let color = (255.
                        * (0.1 + 0.4 * dot(n, light).max(0.) + 0.5 * dot(n, half).max(0.).powi(20))
                            .min(1.))
                    .round() as u8;
                    expected[index] = Some([color; 3]);
                }
            }
        }
    }
    let mut counts = vec![0usize; instances + 1];
    let mut failures = 0;
    let mut first = None;
    // Interior lighting is checked densely; background checks stay two pixels
    // from mesh silhouettes, based only on the independently projected mesh.
    for y in 2..height - 2 {
        for x in 2..width - 2 {
            let i = y * width + x;
            let Some(rgb) = expected[i] else {
                continue;
            };
            if owners[i].is_none()
                && (y - 2..=y + 2)
                    .any(|yy| (x - 2..=x + 2).any(|xx| owners[yy * width + xx].is_some()))
            {
                continue;
            }
            // Protect interior samples adjacent to an occluder's silhouette too,
            // not only the edges of the triangle selected at this pixel.
            if (y - 2..=y + 2).any(|yy| {
                (x - 2..=x + 2).any(|xx| {
                    expected[yy * width + xx].is_none() && owners[yy * width + xx] != owners[i]
                })
            }) {
                continue;
            }
            counts[owners[i].unwrap_or(instances)] += 1;
            let pixel = &pixels[i * 4..i * 4 + 4];
            if pixel[..3].iter().zip(rgb).any(|(a, b)| a.abs_diff(b) > 2) {
                failures += 1;
                first.get_or_insert((x, y, pixel.to_vec(), rgb));
            }
        }
    }
    eprintln!(
        "{name}: source-derived pixel counts {counts:?}; mismatches={failures}; first={first:?}"
    );
    assert_eq!(failures, 0);
    assert!(counts[..instances].iter().sum::<usize>() > 1000);
    if dynamic {
        assert!(counts[..instances].iter().filter(|n| **n > 10).count() > 8);
    }
    fs::remove_dir_all(home).unwrap();
}
#[test]
#[ignore = "requires signed runner and local homebrew source"]
fn lenny_geometry_and_lighting() {
    check(false)
}
#[test]
#[ignore = "requires signed runner and local homebrew source"]
fn dynamic_resolution_geometry_and_lighting() {
    check(true)
}
