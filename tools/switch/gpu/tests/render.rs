#![cfg(target_os = "macos")]
use soju_switch_gpu::{
    backend::Backend,
    engine::{Engine, B197},
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
        (2, 2)
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
        (0xd7c, 0, "clip depth"),
        (0x121c, 2, "CT_SELECT"),
        (0x810, 0xcf, "render target requires"),
        (0x2080, 0x21, "unsupported enabled shader stage"),
        (0x1160, 0, "vertex format"),
        (0x2144, 0, "shader extent"),
    ] {
        let mut invalid = original.clone();
        invalid.registers.0.insert(offset, value);
        let error = h.backend.prepare(&invalid, &h.memory).unwrap_err();
        assert!(error.0.contains(message), "{offset:#x}: {error}");
    }
    let mut invalid = original.clone();
    invalid.action = soju_switch_gpu::engine::Action::Draw {
        topology: 4,
        first: 0,
        count: 6,
        indexed: true,
        instance: 0,
    };
    assert!(h
        .backend
        .prepare(&invalid, &h.memory)
        .unwrap_err()
        .0
        .contains("index buffer limit"));
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
    let sem_address = h.alloc(vec![0; 4]);
    let mut host = soju_switch_gpu::sync::Host::default();
    host.method(0x14, sem_address as u32).unwrap();
    host.method(0x18, 42).unwrap();
    let release = host.method(0x1c, (1 << 24) | 2).unwrap().unwrap();
    release.release(first, &mut h.memory).unwrap();
    assert_eq!(h.memory.read(sem_address, 4).unwrap(), 42u32.to_le_bytes());
    assert!(host
        .method(0x1c, 1)
        .unwrap()
        .unwrap()
        .acquired(&h.memory)
        .unwrap());
    let mut syncpoint = soju_switch_gpu::sync::Syncpoint::default();
    syncpoint.complete(second).unwrap();
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
    assert!(report.diagnostics.contains("pipeline_hits=1"));
    eprintln!(
        "persistent depth/stencil and deferred CPU readback: checked=2048; {}",
        report.diagnostics
    );
}
