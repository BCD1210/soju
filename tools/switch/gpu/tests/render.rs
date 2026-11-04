#![cfg(target_os = "macos")]
use soju_switch_gpu::{
    backend::Backend,
    engine::{B197, Engine},
    memory::Memory,
};
use soju_switch_shader::msl::Options;
use soju_switch_texture::Surface;
use std::{collections::BTreeMap, path::PathBuf};
const PT: u64 = 7 << 16;
fn attr(store: bool, address: u64, first: u64, count: u64) -> u64 {
    (if store {
        0xeff0000000000000
    } else {
        0xefd8000000000000
    }) | PT
        | first
        | 255 << 8
        | address << 20
        | 255 << 39
        | (count - 1) << 47
}
fn ipa(dst: u64, address: u64) -> u64 {
    0xe00000000000ff00 | PT | dst | 255 << 20 | address << 28 | 255 << 39 | 7 << 47
}
fn shader(texture: bool, vertex: bool) -> Vec<u8> {
    shader_with_tint(texture, vertex, false)
}
fn shader_with_tint(texture: bool, vertex: bool, tint: bool) -> Vec<u8> {
    let mut h = [0u32; 20];
    let mut code = Vec::new();
    if vertex {
        h[0] = 1 | 1 << 10;
        h[6] = 0xffff;
        h[13] = (15 << 12) | (15 << 16) | (3 << 20);
        for (addr, reg) in [(0x80, 0), (0x90, 4), (0xa0, 8), (0xb0, 12)] {
            code.push(attr(false, addr, reg, 4));
        }
        code.push(0x5c58000000000000 | PT | 12 << 20);
        code.push(attr(true, 0x70, 0, 4));
        code.push(attr(true, 0x80, 4, 4));
        code.push(attr(true, 0x90, 8, 2));
    } else {
        h[0] = 2 | 5 << 10;
        h[18] = 15;
        if texture {
            h[6] = (2 << 8) | (2 << 10);
            code.push(ipa(4, 0x90));
            code.push(ipa(5, 0x94));
            code.push(0xc038000000000000 | PT | 4 << 8 | 255 << 20 | 1 << 29 | 15 << 31);
        } else {
            h[6] = 0xaa;
            for c in 0..4 {
                code.push(ipa(c, 0x80 + c * 4));
            }
        }
    }
    if tint {
        for c in 0..4 {
            code.push(0x4c68000000000000 | PT | c | c << 8 | c << 20 | 2 << 34);
        }
    }
    code.push(0xe30000000000000f | PT);
    let mut bytes = h.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for w in chunk {
            bytes.extend(w.to_le_bytes());
        }
    }
    bytes
}
type ExpectedImage = (&'static str, Vec<Option<[u8; 4]>>);
struct Harness {
    memory: Memory,
    backend: Backend,
    next: u64,
    expected: BTreeMap<u64, ExpectedImage>,
    vs: u64,
    fs: u64,
    tex_fs: u64,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self {
            memory: Memory::default(),
            backend: Backend::default(),
            next: 0x10000,
            expected: BTreeMap::new(),
            vs: 0,
            fs: 0,
            tex_fs: 0,
        };
        h.vs = h.program(shader(false, true));
        h.fs = h.program(shader(false, false));
        h.tex_fs = h.program(shader(true, false));
        h
    }
    fn alloc(&mut self, bytes: Vec<u8>) -> u64 {
        let a = self.next;
        self.next += (bytes.len() as u64).div_ceil(4096) * 4096;
        self.memory.map(a, bytes).unwrap();
        a
    }
    fn program(&mut self, bytes: Vec<u8>) -> u64 {
        let n = bytes.len();
        let a = self.alloc(bytes);
        self.backend
            .program(
                a,
                n,
                Options {
                    texture_count: 1,
                    ..Options::default()
                },
            )
            .unwrap();
        a
    }
    fn methods(&mut self, e: &mut Engine, writes: &[(u32, u32)]) {
        for &(r, v) in writes {
            if let Some(s) = e.method(B197, r, v, &mut self.memory).unwrap() {
                self.backend.prepare(&s, &self.memory).unwrap();
            }
        }
    }
    fn engine(&mut self, name: &'static str, clear: [f32; 4]) -> (Engine, u64) {
        let mut e = Engine::default();
        let a = self.alloc(vec![0; 32 * 32 * 4]);
        self.methods(
            &mut e,
            &[
                (0x800, 0),
                (0x804, a as u32),
                (0x808, 32),
                (0x80c, 32),
                (0x810, 0xd5),
                (0x814, 0),
                (0x818, 1),
                (0x121c, 1),
                (0xd7c, 1),
                (0xa00, 16f32.to_bits()),
                (0xa04, 16f32.to_bits()),
                (0xa08, 1f32.to_bits()),
                (0xa0c, 16f32.to_bits()),
                (0xa10, 16f32.to_bits()),
                (0xa14, 0),
                (0x2040, 0x11),
                (0x2044, self.vs as u32),
                (0x2050, 0),
                (0x2140, 0x51),
                (0x2144, self.fs as u32),
                (0x2150, 4),
                (0xdac, 0x1b02),
                (0xdb0, 0x1b02),
            ],
        );
        for (i, v) in clear.into_iter().enumerate() {
            self.methods(&mut e, &[(0xd80 + i as u32 * 4, v.to_bits())]);
        }
        self.methods(&mut e, &[(0x19d0, 60)]);
        self.expected.insert(
            a,
            (
                name,
                vec![Some(clear.map(|f| (f * 255.).round() as u8)); 1024],
            ),
        );
        (e, a)
    }
    fn vertices(&mut self, e: &mut Engine, vertices: &[[f32; 12]], offsets: &[[f32; 4]]) {
        let data = vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let length = data.len();
        let a = self.alloc(data);
        let data = offsets
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let len = data.len();
        let b = self.alloc(data);
        self.methods(
            e,
            &[
                (0x1c00, 4096 | 48),
                (0x1c04, 0),
                (0x1c08, a as u32),
                (0x1f00, 0),
                (0x1f04, (a + length as u64 - 1) as u32),
                (0x1c10, 4096 | 16),
                (0x1c14, 0),
                (0x1c18, b as u32),
                (0x1c1c, 1),
                (0x1884, 1),
                (0x1f08, 0),
                (0x1f0c, (b + len as u64 - 1) as u32),
            ],
        );
        for (i, stream, offset) in [(0, 0, 0), (1, 0, 16), (2, 0, 32), (3, 1, 0)] {
            self.methods(
                e,
                &[(0x1160 + i * 4, stream | offset << 7 | 1 << 21 | 7 << 27)],
            );
        }
    }
    fn draw(&mut self, e: &mut Engine, count: u32, indexed: bool, next_instance: bool) {
        self.methods(
            e,
            &[
                (0x1618, 4 | if next_instance { 1 << 26 } else { 0 }),
                (if indexed { 0x17e0 } else { 0xd78 }, count),
                (0x1614, 0),
            ],
        );
    }
    fn rect_expected(
        &mut self,
        a: u64,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        color: [u8; 4],
    ) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.expected.get_mut(&a).unwrap().1[y * 32 + x] = Some(color);
            }
        }
    }
}
fn vertex(x: f32, y: f32, z: f32, color: [f32; 4], u: f32, v: f32) -> [f32; 12] {
    [
        x, y, z, 1., color[0], color[1], color[2], color[3], u, v, 0., 0.,
    ]
}
fn quad(x0: f32, y0: f32, x1: f32, y1: f32, z: f32, color: [f32; 4]) -> Vec<[f32; 12]> {
    let v = [
        vertex(x0, y0, z, color, 0., 1.),
        vertex(x1, y0, z, color, 1., 1.),
        vertex(x1, y1, z, color, 1., 0.),
        vertex(x0, y1, z, color, 0., 0.),
    ];
    [0, 1, 2, 0, 2, 3].map(|i| v[i]).to_vec()
}
#[test]
fn method_streams_render_expected_images_on_metal() {
    let mut h = Harness::new();
    h.engine("clear", [0.125, 0.25, 0.5, 1.]);
    let (mut e, a) = h.engine("coloured-triangle", [0., 0., 0., 1.]);
    let v = [
        vertex(-0.75, -0.75, 0.5, [1., 0., 0., 1.], 0., 0.),
        vertex(0.75, -0.75, 0.5, [0., 1., 0., 1.], 0., 0.),
        vertex(0., 0.75, 0.5, [0., 0., 1., 1.], 0., 0.),
    ];
    h.vertices(&mut e, &v, &[[0.; 4]]);
    h.draw(&mut e, 3, false, false);
    for y in 0..32 {
        for x in 0..32 {
            let px = (x as f32 + 0.5 - 16.) / 16.;
            let py = (16. - y as f32 - 0.5) / 16.;
            let b2 = (py + 0.75) / 1.5;
            let b1 = (px + 0.75 - 0.75 * b2) / 1.5;
            let b0 = 1. - b1 - b2;
            let min = b0.min(b1).min(b2);
            h.expected.get_mut(&a).unwrap().1[y * 32 + x] = if min.abs() < 0.015 {
                None
            } else if min > 0. {
                [b0, b1, b2, 1.].map(|v| (v * 255.).round() as u8).into()
            } else {
                Some([0, 0, 0, 255])
            };
        }
    }
    for bc1 in [false, true] {
        let (mut e, a) = h.engine(
            if bc1 { "bc1-quad" } else { "rgba8-quad" },
            [0., 0., 0., 1.],
        );
        let surface = Surface {
            width: if bc1 { 2 } else { 8 },
            height: if bc1 { 2 } else { 8 },
            depth: 1,
            bytes_per_block: if bc1 { 8 } else { 4 },
            block_height_log2: 1,
            block_depth_log2: 0,
        };
        let colors = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ];
        let linear = if bc1 {
            [0xf800u16, 0x07e0, 0x001f, 0xffff]
                .into_iter()
                .flat_map(|c| {
                    let mut v = c.to_le_bytes().to_vec();
                    v.extend([0; 6]);
                    v
                })
                .collect::<Vec<_>>()
        } else {
            (0..64)
                .flat_map(|i| colors[(i / 8 / 4) * 2 + (i % 8 / 4)])
                .collect::<Vec<_>>()
        };
        let tex = h.alloc(surface.swizzle(&linear).unwrap());
        let words = [
            (if bc1 { 0x24 } else { 8 })
                | (2 << 7)
                | (2 << 10)
                | (2 << 13)
                | (2 << 16)
                | (2 << 19)
                | (3 << 22)
                | (4 << 25)
                | (5 << 28),
            tex as u32,
            3 << 21,
            1 << 3,
            (1 << 23) | 7,
            (1 << 31) | 7,
            0,
            0,
        ];
        let tic = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
        let tsc = h.alloc(
            [
                2u32 | 2 << 3 | 2 << 6,
                1 | 1 << 4 | 1 << 6,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        );
        h.methods(
            &mut e,
            &[
                (0xf10, 1),
                (0x1574, 0),
                (0x1578, tic as u32),
                (0x157c, 0),
                (0x155c, 0),
                (0x1560, tsc as u32),
                (0x1564, 0),
                (0x2144, h.tex_fs as u32),
            ],
        );
        h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
        h.draw(&mut e, 6, false, false);
        for y in 0..32 {
            for x in 0..32 {
                h.expected.get_mut(&a).unwrap().1[y * 32 + x] = Some(colors[(y / 16) * 2 + x / 16]);
            }
        }
    }
    let (mut e, a) = h.engine("depth-test", [0., 0., 0., 1.]);
    let z = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0xfe0, 0),
            (0xfe4, z as u32),
            (0xfe8, 0xa),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xff0, 0x1000),
            (0xd90, 1f32.to_bits()),
            (0x19d0, 1),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
        ],
    );
    for (depth, color) in [(0.25, [1., 0., 0., 1.]), (0.75, [0., 1., 0., 1.])] {
        h.vertices(
            &mut e,
            &quad(-0.75, -0.75, 0.75, 0.75, depth, color),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
    }
    h.rect_expected(a, 4, 4, 28, 28, [255, 0, 0, 255]);
    let (mut e, a) = h.engine("alpha-blend", [0., 0., 1., 1.]);
    h.methods(
        &mut e,
        &[
            (0x1360, 1),
            (0x133c, 1),
            (0x1340, 0x8006),
            (0x1344, 0x4302),
            (0x1348, 0x4303),
            (0x134c, 0x8006),
            (0x1350, 0x4001),
            (0x1358, 0x4303),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-0.75, -0.75, 0.75, 0.75, 0.5, [1., 0., 0., 0.5]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 4, 4, 28, 28, [128, 0, 128, 255]);
    let (mut e, a) = h.engine("indexed-instanced", [0., 0., 0., 1.]);
    let v = quad(-0.25, -0.5, 0.25, 0.5, 0.5, [1., 1., 0., 1.]);
    let v = [v[0], v[1], v[2], v[5]];
    h.vertices(&mut e, &v, &[[-0.5, 0., 0., 0.], [0.5, 0., 0., 0.]]);
    let indices = h.alloc(
        [0u16, 1, 2, 0, 2, 3]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0x17c8, 0),
            (0x17cc, indices as u32),
            (0x17d0, 0),
            (0x17d4, (indices + 11) as u32),
            (0x17d8, 1),
            (0x17dc, 0),
        ],
    );
    h.draw(&mut e, 6, true, false);
    h.draw(&mut e, 6, true, true);
    h.rect_expected(a, 4, 8, 12, 24, [255, 255, 0, 255]);
    h.rect_expected(a, 20, 8, 28, 24, [255, 255, 0, 255]);
    let (mut e, a) = h.engine("scissor", [0., 0., 0., 1.]);
    h.methods(
        &mut e,
        &[(0xe00, 1), (0xe04, 8 | 24 << 16), (0xe08, 12 | 20 << 16)],
    );
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0., 1., 1., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 8, 12, 24, 20, [0, 255, 255, 255]);

    let (mut e, a) = h.engine("constant-buffer-snapshots", [0., 0., 0., 1.]);
    let program = h.program(shader_with_tint(false, false, true));
    let cb = h.alloc(
        [0.25f32, 0.5, 0.75, 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0x2144, program as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2490, 0x21),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 0., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    h.memory
        .write(
            cb,
            &[1.0f32, 0., 0.5, 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    h.vertices(&mut e, &quad(0., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 0, 0, 16, 32, [64, 128, 191, 255]);
    h.rect_expected(a, 16, 0, 32, 32, [255, 0, 128, 255]);

    let (mut e, a) = h.engine("stencil-test", [0., 0., 0., 1.]);
    let z = h.alloc(vec![0; 8192]);
    h.methods(
        &mut e,
        &[
            (0xfe4, z as u32),
            (0xfe8, 0x19),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 1f32.to_bits()),
            (0xda0, 0),
            (0x19d0, 3),
            (0x1380, 1),
            (0x1384, 0x1e00),
            (0x1388, 0x1e00),
            (0x138c, 0x1e01),
            (0x1390, 0x207),
            (0x1394, 1),
            (0x1398, 255),
            (0x139c, 255),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-0.5, -0.5, 0.5, 0.5, 0.5, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.methods(&mut e, &[(0x1390, 0x202), (0x138c, 0x1e00), (0x139c, 0)]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0., 1., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 8, 8, 24, 24, [0, 255, 0, 255]);

    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/render-checks");
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    eprintln!("{}", report.diagnostics);
    assert!(report.diagnostics.contains("pipeline_hits=8"));
    assert_eq!(report.images.len(), 10);
    for image in report.images {
        let (name, expected) = &h.expected[&image.address];
        let mut checked = 0;
        let mut failures = Vec::new();
        let mut max_error = 0;
        for (i, (pixel, expected)) in image.rgba.chunks(4).zip(expected).enumerate() {
            if let Some(expected) = expected {
                checked += 1;
                let error = pixel
                    .iter()
                    .zip(expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                max_error = max_error.max(error);
                if error > 2 {
                    failures.push((i % 32, i / 32, pixel.to_vec(), *expected));
                }
            }
        }
        let ppm = directory.join(format!("{name}.ppm"));
        let mut data = b"P6\n32 32\n255\n".to_vec();
        for p in image.rgba.chunks(4) {
            data.extend(&p[..3]);
        }
        std::fs::write(ppm, data).unwrap();
        eprintln!("{name}: checked={checked} max_channel_error={max_error}");
        assert!(
            failures.is_empty(),
            "{name}: {} failed pixels; first {:?}",
            failures.len(),
            failures.first()
        );
    }
    assert_eq!(
        (h.backend.textures.uploads, h.backend.textures.hits),
        if std::env::var("SOJU_GPU_CPU_CACHE_BYTES").as_deref() == Ok("0") {
            (4, 0)
        } else {
            (2, 2)
        }
    );
}

#[test]
fn malformed_draw_resources_fail_before_native_execution() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("unused", [0., 0., 0., 1.]);
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    e.method(B197, 0x1618, 4, &mut h.memory).unwrap();
    let original = e.method(B197, 0xd78, 6, &mut h.memory).unwrap().unwrap();
    for (offset, value, message) in [
        (0x1f04, 0, "vertex stream limit"),
        (0xd7c, 2, "clip depth"),
        (0x121c, 2, "CT_SELECT"),
        (0x810, 0xc1, "render target requires"),
        (0x2080, 0x21, "unsupported enabled shader stage"),
        (0x2100, 0x41, "geometry stage/OUT"),
        (0x744, 1, "transform feedback"),
        (0x380, 1, "transform feedback"),
        (0x1160, 0, "vertex format"),
        (0x2144, 0, "shader extent"),
    ] {
        let mut invalid = original.clone();
        invalid.registers.0.insert(offset, value);
        let error = h.backend.prepare(&invalid, &h.memory).unwrap_err();
        assert!(error.0.contains(message), "{offset:#x}: {error}");
    }
    let mut alias = original.clone();
    alias.registers.0.insert(0x121c, 2 | (1 << 7));
    for offset in (0..=0x20).step_by(4) {
        alias
            .registers
            .0
            .insert(0x840 + offset, original.registers.get(0x800 + offset));
    }
    assert!(
        h.backend
            .prepare(&alias, &h.memory)
            .unwrap_err()
            .0
            .contains("aliased MRT")
    );
    let mut invalid = original.clone();
    invalid.action = soju_switch_gpu::engine::Action::Draw {
        topology: 4,
        first: 0,
        count: 6,
        indexed: true,
        instance: 0,
    };
    assert!(
        h.backend
            .prepare(&invalid, &h.memory)
            .unwrap_err()
            .0
            .contains("index buffer limit")
    );
}

#[test]
fn eight_targets_route_outputs_and_blend_independently() {
    let mut h = Harness::new();
    let (mut e, first) = h.engine("mrt", [0.; 4]);
    let mut addresses = vec![first];
    let mut select = 8;
    for slot in 0..8 {
        select |= (7 - slot) << (4 + slot * 3);
        if slot != 0 {
            let a = h.alloc(vec![0; 4096]);
            addresses.push(a);
            let b = 0x800 + slot * 64;
            h.methods(
                &mut e,
                &[
                    (b + 4, a as u32),
                    (b + 8, 32),
                    (b + 12, 32),
                    (b + 16, 0xd5),
                    (b + 24, 1),
                ],
            );
        }
    }
    h.methods(&mut e, &[(0x121c, select), (0x12e4, 1)]);
    for slot in 0..8 {
        let b = 0x1e00 + slot * 32;
        h.methods(
            &mut e,
            &[
                (0x1360 + slot * 4, 1),
                (b, 1),
                (b + 4, 1),
                (b + 8, if slot % 2 == 0 { 5 } else { 2 }),
                (b + 12, 2),
                (b + 16, 1),
                (b + 20, 2),
                (b + 24, 1),
                (0x19d0, 60 | slot << 6),
            ],
        );
    }
    let mut header = [0u32; 20];
    header[0] = 2 | 5 << 10;
    header[6] = 0xaa;
    header[18] = u32::MAX;
    let mut code = Vec::new();
    for slot in 0..8 {
        for c in 0..4 {
            code.push(ipa(slot * 4 + c, 0x80 + ((c + slot) % 4) * 4));
        }
    }
    code.push(0xe30000000000000f | PT);
    let mut bytes: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for w in chunk {
            bytes.extend(w.to_le_bytes());
        }
    }
    let fs = h.program(bytes);
    h.methods(&mut e, &[(0x2144, fs as u32)]);
    let color = [1., 0.5, 0.25, 0.5];
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, color), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/mrt-checks");
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 8);
    for image in report.images {
        let physical = addresses.iter().position(|a| *a == image.address).unwrap();
        let slot = 7 - physical;
        let mut expected = std::array::from_fn::<_, 4, _>(|c| color[(c + slot) % 4]);
        if physical % 2 == 0 {
            for c in 0..3 {
                expected[c] *= expected[3];
            }
        }
        let expected = expected.map(|v| (v * 255.).round() as u8);
        assert!(
            image
                .rgba
                .chunks_exact(4)
                .all(|p| p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2)),
            "target {physical}"
        );
    }
    eprintln!("MRT: checked=8192, eight routed targets, independent blending");
}

#[test]
fn asynchronous_batches_preserve_depth_stencil_without_cpu_readback() {
    let mut h = Harness::new();
    let (mut e, a) = h.engine("persistent", [0., 0., 0., 1.]);
    let z = h.alloc(vec![0; 8192]);
    h.methods(
        &mut e,
        &[
            (0xfe4, z as u32),
            (0xfe8, 0x19),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 1f32.to_bits()),
            (0x19d0, 3),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
            (0x1380, 1),
            (0x1384, 0x1e00),
            (0x1388, 0x1e00),
            (0x138c, 0x1e01),
            (0x1390, 0x207),
            (0x1394, 1),
            (0x1398, 255),
            (0x139c, 255),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-1., -1., 0., 1., 0.25, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/async-checks");
    let first = h.backend.submit(&root).unwrap();
    h.methods(&mut e, &[(0x1390, 0x202), (0x138c, 0x1e00), (0x139c, 0)]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.75, [0., 0., 1., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    let second = h.backend.submit(&root).unwrap();
    let empty = h.backend.submit(&root).unwrap();
    let sem_address = h.alloc(vec![0; 4]);
    let mut host = soju_switch_gpu::sync::Host::default();
    host.method(0x14, sem_address as u32).unwrap();
    host.method(0x18, 42).unwrap();
    let release = host.method(0x1c, (1 << 24) | 2).unwrap().unwrap();
    release.release(first, &mut h.memory).unwrap();
    assert_eq!(h.memory.read(sem_address, 4).unwrap(), 42u32.to_le_bytes());
    assert!(
        host.method(0x1c, 1)
            .unwrap()
            .unwrap()
            .acquired(&h.memory)
            .unwrap()
    );
    let mut syncpoint = soju_switch_gpu::sync::Syncpoint::default();
    syncpoint.complete(second).unwrap();
    empty.wait().unwrap();
    assert_eq!(syncpoint.value, 1);
    assert!(
        h.memory.read(a, 4096).unwrap().iter().all(|b| *b == 0),
        "fence unexpectedly copied image to CPU"
    );
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 1);
    for (i, p) in report.images[0].rgba.chunks_exact(4).enumerate() {
        assert_eq!(
            p,
            if i % 32 < 16 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 0, 255]
            }
        );
    }
    // A later batch must retain stencil and allow a nearer fragment only on its marked half.
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.1, [0., 1., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    for (i, p) in report.images[0].rgba.chunks_exact(4).enumerate() {
        assert_eq!(
            p,
            if i % 32 < 16 {
                &[0, 255, 0, 255]
            } else {
                &[0, 0, 0, 255]
            }
        );
    }
    let hits = if std::env::var("SOJU_GPU_CACHE_BYTES").as_deref() == Ok("0") {
        "pipeline_hits=0"
    } else {
        "pipeline_hits=1"
    };
    assert!(report.diagnostics.contains(hits));
    eprintln!(
        "persistent depth/stencil and deferred CPU readback: checked=2048; {}",
        report.diagnostics
    );
}

#[test]
fn color_formats_clear_draw_and_write_back_raw_storage() {
    use soju_switch_texture::format::Format;
    let mut h = Harness::new();
    let codes = [
        0xc0, 0xca, 0xcb, 0xcf, 0xd0, 0xd1, 0xd5, 0xd6, 0xde, 0xe0, 0xe4, 0xe5, 0xe8, 0xe9, 0xea,
        0xf2, 0xf3,
    ];
    let mut formats = BTreeMap::new();
    for code in codes {
        let format = soju_switch_gpu::format::color(code).unwrap();
        let (mut e, _) = h.engine("format", [0.; 4]);
        let info = format.info();
        let surface = Surface {
            width: 32,
            height: 32,
            depth: 1,
            bytes_per_block: info.bytes,
            block_height_log2: 0,
            block_depth_log2: 0,
        };
        let a = h.alloc(vec![0; surface.size()]);
        h.methods(
            &mut e,
            &[
                (0x804, a as u32),
                (0x810, code),
                (0xd80, 0.25f32.to_bits()),
                (0xd84, 0.5f32.to_bits()),
                (0xd88, 0.75f32.to_bits()),
                (0xd8c, 1f32.to_bits()),
                (0x19d0, 60),
            ],
        );
        // Integer attachment outputs preserve guest GPR bits, not float conversion.
        let color = if format == Format::R32Uint {
            [1., 0., 0., 1.]
        } else {
            [0.5, 0.25, 1., 1.]
        };
        h.vertices(&mut e, &quad(-1., -1., 0., 1., 0.5, color), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
        formats.insert(a, (format, surface));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/format-target-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    let mut checked = 0;
    for image in report.images {
        let Some((format, surface)) = formats.get(&image.address) else {
            continue;
        };
        let raw = surface
            .deswizzle(h.memory.read(image.address, surface.size()).unwrap())
            .unwrap();
        assert_eq!(image.rgba, soju_switch_gpu::format::rgba(*format, &raw));
        if *format == Format::R32Uint {
            for (i, p) in raw.chunks_exact(4).enumerate() {
                assert_eq!(
                    u32::from_le_bytes(p.try_into().unwrap()),
                    if i % 32 < 16 {
                        1f32.to_bits()
                    } else {
                        0.25f32.to_bits()
                    }
                );
            }
        }
        for (i, p) in image.rgba.chunks_exact(4).enumerate() {
            let drawn = i % 32 < 16;
            let mut expected = if drawn {
                [128u8, 64, 255, 255]
            } else {
                [64, 128, 191, 255]
            };
            match format {
                Format::R8Unorm | Format::R16Float | Format::R32Float => {
                    expected[1] = 0;
                    expected[2] = 0;
                }
                Format::Rg8Unorm | Format::Rg16Float | Format::Rg32Float => expected[2] = 0,
                Format::R32Uint => expected = [255, 0, 0, 255],
                Format::Rgba8Srgb | Format::Bgra8Srgb => {
                    expected = if drawn {
                        [188, 137, 255, 255]
                    } else {
                        [137, 188, 225, 255]
                    };
                }
                _ => {}
            }
            let tolerance = if matches!(format, Format::B5G6R5Unorm | Format::A1Bgr5Unorm) {
                5
            } else {
                2
            };
            assert!(
                p.iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= tolerance),
                "{format:?} pixel {i}: {p:?} expected {expected:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 17 * 1024);
    eprintln!("17 color formats: checked={checked}");
}

#[test]
fn texture_formats_and_tic_swizzles_sample_on_metal() {
    let mut h = Harness::new();
    let cases = [
        (0x3a, 2, false),
        (0x2f, 7, false),
        (0x29, 2, false),
        (0x30, 7, false),
        (1, 7, false),
        (3, 7, false),
        (4, 7, false),
        (8, 2, false),
        (8, 2, true),
        (9, 2, false),
        (0xc, 7, false),
        (0xf, 7, false),
        (0x14, 2, false),
        (0x15, 2, false),
        (0x18, 2, false),
        (0x1b, 7, false),
        (0x1d, 2, false),
        (0x21, 7, false),
        (0x24, 2, false),
        (0x24, 2, true),
        (0x25, 2, false),
        (0x25, 2, true),
        (0x26, 2, false),
        (0x26, 2, true),
        (0x27, 2, false),
        (0x27, 1, false),
        (0x28, 2, false),
        (0x28, 1, false),
        (0x10, 7, false),
        (0x11, 7, false),
        (0x17, 2, false),
        (0x17, 2, true),
        (0x40, 2, false),
        (0x40, 2, true),
        (0x41, 2, false),
        (0x41, 2, true),
        (0x42, 2, false),
        (0x42, 2, true),
        (0x44, 2, false),
        (0x44, 2, true),
        (0x45, 2, false),
        (0x45, 2, true),
        (0x46, 2, false),
        (0x46, 2, true),
    ];
    let mut expected = BTreeMap::new();
    for (code, ty, srgb) in cases {
        let format = soju_switch_gpu::format::texture(code, ty, srgb).unwrap();
        let info = format.info();
        let size = info.block_w * info.block_h;
        let mut linear = vec![0; size * info.bytes];
        // BC7 mode 6 and ASTC LDR void extent with mid-gray RGB and opaque alpha.
        if code == 0x17 {
            for b in linear.chunks_exact_mut(16) {
                b[0] = 0x40;
            }
        }
        if code >= 0x40 {
            for b in linear.chunks_exact_mut(16) {
                b[..8].copy_from_slice(&[0xfc, 0xfd, 255, 255, 255, 255, 255, 255]);
                for channel in 0..3 {
                    b[8 + channel * 2..10 + channel * 2].copy_from_slice(&0x8080u16.to_le_bytes());
                }
                b[14] = 255;
                b[15] = 255;
            }
        }
        let mut pixel = if code >= 0x40 {
            let value = if srgb { 55 } else { 128 };
            [value, value, 255, 0]
        } else {
            [0, 0, 255, 0]
        };
        match code {
            8 => {
                linear.fill(128);
                pixel = [
                    if srgb { 55 } else { 128 },
                    if srgb { 55 } else { 128 },
                    255,
                    0,
                ];
            }
            0x14 | 0x15 => {
                for p in linear.chunks_exact_mut(2) {
                    p.copy_from_slice(
                        &(if code == 0x14 { 0xfc00u16 } else { 0xf800u16 }).to_le_bytes(),
                    );
                }
                pixel = [255, 0, 255, 0];
            }
            0x3a => {
                linear.copy_from_slice(&16384u16.to_le_bytes());
                pixel[1] = 64;
            }
            0x29 => {
                linear.copy_from_slice(&0x400000abu32.to_le_bytes());
                pixel[1] = 64;
            }
            0x2f | 0x30 => {
                linear[..4].copy_from_slice(&0.25f32.to_le_bytes());
                pixel[1] = 64;
            }
            _ => {}
        }
        let w = info.block_w * info.block_w;
        let height = info.block_h * info.block_h;
        let surface = Surface {
            width: info.block_w,
            height: info.block_h,
            depth: 1,
            bytes_per_block: info.bytes,
            block_height_log2: 0,
            block_depth_log2: 0,
        };
        let tex = h.alloc(surface.swizzle(&linear).unwrap());
        // B, R, constant-one, constant-zero exercises actual Metal texture views.
        let words = [
            code | (ty << 7)
                | (ty << 10)
                | (ty << 13)
                | (ty << 16)
                | (4 << 19)
                | (2 << 22)
                | (7 << 25),
            tex as u32,
            3 << 21,
            0,
            (1 << 23) | ((srgb as u32) << 22) | (w as u32 - 1),
            (1 << 31) | (height as u32 - 1),
            0,
            0,
        ];
        let tic = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
        let tsc = h.alloc(
            [
                2u32 | 2 << 3 | 2 << 6,
                1 | 1 << 4 | 1 << 6,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        );
        let (mut e, a) = h.engine("sample-format", [1.; 4]);
        h.methods(
            &mut e,
            &[
                (0x2144, h.tex_fs as u32),
                (0xf10, 1),
                (0x1574, 0),
                (0x1578, tic as u32),
                (0x157c, 0),
                (0x155c, 0),
                (0x1560, tsc as u32),
                (0x1564, 0),
            ],
        );
        h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
        expected.insert(a, (format, pixel));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/format-texture-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    for image in report.images {
        assert!(
            image.rgba.chunks_exact(4).all(|p| p
                .iter()
                .zip(expected[&image.address].1)
                .all(|(a, b)| a.abs_diff(b) <= 1)),
            "{:?}: {:?}",
            expected[&image.address],
            &image.rgba[..4]
        );
    }
    eprintln!(
        "{} texture formats and TIC swizzles: checked={}",
        cases.len(),
        cases.len() * 1024
    );
}

#[test]
fn depth16_and_depth24_stencil_render_on_metal() {
    let mut h = Harness::new();
    for format in [0x13, 0x14] {
        let (mut e, _) = h.engine("depth-format", [0., 0., 0., 1.]);
        let z = h.alloc(vec![0; 4096]);
        h.methods(
            &mut e,
            &[
                (0xfe4, z as u32),
                (0xfe8, format),
                (0x1228, 32),
                (0x122c, 32),
                (0x1230, 1),
                (0x1538, 1),
                (0xd90, 0.5f32.to_bits()),
                (0x19d0, if format == 0x14 { 3 } else { 1 }),
                (0x12cc, 1),
                (0x12e8, 1),
                (0x130c, 0x201),
            ],
        );
        h.vertices(
            &mut e,
            &quad(-1., -1., 1., 1., 0.75, [1., 0., 0., 1.]),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
        h.vertices(
            &mut e,
            &quad(-1., -1., 1., 1., 0.25, [0., 1., 0., 1.]),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/depth-format-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 2);
    for image in report.images {
        assert!(image.rgba.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    }
    eprintln!("depth16 and promoted depth24/stencil: checked=2048");
}

#[test]
fn raster_extras_and_occlusion_results_match_images() {
    let mut h = Harness::new();
    // Signed viewport plus negative-W clip convention and a non-default depth range.
    let (mut e, a) = h.engine("signed-viewport", [0., 0., 0., 1.]);
    let z = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0xa00, (-16f32).to_bits()),
            (0xd7c, 0),
            (0xa08, 0.3f32.to_bits()),
            (0xa14, 0.5f32.to_bits()),
            (0xfe4, z as u32),
            (0xfe8, 0xa),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 0.4f32.to_bits()),
            (0x19d0, 1),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-1., -1., 0., 1., -0.5, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0., 0., 1., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 16, 0, 32, 32, [255, 0, 0, 255]);

    let (mut e, a) = h.engine("clip-distance", [0., 0., 0., 1.]);
    let original = shader(false, true);
    let mut header = original[..80].to_vec();
    header[72..76].copy_from_slice(&1u32.to_le_bytes());
    let mut instructions: Vec<u64> = original[80..]
        .chunks_exact(8)
        .enumerate()
        .filter(|(i, _)| i % 4 != 0)
        .map(|(_, b)| u64::from_le_bytes(b.try_into().unwrap()))
        .collect();
    instructions.insert(instructions.len() - 1, attr(true, 0x2c0, 0, 1));
    for chunk in instructions.chunks(3) {
        header.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for w in chunk {
            header.extend(w.to_le_bytes());
        }
    }
    let vs = h.program(header);
    h.methods(&mut e, &[(0x2044, vs as u32), (0x1510, 1)]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0., 1., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 16, 0, 32, 32, [0, 255, 0, 255]);

    let (mut e, a) = h.engine("point-size", [0., 0., 0., 1.]);
    h.methods(&mut e, &[(0x1518, 4f32.to_bits())]);
    h.vertices(
        &mut e,
        &[vertex(0., 0., 0.5, [1., 1., 0., 1.], 0., 0.)],
        &[[0.; 4]],
    );
    h.methods(&mut e, &[(0x1618, 0), (0xd78, 1), (0x1614, 0)]);
    h.rect_expected(a, 14, 14, 18, 18, [255, 255, 0, 255]);

    let (mut e, a) = h.engine("primitive-restart", [0., 0., 0., 1.]);
    let vertices = quad(-1., -1., 1., 1., 0.5, [1., 0., 1., 1.]);
    h.vertices(&mut e, &vertices, &[[0.; 4]]);
    let indices = h.alloc(
        [0u16, 1, 2, 1234, 3, 4, 5]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0x17c8, 0),
            (0x17cc, indices as u32),
            (0x17d0, 0),
            (0x17d4, (indices + 13) as u32),
            (0x17d8, 1),
            (0x1644, 1),
            (0x1648, 1234),
        ],
    );
    h.draw(&mut e, 7, true, false);
    h.rect_expected(a, 0, 0, 32, 32, [255, 0, 255, 255]);

    let (mut e, a) = h.engine("divisor-two", [0., 0., 0., 1.]);
    h.vertices(
        &mut e,
        &quad(-0.25, -0.5, 0.25, 0.5, 0.5, [1., 0., 0., 0.5]),
        &[[-0.5, 0., 0., 0.], [0.5, 0., 0., 0.]],
    );
    h.methods(
        &mut e,
        &[
            (0x1c1c, 2),
            (0x1360, 1),
            (0x1340, 1),
            (0x1344, 5),
            (0x1348, 6),
            (0x133c, 1),
            (0x134c, 1),
            (0x1350, 1),
            (0x1358, 2),
        ],
    );
    for next in [false, true, true] {
        h.draw(&mut e, 6, false, next);
    }
    h.rect_expected(a, 4, 8, 12, 24, [192, 0, 0, 255]);
    h.rect_expected(a, 20, 8, 28, 24, [128, 0, 0, 255]);

    let (mut e, a) = h.engine("query-and-bias", [0., 0., 0., 1.]);
    let z = h.alloc(vec![0; 4096]);
    let query = h.alloc(vec![0; 4]);
    h.methods(
        &mut e,
        &[
            (0xfe4, z as u32),
            (0xfe8, 0xa),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 1f32.to_bits()),
            (0x19d0, 1),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
            (0x1514, 1),
            (0x1530, 1),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.75, [0., 0., 1., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.methods(
        &mut e,
        &[
            (0x1b00, 0),
            (0x1b04, query as u32),
            (0x1b0c, (1 << 28) | (2 << 23) | (0xc << 12) | 2),
            (0x1514, 0),
            (0x1110, 1),
            (0xdc8, 1),
            (0x15bc, (-65536f32).to_bits()),
        ],
    );
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0., 1., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.methods(&mut e, &[(0x1558, 0)]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.1, [0., 0., 1., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.rect_expected(a, 0, 0, 32, 32, [0, 255, 0, 255]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/raster-extras-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    assert_eq!(
        u32::from_le_bytes(h.memory.read(query, 4).unwrap().try_into().unwrap()),
        1024
    );
    for image in report.images {
        let (name, expected) = &h.expected[&image.address];
        for (i, (p, e)) in image.rgba.chunks_exact(4).zip(expected).enumerate() {
            assert!(
                p.iter().zip(e.unwrap()).all(|(a, b)| a.abs_diff(b) <= 2),
                "{name} at {i}: {p:?} expected {e:?}"
            );
        }
    }
    eprintln!("viewport/clip/point/restart/divisor/bias/query: checked=6144; occlusion=1024");
}

#[test]
#[ignore = "explicit throughput measurement"]
fn microbenchmark_cached_draws() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("benchmark", [0., 0., 0., 1.]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [0.25, 0.5, 0.75, 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/benchmark");
    h.backend.submit(&root).unwrap().wait().unwrap();
    const DRAWS: u32 = 1000;
    let started = std::time::Instant::now();
    for _ in 0..DRAWS {
        h.draw(&mut e, 6, false, false);
    }
    let diagnostic = h.backend.submit(&root).unwrap().wait().unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        diagnostic.contains("encoders=1 command_buffers=1"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("pipeline_hits=1000"), "{diagnostic}");
    assert!(diagnostic.contains("depth_hits=1000"), "{diagnostic}");
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 1);
    assert!(
        report.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [64, 128, 191, 255])
    );
    eprintln!(
        "benchmark {DRAWS} draws: end_to_end_seconds={elapsed:.6} end_to_end_draws_per_second={:.1}; {diagnostic}; checked=1024",
        DRAWS as f64 / elapsed
    );
}

#[test]
fn depth_import_writeback_and_cpu_invalidation() {
    let mut h = Harness::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/depth-io-checks");
    for code in [0x13, 0x14, 0x16, 0xa, 0x19] {
        let bytes = match code {
            0x13 => 2,
            0x19 => 8,
            _ => 4,
        };
        let surface = Surface {
            width: 32,
            height: 32,
            depth: 1,
            bytes_per_block: bytes,
            block_height_log2: 1,
            block_depth_log2: 0,
        };
        let packed = |z: f32, stencil: u8| -> Vec<u8> {
            match code {
                0x13 => ((z * 65535.).round() as u16).to_le_bytes().to_vec(),
                0x14 => ((((z * 16777215.).round() as u32) << 8) | u32::from(stencil))
                    .to_le_bytes()
                    .to_vec(),
                0x16 => (((z * 16777215.).round() as u32) | (u32::from(stencil) << 24))
                    .to_le_bytes()
                    .to_vec(),
                0x19 => {
                    let mut b = z.to_le_bytes().to_vec();
                    b.extend([stencil, 99, 98, 97]);
                    b
                }
                _ => z.to_le_bytes().to_vec(),
            }
        };
        let z = h.alloc(surface.swizzle(&packed(0.25, 1).repeat(1024)).unwrap());
        let (mut e, _) = h.engine("depth-io", [0., 0., 0., 1.]);
        h.methods(
            &mut e,
            &[
                (0xfe4, z as u32),
                (0xfe8, code),
                (0xfec, 0x10),
                (0x1228, 32),
                (0x122c, 32),
                (0x1230, 1),
                (0x1538, 1),
                (0x12cc, 1),
                (0x12e8, 1),
                (0x130c, 0x201),
            ],
        );
        if matches!(code, 0x14 | 0x16 | 0x19) {
            h.methods(
                &mut e,
                &[
                    (0x1380, 1),
                    (0x1384, 0x1e00),
                    (0x1388, 0x1e00),
                    (0x138c, 0x1e00),
                    (0x1390, 0x202),
                    (0x1394, 1),
                    (0x1398, 255),
                    (0x139c, 0),
                ],
            );
        }
        h.vertices(
            &mut e,
            &quad(-1., -1., 1., 1., 0.5, [1., 0., 0., 1.]),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
        h.vertices(
            &mut e,
            &quad(-1., -1., 0., 1., 0.125, [0., 1., 0., 1.]),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
        let report = h.backend.execute(&root, &mut h.memory).unwrap();
        for (i, p) in report.images[0].rgba.chunks_exact(4).enumerate() {
            assert_eq!(
                p,
                if i % 32 < 16 {
                    &[0, 255, 0, 255]
                } else {
                    &[0, 0, 0, 255]
                }
            );
        }
        let raw = surface
            .deswizzle(h.memory.read(z, surface.size()).unwrap())
            .unwrap();
        for (i, p) in raw.chunks_exact(bytes).enumerate() {
            let expected = packed(if i % 32 < 16 { 0.125 } else { 0.25 }, 1);
            assert_eq!(p, expected, "depth {code:x} raw pixel {i}");
        }
        h.memory
            .write(z, &surface.swizzle(&packed(0.75, 2).repeat(1024)).unwrap())
            .unwrap();
        if matches!(code, 0x14 | 0x16 | 0x19) {
            h.methods(&mut e, &[(0x1394, 2)]);
        }
        h.vertices(
            &mut e,
            &quad(-1., -1., 1., 1., 0.5, [0., 0., 1., 1.]),
            &[[0.; 4]],
        );
        h.draw(&mut e, 6, false, false);
        let report = h.backend.execute(&root, &mut h.memory).unwrap();
        assert!(
            report.images[0]
                .rgba
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        let raw = surface
            .deswizzle(h.memory.read(z, surface.size()).unwrap())
            .unwrap();
        assert!(
            raw.chunks_exact(bytes).all(|p| p == packed(0.5, 2)),
            "depth format {code:x}"
        );
    }
    eprintln!(
        "depth import/writeback/CPU invalidation: checked=10240 color pixels and 10240 raw depth/stencil pixels"
    );
}

#[test]
fn vertex_streams_and_constants_have_independent_binding_spaces() {
    let mut h = Harness::new();
    let mut vertex = shader(false, true);
    // Replace the fixture's position-offset add with cbuf0[0].
    for word in vertex[80..].chunks_exact_mut(8) {
        if u64::from_le_bytes(word.try_into().unwrap()) == 0x5c58000000000000 | PT | 12 << 20 {
            word.copy_from_slice(&(0x4c58000000000000u64 | PT).to_le_bytes());
        }
    }
    let vs = h.program(vertex);
    let cb = h.alloc(0.5f32.to_le_bytes().to_vec());
    let (mut e, _) = h.engine("vertex-binding-spaces", [0., 0., 0., 1.]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 0., 1., 0.5, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.methods(
        &mut e,
        &[
            (0x2044, vs as u32),
            (0x2380, 4),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2410, 1),
        ],
    );
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/vertex-bindings");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    for (i, pixel) in report.images[0].rgba.chunks_exact(4).enumerate() {
        assert_eq!(
            pixel,
            if (8..24).contains(&(i % 32)) {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 0, 255]
            }
        );
    }
}

#[test]
fn unsigned_integer_texture_preserves_bits_through_sampling() {
    let mut h = Harness::new();
    let surface = Surface {
        width: 2,
        height: 2,
        depth: 1,
        bytes_per_block: 4,
        block_height_log2: 0,
        block_depth_log2: 0,
    };
    let values = [1u32, 0x3f800000, 0xdeadbeef, 0xffffffff];
    let tex = h.alloc(
        surface
            .swizzle(
                &values
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
    );
    let tic = h.alloc(
        [
            0xfu32
                | (4 << 7)
                | (4 << 10)
                | (4 << 13)
                | (4 << 16)
                | (2 << 19)
                | (3 << 22)
                | (4 << 25)
                | (5 << 28),
            tex as u32,
            3 << 21,
            0,
            (1 << 23) | 1,
            (1 << 31) | 1,
            0,
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect(),
    );
    let tsc = h.alloc(
        [
            2u32 | 2 << 3 | 2 << 6,
            1 | 1 << 4 | 1 << 6,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect(),
    );
    let (mut e, _) = h.engine("uint-texture", [0.; 4]);
    let target = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0x804, target as u32),
            (0x810, 0xe4),
            (0x2144, h.tex_fs as u32),
            (0xf10, 1),
            (0x1574, 0),
            (0x1578, tic as u32),
            (0x157c, 0),
            (0x155c, 0),
            (0x1560, tsc as u32),
            (0x1564, 0),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/uint-texture-checks");
    h.backend.execute(&root, &mut h.memory).unwrap();
    let target_surface = Surface {
        width: 32,
        height: 32,
        ..surface
    };
    let raw = target_surface
        .deswizzle(h.memory.read(target, 4096).unwrap())
        .unwrap();
    for (i, p) in raw.chunks_exact(4).enumerate() {
        let expected = values[(i / 32 / 16) * 2 + (i % 32 / 16)];
        assert_eq!(
            u32::from_le_bytes(p.try_into().unwrap()),
            expected,
            "pixel {i}"
        );
    }
    eprintln!("R32Uint nearest sampling: checked=1024 exact integer pixels");
}

#[test]
fn bgra_views_and_srgb_sampling() {
    let mut h = Harness::new();
    let mut expected = BTreeMap::new();
    for srgb in [false, true] {
        let surface = Surface {
            width: 1,
            height: 1,
            depth: 1,
            bytes_per_block: 4,
            block_height_log2: 0,
            block_depth_log2: 0,
        };
        let tex = h.alloc(surface.swizzle(&[64, 128, 192, 255]).unwrap());
        let tic = h.alloc(
            [
                8u32 | (2 << 7)
                    | (2 << 10)
                    | (2 << 13)
                    | (2 << 16)
                    | (4 << 19)
                    | (3 << 22)
                    | (2 << 25)
                    | (5 << 28),
                tex as u32,
                3 << 21,
                0,
                (1 << 23) | ((srgb as u32) << 22),
                1 << 31,
                0,
                0,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        );
        let tsc = h.alloc(
            [
                2u32 | 2 << 3 | 2 << 6,
                1 | 1 << 4 | 1 << 6,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        );
        let (mut e, a) = h.engine("bgra", [0.; 4]);
        h.methods(
            &mut e,
            &[
                (0x2144, h.tex_fs as u32),
                (0xf10, 1),
                (0x1574, 0),
                (0x1578, tic as u32),
                (0x157c, 0),
                (0x155c, 0),
                (0x1560, tsc as u32),
                (0x1564, 0),
            ],
        );
        h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
        expected.insert(
            a,
            if srgb {
                [134u8, 55, 13, 255]
            } else {
                [192, 128, 64, 255]
            },
        );
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/bgra-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    for image in report.images {
        assert!(image.rgba.chunks_exact(4).all(|p| {
            p.iter()
                .zip(expected[&image.address])
                .all(|(a, b)| a.abs_diff(b) <= 1)
        }));
    }
    eprintln!("BGRA and BGRA sRGB texture views: checked=2048");
}

#[test]
#[ignore = "explicit mixed throughput measurement"]
fn throughput_state_mixes() {
    let draws: u32 = std::env::var("SOJU_GPU_BENCH_DRAWS")
        .map(|v| v.parse().expect("SOJU_GPU_BENCH_DRAWS must be an integer"))
        .unwrap_or(2000);
    assert!(draws >= 2 && draws.is_multiple_of(2));
    for mix in ["same", "textures", "constants", "targets"] {
        let mut h = Harness::new();
        let (mut e, first_target) = h.engine("mix", [0., 0., 0., 1.]);
        let second_target = h.alloc(vec![0; 4096]);
        h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
        let cb = h.alloc(vec![0; 16]);
        let mut pools = [0; 2];
        if mix == "constants" {
            let program = h.program(shader_with_tint(false, false, true));
            h.methods(
                &mut e,
                &[
                    (0x2144, program as u32),
                    (0x2380, 16),
                    (0x2384, 0),
                    (0x2388, cb as u32),
                    (0x2490, 0x21),
                ],
            );
        }
        if mix == "textures" {
            let surface = Surface {
                width: 8,
                height: 8,
                depth: 1,
                bytes_per_block: 4,
                block_height_log2: 1,
                block_depth_log2: 0,
            };
            for (i, pool) in pools.iter_mut().enumerate() {
                let pixel = if i == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                let address = h.alloc(surface.swizzle(&pixel.repeat(64)).unwrap());
                let words = [
                    8 | 2 << 7
                        | 2 << 10
                        | 2 << 13
                        | 2 << 16
                        | 2 << 19
                        | 3 << 22
                        | 4 << 25
                        | 5 << 28,
                    address as u32,
                    3 << 21,
                    1 << 3,
                    1 << 23 | 7,
                    1 << 31 | 7,
                    0,
                    0,
                ];
                *pool = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
            }
            let tsc = h.alloc(
                [
                    2u32 | 2 << 3 | 2 << 6,
                    1 | 1 << 4 | 1 << 6,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
            );
            h.methods(
                &mut e,
                &[
                    (0x2144, h.tex_fs as u32),
                    (0xf10, 1),
                    (0x1574, 0),
                    (0x1578, pools[0] as u32),
                    (0x157c, 0),
                    (0x155c, 0),
                    (0x1560, tsc as u32),
                    (0x1564, 0),
                ],
            );
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/mix-{mix}"));
        // Warm both alternating states, then measure including method decode and completion.
        for phase in 0..2 {
            h.backend.enable_profiling();
            let started = std::time::Instant::now();
            let mut decode = std::time::Duration::ZERO;
            let mut prepare = std::time::Duration::ZERO;
            let count = if phase == 0 { 2 } else { draws };
            for i in 0..count {
                let change = match mix {
                    "textures" => Some((0x1578, pools[i as usize % 2] as u32)),
                    "targets" => Some((
                        0x804,
                        if i % 2 == 0 {
                            first_target
                        } else {
                            second_target
                        } as u32,
                    )),
                    "constants" => {
                        let color = if i % 2 == 0 {
                            [1f32, 0., 0., 1.]
                        } else {
                            [0f32, 1., 0., 1.]
                        };
                        h.memory
                            .write(
                                cb,
                                &color
                                    .into_iter()
                                    .flat_map(f32::to_le_bytes)
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap();
                        None
                    }
                    _ => None,
                };
                for (offset, value) in
                    change
                        .into_iter()
                        .chain([(0x1618, 4), (0xd78, 6), (0x1614, 0)])
                {
                    let t = std::time::Instant::now();
                    let snapshot = e.method(B197, offset, value, &mut h.memory).unwrap();
                    decode += t.elapsed();
                    if let Some(s) = snapshot {
                        let t = std::time::Instant::now();
                        h.backend.prepare(&s, &h.memory).unwrap();
                        prepare += t.elapsed();
                    }
                }
            }
            let t = std::time::Instant::now();
            let fence = h.backend.submit(&root).unwrap();
            let submit = t.elapsed();
            let diagnostic = fence.wait().unwrap();
            let elapsed = started.elapsed().as_secs_f64();
            if phase == 1 {
                eprintln!(
                    "preparation phases (state, shader/key, resources) {:?}",
                    h.backend.preparation_profile().unwrap()
                );
                eprintln!(
                    "mix={mix} draws={draws} e2e={:.1}/s decode_snapshot={:.6}s prepare={:.6}s submit_io={:.6}s total={elapsed:.6}s; {diagnostic}",
                    draws as f64 / elapsed,
                    decode.as_secs_f64(),
                    prepare.as_secs_f64(),
                    submit.as_secs_f64()
                );
            }
        }
        let report = h.backend.readback(&root, &mut h.memory).unwrap();
        let expected = if matches!(mix, "textures" | "constants") {
            [0, 255, 0, 255]
        } else {
            [255; 4]
        };
        assert_eq!(report.images.len(), if mix == "targets" { 2 } else { 1 });
        for image in report.images {
            assert!(image.rgba.chunks_exact(4).all(|p| p == expected), "{mix}");
        }
    }
}

#[test]
fn borrowed_repeated_draws_preserve_blending_and_constant_uploads() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("repeats", [0., 0., 0., 1.]);
    let program = h.program(shader_with_tint(false, false, true));
    let cb = h.alloc(vec![0; 16]);
    h.methods(
        &mut e,
        &[
            (0x2144, program as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2490, 0x21),
            (0x1360, 1),
            (0x133c, 1),
            (0x1340, 0x8006),
            (0x1344, 0x4302),
            (0x1348, 0x4303),
            (0x134c, 0x8006),
            (0x1350, 0x4001),
            (0x1358, 0x4303),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/repeats-checks");
    for batch in 0..7 {
        let color = if batch % 2 == 0 {
            [1f32, 0., 0., 0.5]
        } else {
            [0f32, 1., 0., 0.5]
        };
        h.memory
            .write(
                cb,
                &color
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        for (offset, value) in [(0x19d0, 60)]
            .into_iter()
            .chain((0..4).flat_map(|_| [(0x1618, 4), (0xd78, 6), (0x1614, 0)]))
        {
            e.method_into(B197, offset, value, &mut h.memory, &mut h.backend)
                .unwrap();
        }
        h.backend.submit(&root).unwrap().wait().unwrap();
        let report = h.backend.readback(&root, &mut h.memory).unwrap();
        let expected = if batch % 2 == 0 {
            [239u8, 0, 0, 255]
        } else {
            [0u8, 239, 0, 255]
        };
        assert!(
            report.images[0]
                .rgba
                .chunks_exact(4)
                .all(|p| p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1))
        );
    }
}

#[test]
fn queued_batches_keep_captured_constants_alive_across_ring_wraps() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("queued", [0., 0., 0., 1.]);
    let program = h.program(shader_with_tint(false, false, true));
    let cb = h.alloc(vec![0; 16]);
    h.methods(
        &mut e,
        &[
            (0x2144, program as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2490, 0x21),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/queued-checks");
    let mut fences = Vec::new();
    for batch in 0..24 {
        let color = if batch % 2 == 0 {
            [1f32, 0., 0., 1.]
        } else {
            [0f32, 1., 0., 1.]
        };
        h.memory
            .write(
                cb,
                &color
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        for (offset, value) in [(0x1618, 4), (0xd78, 6), (0x1614, 0)] {
            e.method_into(B197, offset, value, &mut h.memory, &mut h.backend)
                .unwrap();
        }
        fences.push(h.backend.submit(&root).unwrap());
    }
    for fence in fences {
        fence.wait().unwrap();
    }
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    assert!(
        report.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
}

#[test]
fn prepared_variants_revalidate_vertex_writes_and_public_register_changes() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("variants", [0., 0., 0., 1.]);
    h.vertices(
        &mut e,
        &quad(-1., -1., 1., 1., 0.5, [1., 0., 0., 1.]),
        &[[0.; 4]],
    );
    h.methods(&mut e, &[(0xe00, 1), (0xe08, 32 << 16)]);
    for x in [16 << 16, 16 | (32 << 16), 16 << 16] {
        e.registers.0.insert(0xe04, x);
        h.draw(&mut e, 6, false, false);
    }
    let vertices = quad(-1., -1., 1., 1., 0.5, [0., 1., 0., 1.]);
    let bytes = vertices
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    h.memory.write(e.registers.address(0x1c04), &bytes).unwrap();
    for x in [16 | (32 << 16), 16 << 16] {
        e.registers.0.insert(0xe04, x);
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/variants-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    assert!(
        report.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
}

#[test]
fn independent_uniform_slots_keep_unchanged_offsets() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("uniforms", [0., 0., 0., 1.]);
    let mut code = shader_with_tint(false, false, true);
    for instruction in code[80..].chunks_exact_mut(8) {
        let mut word = u64::from_le_bytes(instruction.try_into().unwrap());
        if word >> 48 == 0x4c68 && word & 1 != 0 {
            word = (word & !(31 << 34)) | (3 << 34);
            instruction.copy_from_slice(&word.to_le_bytes());
        }
    }
    let program = h.program(code);
    let changing = h.alloc(vec![0; 16]);
    let stable = h.alloc([1f32; 4].into_iter().flat_map(f32::to_le_bytes).collect());
    h.methods(
        &mut e,
        &[
            (0x2144, program as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, changing as u32),
            (0x2490, 0x21),
            (0x2388, stable as u32),
            (0x2490, 0x31),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    for i in 0..4 {
        let tint = if i % 2 == 0 {
            [1f32, 0., 0., 0.]
        } else {
            [0f32, 0., 1., 0.]
        };
        h.memory
            .write(
                changing,
                &tint
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/uniform-slot-checks");
    let diagnostic = h.backend.submit(&root).unwrap().wait().unwrap();
    assert!(diagnostic.contains("offset_writes=3 "), "{diagnostic}");
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    assert!(
        report.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [0, 255, 255, 255])
    );
}

#[test]
fn independent_target_passes_coalesce_without_reordering_blends() {
    let mut h = Harness::new();
    let (mut e, a) = h.engine("pass-order", [0., 0., 0., 1.]);
    let b = h.alloc(vec![0; 4096]);
    h.methods(&mut e, &[(0x804, b as u32), (0x19d0, 60)]);
    let program = h.program(shader_with_tint(false, false, true));
    let cb = h.alloc(vec![0; 16]);
    h.methods(
        &mut e,
        &[
            (0x2144, program as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2490, 0x21),
            (0x1360, 1),
            (0x133c, 1),
            (0x1340, 0x8006),
            (0x1344, 0x4302),
            (0x1348, 0x4303),
            (0x134c, 0x8006),
            (0x1350, 0x4001),
            (0x1358, 0x4303),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    for i in 0..8 {
        let tint = [
            [1f32, 0., 0., 0.5],
            [0., 0., 1., 0.5],
            [0., 1., 0., 0.5],
            [1., 1., 0., 0.5],
        ][i % 4];
        h.memory
            .write(
                cb,
                &tint
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        h.methods(&mut e, &[(0x804, if i % 2 == 0 { a } else { b } as u32)]);
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pass-order-checks");
    let diagnostic = h.backend.submit(&root).unwrap().wait().unwrap();
    assert!(diagnostic.contains("encoders=4 "), "{diagnostic}");
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    for image in report.images {
        let expected = if image.address == a {
            [80u8, 159, 0, 255]
        } else {
            [159u8, 159, 80, 255]
        };
        assert!(
            image
                .rgba
                .chunks_exact(4)
                .all(|p| p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2))
        );
    }
}

#[test]
fn shared_depth_prevents_target_pass_reordering() {
    let mut h = Harness::new();
    let (mut e, a) = h.engine("shared-depth", [0., 0., 0., 1.]);
    let b = h.alloc(vec![0; 4096]);
    h.methods(&mut e, &[(0x804, b as u32), (0x19d0, 60)]);
    let z = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0xfe0, 0),
            (0xfe4, z as u32),
            (0xfe8, 0xa),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 1f32.to_bits()),
            (0x19d0, 1),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
        ],
    );
    for (target, depth, color) in [
        (a, 0.6, [1., 0., 0., 1.]),
        (b, 0.4, [0., 1., 0., 1.]),
        (a, 0.5, [0., 0., 1., 1.]),
    ] {
        h.methods(&mut e, &[(0x804, target as u32)]);
        h.vertices(&mut e, &quad(-1., -1., 1., 1., depth, color), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shared-depth-order-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    let merged = std::env::var("SOJU_GPU_MERGE_DEPTH").as_deref() != Ok("0");
    assert!(
        report.diagnostics.contains(if merged {
            "merged_depth_draws=3"
        } else {
            "merged_depth_draws=0"
        }),
        "{}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .contains(if merged { "encoders=4 " } else { "encoders=6 " }),
        "{}",
        report.diagnostics
    );
    eprintln!("shared_depth {}", report.diagnostics);
    for image in report.images {
        let expected = if image.address == a {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        assert!(image.rgba.chunks_exact(4).all(|p| p == expected));
    }
}

#[test]
#[ignore = "explicit first-seen shader latency measurement"]
fn first_seen_shader_frames() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("first-seen", [0., 0., 0., 1.]);
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/first-seen");
    h.backend.submit(&root).unwrap().wait().unwrap();
    let mut times = Vec::new();
    let mut skipped = 0;
    for frame in 0..300 {
        let start = std::time::Instant::now();
        h.methods(
            &mut e,
            &[
                (0x1518, (1.0 + (frame / 10) as f32 * 0.125).to_bits()),
                (0x1618, 0),
                (0xd78, 1),
                (0x1614, 0),
            ],
        );
        let diagnostic = h.backend.submit(&root).unwrap().wait().unwrap();
        if diagnostic.contains("skipped_draws=1 ") {
            skipped += 1;
        }
        times.push(start.elapsed().as_secs_f64() * 1000.);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    times.sort_by(f64::total_cmp);
    eprintln!(
        "first_seen frames=300 new_every=10 p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3} skipped={skipped}",
        times[150], times[285], times[297], times[299]
    );
    // A final repeated shader must eventually render even in nonblocking mode.
    for _ in 0..200 {
        h.methods(&mut e, &[(0x1618, 0), (0xd78, 1), (0x1614, 0)]);
        let diagnostic = h.backend.submit(&root).unwrap().wait().unwrap();
        if !diagnostic.contains("skipped_draws=1 ") {
            let report = h.backend.readback(&root, &mut h.memory).unwrap();
            assert!(report.images[0].rgba.chunks_exact(4).any(|p| p == [255; 4]));
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("background pipeline failed to become ready");
}

#[test]
fn scissored_color_clear_preserves_outside_and_accepts_single_layer_stride() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("scissored-clear", [0., 0., 1., 1.]);
    h.methods(
        &mut e,
        &[
            (0x81c, 1024),
            (0xe00, 1),
            (0xe04, (23 << 16) | 7),
            (0xe08, (19 << 16) | 5),
            (0xd80, 1f32.to_bits()),
            (0xd88, 0),
            (0x19d0, 60),
        ],
    );
    let dir = std::env::temp_dir().join(format!("soju-scissor-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let report = h.backend.execute(&dir, &mut h.memory).unwrap();
    for (i, pixel) in report.images[0].rgba.chunks_exact(4).enumerate() {
        let inside = (7..23).contains(&(i % 32)) && (5..19).contains(&(i / 32));
        assert_eq!(
            pixel,
            if inside {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            },
            "pixel {i}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "explicit 30-minute resource churn soak"]
fn resource_churn_soak() {
    let seconds: u64 = std::env::var("SOJU_GPU_SOAK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1800);
    let label = std::env::var("SOJU_GPU_SOAK_LABEL").unwrap_or_else(|_| "bounded".into());
    assert!(
        label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/soak-{label}"));
    let mut h = Harness::new();
    let (mut e, first) = h.engine("soak", [0., 0., 0., 1.]);
    let mut targets = vec![first];
    for _ in 0..63 {
        targets.push(h.alloc(vec![0; 4096]));
    }
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    let surface = Surface {
        width: 8,
        height: 8,
        depth: 1,
        bytes_per_block: 4,
        block_height_log2: 1,
        block_depth_log2: 0,
    };
    let address = h.alloc(surface.swizzle(&[255u8; 256]).unwrap());
    let words = [
        8 | 2 << 7 | 2 << 10 | 2 << 13 | 2 << 16 | 2 << 19 | 3 << 22 | 4 << 25 | 5 << 28,
        address as u32,
        3 << 21,
        1 << 3,
        1 << 23 | 7,
        1 << 31 | 7,
        0,
        0,
    ];
    let tic = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
    let tsc = h.alloc(
        [
            2u32 | 2 << 3 | 2 << 6,
            1 | 1 << 4 | 1 << 6,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0x2144, h.tex_fs as u32),
            (0xf10, 1),
            (0x1574, 0),
            (0x1578, tic as u32),
            (0x157c, 0),
            (0x155c, 0),
            (0x1560, tsc as u32),
            (0x1564, 0),
        ],
    );
    h.backend.submit(&root).unwrap().wait().unwrap();
    h.backend.readback(&root, &mut h.memory).unwrap();
    let start = std::time::Instant::now();
    let mut frame = 0;
    let mut next_sample = 0;
    while start.elapsed().as_secs() < seconds {
        let pixel = [(frame & 255) as u8, ((frame >> 8) & 255) as u8, 127, 255];
        h.memory
            .write(address, &surface.swizzle(&pixel.repeat(64)).unwrap())
            .unwrap();
        h.methods(&mut e, &[(0x804, targets[frame % targets.len()] as u32)]);
        // Vary validated blend descriptors while preserving the full opaque image.
        h.methods(
            &mut e,
            &[
                (0x1360, 1),
                (0x1340, 1),
                (0x1344, 2),
                (0x1348, 1),
                (0x1394, (frame % 256) as u32),
            ],
        );
        h.draw(&mut e, 6, false, false);
        let stats = h.backend.submit(&root).unwrap().wait().unwrap();
        if let Some(bytes) = stats
            .split_whitespace()
            .find_map(|s| s.strip_prefix("cache_bytes="))
            .and_then(|s| s.parse::<usize>().ok())
        {
            let budget = stats
                .split_whitespace()
                .find_map(|s| s.strip_prefix("cache_budget="))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert!(bytes <= budget, "{stats}");
        }
        let report = h.backend.readback(&root, &mut h.memory).unwrap();
        assert_eq!(report.images.len(), 1);
        assert!(report.images[0].rgba.chunks_exact(4).all(|p| p == pixel));
        if start.elapsed().as_secs() >= next_sample {
            let rss = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .unwrap();
            eprintln!(
                "soak label={label} seconds={} frame={frame} rss_kib={} {stats}",
                start.elapsed().as_secs(),
                String::from_utf8_lossy(&rss.stdout).trim()
            );
            next_sample += 60;
        }
        frame += 1;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!(
        "soak_complete label={label} seconds={} frames={frame} checked_pixels={}",
        start.elapsed().as_secs(),
        frame * 1024
    );
}

#[test]
fn parallel_color_passes_preserve_per_target_blend_order() {
    let mut h = Harness::new();
    let (mut e, a) = h.engine("parallel", [0., 0., 0., 1.]);
    let b = h.alloc(vec![0; 4096]);
    h.methods(&mut e, &[(0x804, b as u32), (0x19d0, 60)]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/parallel-checks");
    h.backend.submit(&root).unwrap().wait().unwrap();
    h.methods(
        &mut e,
        &[
            (0x1360, 1),
            (0x133c, 1),
            (0x1340, 0x8006),
            (0x1344, 0x4302),
            (0x1348, 0x4303),
            (0x134c, 0x8006),
            (0x1350, 0x4001),
            (0x1358, 0x4303),
        ],
    );
    for i in 0..8 {
        let color = if i < 4 {
            [1., 0., 0., 0.5]
        } else {
            [0., 1., 0., 0.5]
        };
        h.methods(&mut e, &[(0x804, if i % 2 == 0 { a } else { b } as u32)]);
        h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, color), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
        // Repeat references must be remapped into each independent command list.
        h.draw(&mut e, 6, false, false);
    }
    let stats = h.backend.submit(&root).unwrap().wait().unwrap();
    if std::env::var("SOJU_GPU_PARALLEL_PASSES").as_deref() == Ok("1")
        && std::env::var("SOJU_GPU_CACHE_BYTES").as_deref() != Ok("0")
    {
        assert!(stats.contains("parallel_passes=2"), "{stats}");
        assert!(stats.contains("command_buffers=2"), "{stats}");
    }
    let report = h.backend.readback(&root, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 2);
    for image in report.images {
        assert!(image.rgba.chunks_exact(4).all(|p| {
            p.iter()
                .zip([15u8, 239, 0, 255])
                .all(|(a, b)| a.abs_diff(b) <= 2)
        }));
    }
}

#[test]
fn shared_depth_mrt_slot_permutations_keep_outputs_and_depth_order() {
    let mut h = Harness::new();
    let (mut e, a) = h.engine("depth-mrt", [0., 0., 0., 1.]);
    let b = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0x844, b as u32),
            (0x848, 32),
            (0x84c, 32),
            (0x850, 0xd5),
            (0x858, 1),
            (0x121c, 2 | 1 << 7),
        ],
    );
    let z = h.alloc(vec![0; 4096]);
    h.methods(
        &mut e,
        &[
            (0xfe4, z as u32),
            (0xfe8, 0xa),
            (0x1228, 32),
            (0x122c, 32),
            (0x1230, 1),
            (0x1538, 1),
            (0xd90, 1f32.to_bits()),
            (0x19d0, 1),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x201),
        ],
    );
    let mut header = [0u32; 20];
    header[0] = 2 | 5 << 10;
    header[6] = 0xaa;
    header[18] = 255;
    let mut code = Vec::new();
    for slot in 0..2 {
        for c in 0..4 {
            code.push(ipa(slot * 4 + c, 0x80 + ((c + slot) % 4) * 4));
        }
    }
    code.push(0xe30000000000000f | PT);
    let mut bytes: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for w in chunk {
            bytes.extend(w.to_le_bytes());
        }
    }
    let fs = h.program(bytes);
    h.methods(&mut e, &[(0x2144, fs as u32)]);
    for (select, z, color) in [
        (2 | 1 << 7, 0.6, [1., 0., 0., 1.]),
        (2 | 1 << 4, 0.4, [0., 1., 0., 1.]),
        (2 | 1 << 7, 0.5, [0., 0., 1., 1.]),
    ] {
        h.methods(&mut e, &[(0x121c, select)]);
        h.vertices(&mut e, &quad(-1., -1., 1., 1., z, color), &[[0.; 4]]);
        h.draw(&mut e, 6, false, false);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shared-depth-mrt");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 2);
    for image in report.images {
        let expected = if image.address == a {
            [255, 0, 255, 0]
        } else {
            [0, 255, 0, 255]
        };
        assert!(image.rgba.chunks_exact(4).all(|p| p == expected));
    }
}

#[test]
#[ignore = "run with SOJU_GPU_ASYNC_PIPELINES=1"]
fn pending_pipeline_skips_frame_then_renders() {
    assert_eq!(
        std::env::var("SOJU_GPU_ASYNC_PIPELINES").as_deref(),
        Ok("1")
    );
    let mut h = Harness::new();
    let (mut e, _) = h.engine("pending", [0., 0., 0., 1.]);
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pending-pipeline");
    let stats = h.backend.submit(&root).unwrap().wait().unwrap();
    assert!(
        stats.contains("skipped_draws=1 ") && stats.contains("draws=0 "),
        "{stats}"
    );
    let first = h.backend.readback(&root, &mut h.memory).unwrap();
    assert!(
        first.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [0, 0, 0, 255])
    );
    for _ in 0..200 {
        h.draw(&mut e, 6, false, false);
        let stats = h.backend.submit(&root).unwrap().wait().unwrap();
        if stats.contains("skipped_draws=0 ") {
            let ready = h.backend.readback(&root, &mut h.memory).unwrap();
            assert!(ready.images[0].rgba.chunks_exact(4).all(|p| p == [255; 4]));
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("ready pipeline never rendered");
}

#[test]
fn explicit_fragment_interpolation_multiplier_uses_reciprocal_w() {
    let mut h = Harness::new();
    let mut header = [0u32; 20];
    header[0] = 2 | (5 << 10);
    header[6] = 0xaa;
    header[18] = 15;
    let mut code = vec![0x010000000000f000 | PT | 4 | (u64::from(2f32.to_bits()) << 20)];
    for c in 0..4 {
        code.push((ipa(c, 0x80 + c * 4) & !(255 << 20)) | (4 << 20) | (1 << 54));
    }
    code.push(0xe30000000000000f | PT);
    let mut binary: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        binary.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            binary.extend(word.to_le_bytes());
        }
    }
    let fs = h.program(binary);
    let (mut e, address) = h.engine("ipa-multiply", [0.; 4]);
    h.methods(&mut e, &[(0x2144, fs as u32), (0x1510, 255), (0x1910, 1)]);
    let mut vertices = quad(-1., -1., 1., 1., 0.5, [0.25, 0.5, 0.75, 1.]);
    for v in &mut vertices {
        for component in &mut v[..4] {
            *component *= 2.;
        }
    }
    h.vertices(&mut e, &vertices, &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    h.expected
        .get_mut(&address)
        .unwrap()
        .1
        .fill(Some([64, 128, 191, 255]));
    let directory = std::env::temp_dir().join(format!("soju-ipa-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    for pixel in report.images[0].rgba.chunks_exact(4) {
        assert!(
            pixel
                .iter()
                .zip([64u8, 128, 191, 255])
                .all(|(a, b)| a.abs_diff(b) <= 2),
            "{pixel:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn nonindexed_quad_expands_to_two_triangles() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("quad", [0.; 4]);
    let v = quad(-1., -1., 1., 1., 0.5, [1., 0., 0., 1.]);
    h.vertices(&mut e, &[v[0], v[1], v[2], v[5]], &[[0.; 4]]);
    h.methods(&mut e, &[(0x1618, 7), (0xd78, 4), (0x1614, 0)]);
    let directory = std::env::temp_dir().join(format!("soju-quad-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    assert!(
        report.images[0]
            .rgba
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255])
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn split_texs_exports_four_distinct_components() {
    let mut h = Harness::new();
    let mut header = [0u32; 20];
    header[0] = 2 | (5 << 10);
    header[18] = 15;
    let code = [
        0x010000000000f000 | PT | 4 | (u64::from(0.5f32.to_bits()) << 20),
        0x010000000000f000 | PT | 5 | (u64::from(0.5f32.to_bits()) << 20),
        0xd000000000000000 | PT | (4 << 8) | (5 << 20) | (2 << 28) | (1 << 53) | (4 << 50),
        0xe30000000000000f | PT,
    ];
    let mut binary: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        binary.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            binary.extend(word.to_le_bytes());
        }
    }
    let fs = h.program(binary);
    let (mut e, _) = h.engine("split-texs", [0.; 4]);
    let surface = Surface {
        width: 8,
        height: 8,
        depth: 1,
        bytes_per_block: 4,
        block_height_log2: 1,
        block_depth_log2: 0,
    };
    let tex = h.alloc(surface.swizzle(&[51u8, 102, 153, 204].repeat(64)).unwrap());
    let words = [
        8 | (2 << 7)
            | (2 << 10)
            | (2 << 13)
            | (2 << 16)
            | (2 << 19)
            | (3 << 22)
            | (4 << 25)
            | (5 << 28),
        tex as u32,
        3 << 21,
        (1 << 3) | 0x70000,
        (1 << 23) | 7 | 0xe8000000,
        (1 << 31) | 7,
        0x03000000,
        0,
    ];
    let tic = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
    let tsc = h.alloc(
        [0x26492u32, 0x362, 0x00f00000, 0, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0xf10, 1),
            (0x1574, 0),
            (0x1578, tic as u32),
            (0x157c, 0),
            (0x155c, 0),
            (0x1560, tsc as u32),
            (0x1564, 0),
            (0x2144, fs as u32),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    for encoding in 0..8u32 {
        h.memory
            .write(tsc, &(0x26492 | (encoding << 20)).to_le_bytes())
            .unwrap();
        h.memory.write(tsc + 4, &0x372u32.to_le_bytes()).unwrap();
        h.draw(&mut e, 6, false, false);
    }
    let directory = std::env::temp_dir().join(format!("soju-texs-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    for pixel in report.images[0].rgba.chunks_exact(4) {
        assert!(
            pixel
                .iter()
                .zip([51u8, 102, 153, 204])
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{pixel:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn texs_array_selects_second_layer() {
    let mut h = Harness::new();
    let mut header = [0u32; 20];
    header[0] = 2 | (5 << 10);
    header[18] = 15;
    let code = [
        0x010000000000f000 | PT | 4 | (1 << 20),
        0x010000000000f000 | PT | 5 | (u64::from(0.5f32.to_bits()) << 20),
        0x010000000000f000 | PT | 6 | (u64::from(0.5f32.to_bits()) << 20),
        0xd000000000000000 | PT | (4 << 8) | (6 << 20) | (2 << 28) | (7 << 53) | (4 << 50),
        0xe30000000000000f | PT,
    ];
    let mut binary: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        binary.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            binary.extend(word.to_le_bytes());
        }
    }
    let fs = h.program(binary);
    let (mut e, _) = h.engine("array-texs", [0.; 4]);
    let surface = Surface {
        width: 8,
        height: 8,
        depth: 2,
        bytes_per_block: 4,
        block_height_log2: 1,
        block_depth_log2: 0,
    };
    let tex = h.alloc(surface.swizzle(&[[204u8, 153, 102, 51].repeat(64), [51u8, 102, 153, 204].repeat(64)].concat()).unwrap());
    let words = [
        8 | (2 << 7)
            | (2 << 10)
            | (2 << 13)
            | (2 << 16)
            | (2 << 19)
            | (3 << 22)
            | (4 << 25)
            | (5 << 28),
        tex as u32,
        3 << 21,
        (1 << 3) | 0x70000,
        (5 << 23) | 7 | 0xe8000000,
        (1 << 31) | (1 << 16) | 7,
        0x03000000,
        0,
    ];
    let tic = h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
    let tsc = h.alloc(
        [0x26492u32, 0x362, 0x00f00000, 0, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0xf10, 1),
            (0x1574, 0),
            (0x1578, tic as u32),
            (0x157c, 0),
            (0x155c, 0),
            (0x1560, tsc as u32),
            (0x1564, 0),
            (0x2144, fs as u32),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    let directory = std::env::temp_dir().join(format!("soju-texs-array-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    for pixel in report.images[0].rgba.chunks_exact(4) {
        assert!(
            pixel
                .iter()
                .zip([51u8, 102, 153, 204])
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{pixel:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn instance_builtin_positions_two_separate_draws() {
    let mut h = Harness::new();
    let mut header = [0u32; 20];
    header[0] = 1 | 1 << 10;
    header[6] = 0xfff;
    header[10] = 1 << 30;
    header[13] = (15 << 12) | (15 << 16) | (3 << 20);
    let code = [
        attr(false, 0x80, 0, 4),
        attr(false, 0x90, 4, 4),
        attr(false, 0xa0, 8, 4),
        attr(false, 0x2f8, 12, 1),
        0x5cb8000000000000 | PT | 12 | 12 << 20 | 0xa00,
        0x010000000000f000 | PT | 13 | u64::from((-0.5f32).to_bits()) << 20,
        0x5c58000000000000 | PT | 12 | 12 << 8 | 13 << 20,
        0x5c58000000000000 | PT | 12 << 20,
        attr(true, 0x70, 0, 4),
        attr(true, 0x80, 4, 4),
        attr(true, 0x90, 8, 2),
        0xe30000000000000f | PT,
    ];
    let mut bytes: Vec<_> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            bytes.extend(word.to_le_bytes());
        }
    }
    let vertex = h.program(bytes);
    let (mut e, a) = h.engine("instance-builtin", [0., 0., 0., 1.]);
    h.methods(&mut e, &[(0x2044, vertex as u32)]);
    h.vertices(
        &mut e,
        &quad(-0.25, -0.5, 0.25, 0.5, 0.5, [1., 1., 0., 1.]),
        &[[0.; 4]],
    );
    h.draw(&mut e, 6, false, false);
    h.draw(&mut e, 6, false, true);
    h.rect_expected(a, 4, 8, 12, 24, [255, 255, 0, 255]);
    h.rect_expected(a, 20, 8, 28, 24, [255, 255, 0, 255]);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/instance-builtin");
    let report = h.backend.execute(&dir, &mut h.memory).unwrap();
    assert_eq!(report.images.len(), 1);
    for (pixel, expected) in report.images[0].rgba.chunks_exact(4).zip(&h.expected[&a].1) {
        assert_eq!(pixel, expected.unwrap());
    }
}

#[test]
fn implicit_fragment_group_uses_pixel_constants() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("initial-fragment-group", [0.; 4]);
    e.registers.0.remove(&0x2150);
    let fs = h.program(shader_with_tint(false, false, true));
    let cb = h.alloc(
        [0.25f32, 0.5, 0.75, 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
    );
    h.methods(
        &mut e,
        &[
            (0x2144, fs as u32),
            (0x2380, 16),
            (0x2384, 0),
            (0x2388, cb as u32),
            (0x2490, 0x21),
        ],
    );
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    h.draw(&mut e, 6, false, false);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/initial-fragment-group");
    let report = h.backend.execute(&dir, &mut h.memory).unwrap();
    for pixel in report.images[0].rgba.chunks_exact(4) {
        assert_eq!(pixel, [64, 128, 191, 255]);
    }
}

#[test]
fn repeated_program_registration_observes_changed_shader_bytes() {
    fn constant_fragment(value: f32) -> Vec<u8> {
        let mut header = [0u32; 20];
        header[0] = 2 | (5 << 10);
        header[18] = 15;
        let mut words: Vec<u64> = (0..4)
            .map(|r| 0x010000000000f000 | PT | r | (u64::from(value.to_bits()) << 20))
            .collect();
        words.push(0xe30000000000000f | PT);
        let mut binary: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
        for chunk in words.chunks(3) {
            binary.extend(0x001f8000fc0007e0u64.to_le_bytes());
            for word in chunk {
                binary.extend(word.to_le_bytes());
            }
        }
        binary
    }
    let mut h = Harness::new();
    let binary = constant_fragment(0.25);
    let size = binary.len();
    let fs = h.program(binary);
    let (mut e, _) = h.engine("registered-program", [0.; 4]);
    h.methods(&mut e, &[(0x2144, fs as u32)]);
    h.vertices(&mut e, &quad(-1., -1., 1., 1., 0.5, [1.; 4]), &[[0.; 4]]);
    let directory = std::env::temp_dir().join(format!("soju-registration-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (value, expected) in [(0.25, 64u8), (0.25, 64), (0.75, 191)] {
        if value == 0.75 {
            h.memory.write(fs, &constant_fragment(value)).unwrap();
        }
        h.backend
            .program(
                fs,
                size,
                Options {
                    texture_count: 1,
                    ..Options::default()
                },
            )
            .unwrap();
        h.draw(&mut e, 6, false, false);
        let report = h.backend.execute(&directory, &mut h.memory).unwrap();
        assert!(
            report.images[0]
                .rgba
                .iter()
                .all(|channel| channel.abs_diff(expected) <= 1)
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn shader_selects_all_sixteen_viewports_and_scissors() {
    let mut h = Harness::new();
    let mut header = [0u32; 20];
    header[0] = 1 | 1 << 10;
    header[6] = 0x1ff;
    header[13] = (1 << 10) | (15 << 12) | (15 << 16);
    let code = [
        attr(false, 0x80, 0, 4),
        attr(false, 0x90, 4, 4),
        attr(false, 0xa0, 8, 1),
        attr(true, 0x70, 0, 4),
        attr(true, 0x80, 4, 4),
        attr(true, 0x68, 8, 1),
        0xe30000000000000f | PT,
    ];
    let mut bytes: Vec<_> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            bytes.extend(word.to_le_bytes());
        }
    }
    let vs = h.program(bytes);
    let (mut e, _) = h.engine("viewport-array", [0.; 4]);
    h.methods(&mut e, &[(0x2044, vs as u32)]);
    let mut vertices = Vec::new();
    for index in 0..16u32 {
        let x = (index % 4) * 8;
        let y = (index / 4) * 8;
        let base = 0xa00 + index * 32;
        h.methods(
            &mut e,
            &[
                (base, (if index % 2 == 0 { 4f32 } else { -4f32 }).to_bits()),
                (base + 4, 4f32.to_bits()),
                (base + 8, 1f32.to_bits()),
                (base + 12, ((x + 4) as f32).to_bits()),
                (base + 16, ((y + 4) as f32).to_bits()),
                (base + 20, 0),
                (0xe00 + index * 16, 1),
                (0xe04 + index * 16, x | ((x + 8) << 16)),
                (0xe08 + index * 16, (y + 1) | ((y + 7) << 16)),
            ],
        );
        for mut vertex in quad(-1., -1., 0., 1., 0.5, [1., 0., 0., 1.]) {
            vertex[8] = f32::from_bits(index);
            vertices.push(vertex);
        }
    }
    h.vertices(&mut e, &vertices, &[[0.; 4]]);
    h.draw(&mut e, vertices.len() as u32, false, false);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/viewport-array");
    let report = h.backend.execute(&dir, &mut h.memory).unwrap();
    for (pixel, rgba) in report.images[0].rgba.chunks_exact(4).enumerate() {
        let x = pixel % 32;
        let y = pixel / 32;
        let column = x / 8;
        let inside_x = if column % 2 == 0 {
            x % 8 < 4
        } else {
            x % 8 >= 4
        };
        let expected = if inside_x && (1..7).contains(&(y % 8)) {
            [255, 0, 0, 255]
        } else {
            [0; 4]
        };
        assert_eq!(rgba, expected, "pixel {x},{y}");
    }
    h.methods(
        &mut e,
        &[
            (0xbe0, 4f32.to_bits()),
            (0xbec, 12f32.to_bits()),
            (0xbf0, 4f32.to_bits()),
            (0xef4, 8 | (16 << 16)),
            (0xef8, 8 << 16),
        ],
    );
    h.draw(&mut e, vertices.len() as u32, false, false);
    let report = h.backend.execute(&dir, &mut h.memory).unwrap();
    for y in 0..8 {
        for x in 8..12 {
            let start = (y * 32 + x) * 4;
            assert_eq!(
                &report.images[0].rgba[start..start + 4],
                &[255, 0, 0, 255],
                "updated viewport 15 at {x},{y}"
            );
        }
    }
}

#[test]
fn draw_range_references_keep_first_vertex_count_and_bounds() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("draw-ranges", [0., 0., 0., 1.]);
    let mut vertices = quad(-1., -1., 0., 1., 0.5, [1., 0., 0., 1.]);
    vertices.extend(quad(0., -1., 1., 1., 0.5, [0., 1., 0., 1.]));
    h.vertices(&mut e, &vertices, &[[0.; 4]]);
    h.draw(&mut e, 12, false, false);
    h.methods(&mut e, &[(0x19d0, 60)]);
    h.draw(&mut e, 6, false, false);
    h.methods(&mut e, &[(0xd74, 6)]);
    h.draw(&mut e, 6, false, false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/draw-range-checks");
    let report = h.backend.execute(&root, &mut h.memory).unwrap();
    for (i, pixel) in report.images[0].rgba.chunks_exact(4).enumerate() {
        assert_eq!(pixel, if i % 32 < 16 { &[255, 0, 0, 255] } else { &[0, 255, 0, 255] });
    }
    h.methods(&mut e, &[(0xd74, 0)]);
    h.draw(&mut e, 12, false, false);
    h.methods(&mut e, &[(0xd74, 12), (0x1618, 4)]);
    let invalid = e.method(B197, 0xd78, 1, &mut h.memory).unwrap().unwrap();
    assert!(h.backend.prepare(&invalid, &h.memory).is_err());
    // A declared limit cannot authorize bytes outside the actual mapped backing.
    e.registers.0.insert(0x1f04, e.registers.get(0x1f04) + 48);
    e.registers.0.insert(0xd74, 0);
    let invalid = e.method(B197, 0xd78, 13, &mut h.memory).unwrap().unwrap();
    assert!(h.backend.prepare(&invalid, &h.memory).is_err());
}

#[test]
fn integer_vertex_fetch_preserves_bits_before_conversion() {
    for signed in [false, true] {
        let mut h = Harness::new();
        let mut header = [0u32; 20];
        header[0] = 1 | (1 << 10);
        header[6] = 0x1f;
        header[13] = (15 << 12) | (15 << 16);
        let code = [
            attr(false, 0x80, 0, 4),
            attr(false, 0x90, 4, 1),
            0x5cb8000000000a00 | PT | 4 | (4 << 20) | if signed { 1 << 13 } else { 0 },
            0x010000000000f000 | PT | 8 | (u64::from((if signed { -1f32 / 255. } else { 1f32 / 255. }).to_bits()) << 20),
            0x5c68000000000000 | PT | 4 | (4 << 8) | (8 << 20),
            0x010000000000f000 | PT | 5,
            0x010000000000f000 | PT | 6,
            0x010000000000f000 | PT | 7 | (u64::from(1f32.to_bits()) << 20),
            attr(true, 0x70, 0, 4),
            attr(true, 0x80, 4, 4),
            0xe30000000000000f | PT,
        ];
        let mut binary: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
        for chunk in code.chunks(3) {
            binary.extend(0x001f8000fc0007e0u64.to_le_bytes());
            for word in chunk { binary.extend(word.to_le_bytes()); }
        }
        let vs = h.program(binary);
        let (mut e, _) = h.engine("integer-fetch", [0.; 4]);
        let mut vertices = quad(-1., -1., 1., 1., 0.5, [0.; 4]);
        for vertex in &mut vertices { vertex[4] = f32::from_bits(if signed { 0xffff } else { 1 }); }
        h.vertices(&mut e, &vertices, &[[0.; 4]]);
        h.methods(&mut e, &[(0x2044, vs as u32), (0x1164, (16 << 7) | (0x1b << 21) | ((if signed { 3 } else { 4 }) << 27))]);
        h.draw(&mut e, 6, false, false);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/integer-fetch-{signed}"));
        let report = h.backend.execute(&directory, &mut h.memory).unwrap();
        let expected = [1, 0, 0, 255];
        assert!(report.images[0].rgba.chunks_exact(4).all(|p| p == expected));
    }
}

#[test]
fn rectangle_fill_covers_pixels_outside_triangle_and_alpha_test_discards() {
    let mut h = Harness::new();
    let mut header = [0u32;20];
    header[0] = 1 | 1 << 10;
    header[10] = 1 << 31;
    header[13] = (15 << 12) | (15 << 16);
    let mov = |r: u64, value: f32| 0x010000000000f000 | PT | r | u64::from(value.to_bits()) << 20;
    let add = |d: u64, a: u64, b: u64| 0x5c58000000000000 | PT | d | a << 8 | b << 20;
    let mul = |d: u64, a: u64, b: u64| 0x5c68000000000000 | PT | d | a << 8 | b << 20;
    let code = [attr(false, 0x2fc, 0, 1),
        0x5cb8000000000000 | PT | 0xa00,
        mov(4, -0.5), mov(5, 0.5), mov(6, -2.), mov(7, -1.),
        add(1, 0, 6), mul(1, 0, 1), mul(1, 1, 7), add(1, 1, 4),
        mul(0, 0, 5), add(0, 0, 4), mov(2, 0.5), mov(3, 1.),
        attr(true, 0x70, 0, 4), add(4, 0, 5), mov(5, 1.), mov(6, 0.), mov(7, 1.),
        attr(true, 0x80, 4, 4), 0xe30000000000000f | PT];
    let mut bytes: Vec<_> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    for chunk in code.chunks(3) {
        bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk { bytes.extend(word.to_le_bytes()); }
    }
    let vs = h.program(bytes);
    let (mut e, a) = h.engine("rectangle-fill", [0.,0.,0.,1.]);
    h.methods(&mut e, &[(0x2044, vs as u32), (0x113c, 2)]);
    h.draw(&mut e, 3, false, false);
    for y in 8..24 { for x in 8..24 {
        h.expected.get_mut(&a).unwrap().1[y*32+x] = Some([(((x as f32+0.5-8.)/16.)*255.).round() as u8,255,0,255]);
    }}
    let (mut e, b) = h.engine("alpha-discard", [0.,0.,0.,1.]);
    h.methods(&mut e, &[(0x2044, vs as u32), (0x113c, 2), (0x12ec, 1), (0x1314, 0x201), (0x1310, 0.5f32.to_bits())]);
    h.draw(&mut e, 3, false, false);
    let mut flat = shader(false, false);
    flat[24..28].copy_from_slice(&0x55u32.to_le_bytes());
    let flat = h.program(flat);
    let mut addresses = vec![a,b];
    for last in [0,1] {
        let (mut e, target) = h.engine("rectangle-flat", [0.,0.,0.,1.]);
        h.methods(&mut e, &[(0x2044, vs as u32), (0x2144, flat as u32), (0x113c, 2), (0x1684, last)]);
        h.draw(&mut e, 3, false, false);
        h.rect_expected(target, 8, 8, 24, 24, [if last == 0 {0} else {255},255,0,255]);
        addresses.push(target);
    }
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/rectangle-fill");
    let report = h.backend.execute(&directory, &mut h.memory).unwrap();
    for address in addresses {
        let image = report.images.iter().find(|image| image.address == address).unwrap();
        for (i, (pixel, expected)) in image.rgba.chunks_exact(4).zip(&h.expected[&address].1).enumerate() {
            assert_eq!(pixel, expected.unwrap(), "image {address:#x} pixel {i}");
        }
    }
}

#[test]
fn depth_sampler_comparisons_cover_all_functions_and_cache_keys() {
    let mut h = Harness::new();
    let surface = Surface { width:4, height:4, depth:1, bytes_per_block:4, block_height_log2:0, block_depth_log2:0 };
    let depth = h.alloc(surface.swizzle(&[0.5f32.to_le_bytes();16].concat()).unwrap());
    let mut checked = Vec::new();
    for function in 0..8u32 { for reference in [0.25f32,0.5,0.75] {
        let mut header=[0u32;20];header[0]=2|5<<10;header[18]=15;
        let mov=|r:u64,f:f32|0x010000000000f000|PT|r|u64::from(f.to_bits())<<20;
        let code=[mov(4,0.5),mov(5,0.5),mov(6,reference),0xc038000000000000|PT|4<<8|255<<20|1<<29|15<<31|1<<50,0xe30000000000000f|PT];
        let mut bytes:Vec<_>=header.into_iter().flat_map(u32::to_le_bytes).collect();
        for chunk in code.chunks(3) { bytes.extend(0x001f8000fc0007e0u64.to_le_bytes());for word in chunk {bytes.extend(word.to_le_bytes());} }
        let fs=h.program(bytes);
        let (mut e,a)=h.engine("depth-compare",[1.,0.,1.,1.]);
        let words=[0x2f | 7<<7 | 7<<10 | 7<<13 | 7<<16 | 2<<19 | 3<<22 | 4<<25 | 5<<28,depth as u32,3<<21,0,(1<<23)|3,(1<<31)|3,0,0];
        let tic=h.alloc(words.into_iter().flat_map(u32::to_le_bytes).collect());
        let tsc=h.alloc([2u32|2<<3|2<<6|1<<9|function<<10,1|1<<4|1<<6,0,0,0,0,0,0].into_iter().flat_map(u32::to_le_bytes).collect());
        h.methods(&mut e,&[(0xf10,1),(0x1574,0),(0x1578,tic as u32),(0x157c,0),(0x155c,0),(0x1560,tsc as u32),(0x1564,0),(0x2144,fs as u32)]);
        h.vertices(&mut e,&quad(-1.,-1.,1.,1.,0.5,[1.;4]),&[[0.;4]]);
        h.draw(&mut e,6,false,false);
        let pass=match function {0=>false,1=>reference<0.5,2=>reference==0.5,3=>reference<=0.5,4=>reference>0.5,5=>reference!=0.5,6=>reference>=0.5,7=>true,_=>unreachable!()};
        checked.push((a,if pass {255} else {0}));
    }}
    let directory=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/depth-compare");
    let report=h.backend.execute(&directory,&mut h.memory).unwrap();
    for (address,value) in checked {
        let image=report.images.iter().find(|image|image.address==address).unwrap();
        for pixel in image.rgba.chunks_exact(4) { assert_eq!(pixel,[value;4],"target {address:#x}"); }
    }
}

#[test]
fn multisample_draw_preserves_edge_coverage_depth_and_stencil_samples() {
    let mut h = Harness::new();
    let (mut e, _) = h.engine("single setup", [0., 0., 0., 1.]);
    let directory = std::env::temp_dir().join(format!("soju-msaa-{}", std::process::id()));
    h.backend.execute_memory(&directory, &mut h.memory).unwrap();
    let color = h.alloc(vec![0; 64 * 64 * 4]);
    let depth = h.alloc(vec![0; 64 * 64 * 4]);
    h.methods(
        &mut e,
        &[
            (0x804, color as u32),
            (0x808, 64),
            (0x80c, 64),
            (0x15d0, 2),
            (0x1534, 1),
            (0x11e0, 0xeaa26e26),
            (0xfe0, 0),
            (0xfe4, depth as u32),
            (0xfe8, 0x14),
            (0xfec, 0),
            (0x1538, 1),
            (0x1228, 64),
            (0x122c, 64),
            (0x1230, 1),
            (0xd90, 1f32.to_bits()),
            (0xda0, 0x5a),
            (0x19d0, 63),
            (0x12cc, 1),
            (0x12e8, 1),
            (0x130c, 0x203),
        ],
    );
    h.vertices(
        &mut e,
        &[
            [-0.8, -0.7, 0.5, 1., 1., 1., 1., 1., 0., 0., 0., 0.],
            [0.8, -0.7, 0.5, 1., 1., 1., 1., 1., 0., 0., 0., 0.],
            [-0.8, 0.83, 0.5, 1., 1., 1., 1., 1., 0., 0., 0., 0.],
        ],
        &[[0.; 4]],
    );
    h.draw(&mut e, 3, false, false);
    h.backend.execute_memory(&directory, &mut h.memory).unwrap();
    let surface = Surface {
        width: 64,
        height: 64,
        depth: 1,
        bytes_per_block: 4,
        block_height_log2: 0,
        block_depth_log2: 0,
    };
    let pixels = surface
        .deswizzle(h.memory.read(color, surface.size()).unwrap())
        .unwrap();
    let z = surface
        .deswizzle(h.memory.read(depth, surface.size()).unwrap())
        .unwrap();
    let mut partial = 0;
    let mut covered = 0;
    for y in 0..32 {
        for x in 0..32 {
            let mut coverage = 0;
            for sample in 0..4 {
                let offset = ((y * 2 + sample / 2) * 64 + x * 2 + sample % 2) * 4;
                let p = &pixels[offset..offset + 4];
                assert!(
                    p == [0, 0, 0, 255] || p == [255; 4],
                    "unexpected sample {p:?}"
                );
                let white = p[0] == 255;
                coverage += usize::from(white);
                let packed = u32::from_le_bytes(z[offset..offset + 4].try_into().unwrap());
                assert_eq!(packed & 255, 0x5a, "stencil sample changed");
                let expected = if white { 0x800000 } else { 0xffffff };
                assert!(
                    ((packed >> 8) as i64 - expected).abs() <= 1,
                    "depth sample does not follow coverage"
                );
            }
            if (1..4).contains(&coverage) {
                partial += 1;
            }
            covered += coverage;
        }
    }
    assert!(
        partial > 20,
        "edge samples collapsed into one value: {partial}"
    );
    assert!(
        (1100..1500).contains(&covered),
        "triangle area coverage: {covered}"
    );
    // Re-import the exact guest sample planes into a fresh native backend. A
    // depth-only clear must preserve all previously stored color samples.
    let mut fresh = Backend::default();
    let snapshot = e.method(B197, 0x19d0, 1, &mut h.memory).unwrap().unwrap();
    fresh.prepare(&snapshot, &h.memory).unwrap();
    fresh.execute_memory(&directory, &mut h.memory).unwrap();
    assert_eq!(
        surface
            .deswizzle(h.memory.read(color, surface.size()).unwrap())
            .unwrap(),
        pixels
    );
    let reset = surface
        .deswizzle(h.memory.read(depth, surface.size()).unwrap())
        .unwrap();
    assert!(
        reset
            .chunks_exact(4)
            .all(|p| u32::from_le_bytes(p.try_into().unwrap()) == 0xffffff5a)
    );
    std::fs::remove_dir_all(directory).unwrap();
}
