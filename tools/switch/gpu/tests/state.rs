use soju_switch_gpu::{engine::*, memory::Memory, texture::*};
#[test]
fn ordered_draws_and_constant_bindings() {
    let mut e = Engine::default();
    let mut m = Memory::default();
    m.map(0x1000, vec![0; 16]).unwrap();
    for (r, v) in [
        (0x2380, 16),
        (0x2384, 0),
        (0x2388, 0x1000),
        (0x238c, 0),
        (0x2390, 123),
        (0x2410, 0x21),
        (0x1618, 4),
    ] {
        e.method(B197, r, v, &mut m).unwrap();
    }
    let s = e.method(B197, 0xd78, 3, &mut m).unwrap().unwrap();
    assert_eq!(s.constants[0][2].unwrap().address, 0x1000);
    assert_eq!(m.read(0x1000, 4).unwrap(), 123u32.to_le_bytes());
    assert!(matches!(
        s.action,
        Action::Draw {
            count: 3,
            instance: 0,
            ..
        }
    ));
    e.method(B197, 0x1614, 0, &mut m).unwrap();
    e.method(B197, 0x1618, 4 | 1 << 26, &mut m).unwrap();
    assert!(matches!(
        e.method(B197, 0x17e0, 6, &mut m).unwrap().unwrap().action,
        Action::Draw {
            indexed: true,
            instance: 1,
            ..
        }
    ));
    assert!(e.method(B197, 0x1618, 4, &mut m).is_err());
    assert!(e.method(B197, 0xdead, 0, &mut m).is_err());
}
#[test]
fn inline_upload_and_dirty_overlap() {
    let mut e = Engine::default();
    let mut m = Memory::default();
    m.map(0x1000, vec![0; 32]).unwrap();
    let old = m.generation(0x1000, 4).unwrap();
    for (r, v) in [
        (0x180, 5),
        (0x184, 1),
        (0x188, 0),
        (0x18c, 0x1000),
        (0x1b0, 1),
        (0x1b4, 0x04030201),
        (0x1b4, 5),
    ] {
        e.method(A140, r, v, &mut m).unwrap();
    }
    assert_eq!(m.read(0x1000, 6).unwrap(), [1, 2, 3, 4, 5, 0]);
    assert!(m.generation(0x1000, 4).unwrap() > old);
    assert_eq!(m.generation(0x1010, 4).unwrap(), old);
    assert!(e.method(A140, 0x1b4, 0, &mut m).is_err());
    assert!(m.read(u64::MAX, 2).is_err());
    assert!(m.map(0x1010, vec![0; 1]).is_err());
}
#[test]
fn pitched_copy_rect() {
    let mut e = Engine::default();
    let mut m = Memory::default();
    m.map(0x1000, (0..64).collect()).unwrap();
    m.map(0x2000, vec![0; 64]).unwrap();
    for (r, v) in [
        (0x200, 0xd5),
        (0x230, 0xd5),
        (0x204, 1),
        (0x234, 1),
        (0x2ac, 3),
        (0x20c, 1),
        (0x23c, 1),
        (0x214, 16),
        (0x244, 16),
        (0x218, 4),
        (0x21c, 4),
        (0x248, 4),
        (0x24c, 4),
        (0x224, 0x2000),
        (0x254, 0x1000),
        (0x8b0, 1),
        (0x8b4, 1),
        (0x8b8, 2),
        (0x8bc, 2),
        (0x8c4, 1),
        (0x8cc, 1),
        (0x8d4, 1),
        (0x8dc, 0),
    ] {
        e.method(TWOD, r, v, &mut m).unwrap();
    }
    assert_eq!(m.read(0x2014, 8).unwrap(), m.read(0x1004, 8).unwrap());
    assert_eq!(m.read(0x2024, 8).unwrap(), m.read(0x1014, 8).unwrap());
}
pub fn tic() -> [u32; 8] {
    [
        8 | (2 << 7)
            | (2 << 10)
            | (2 << 13)
            | (2 << 16)
            | (2 << 19)
            | (3 << 22)
            | (4 << 25)
            | (5 << 28),
        0x1000,
        3 << 21,
        0,
        (1 << 23) | 3,
        (1 << 31) | 3,
        0,
        0,
    ]
}
#[test]
fn tic_tsc_validation_and_cache_invalidation() {
    let w = tic();
    let t = Tic::parse(&w.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    assert_eq!((t.width, t.height, t.address), (4, 4, 0x1000));
    let mut m = Memory::default();
    m.map(0x1000, vec![0; 1024]).unwrap();
    let mut c = TextureCache::default();
    assert_eq!(c.upload(&t, &m).unwrap(), vec![0; 64]);
    c.upload(&t, &m).unwrap();
    m.write(0x1300, &[1]).unwrap();
    c.upload(&t, &m).unwrap();
    let disabled = std::env::var("SOJU_GPU_CPU_CACHE_BYTES").as_deref() == Ok("0");
    assert_eq!((c.uploads, c.hits), if disabled { (3, 0) } else { (1, 2) });
    m.write(0x1000, &[17]).unwrap();
    assert_eq!(c.upload(&t, &m).unwrap()[0], 17);
    assert_eq!(c.uploads, if disabled { 4 } else { 2 });
    let mut other = Memory::default();
    other.map(0x1000, vec![99; 1024]).unwrap();
    assert_eq!(c.upload(&t, &other).unwrap()[0], 99);
    assert!(Tic::parse(&[0; 32]).is_err());
    let mut s = [0u32; 8];
    s[0] = 2 | 2 << 3 | 2 << 6;
    s[1] = 1 | 1 << 4 | 1 << 6;
    assert!(Tsc::parse(&s.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).is_ok());
}

#[test]
fn unimplemented_launches_are_not_silently_stored() {
    let mut e = Engine::default();
    let mut m = Memory::default();
    for offset in [0x1214, 0x1218, 0x17e4, 0x17f0] {
        let error = e.method(B197, offset, 3, &mut m).unwrap_err();
        let expected = if soju_switch_gpu::registers::lookup(B197, offset).is_some() {
            "unsupported B197 effect"
        } else {
            "unknown method"
        };
        assert!(error.0.contains(expected), "{offset:#x}: {error}");
        assert!(!e.registers.0.contains_key(&offset));
    }
}

#[test]
fn host_semaphore_acquires_and_unsupported_release_modes() {
    use soju_switch_gpu::sync::Host;
    let mut host = Host::default();
    let mut memory = Memory::default();
    memory.map(0x1000, 8u32.to_le_bytes().to_vec()).unwrap();
    host.method(0x14, 0x1000).unwrap();
    host.method(0x18, 8).unwrap();
    assert!(host
        .method(0x1c, 1)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    host.method(0x18, 7).unwrap();
    assert!(!host
        .method(0x1c, 1)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    assert!(host
        .method(0x1c, 4)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    assert!(!host
        .method(0x1c, 8)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    host.method(0x18, 9).unwrap();
    assert!(!host
        .method(0x1c, 4)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    assert!(host
        .method(0x1c, 8)
        .unwrap()
        .unwrap()
        .acquired(&memory)
        .unwrap());
    assert!(host.method(0x1c, 2).is_err());
    assert!(host.method(0x1c, (1 << 24) | (1 << 20) | 2).is_err());
    assert!(host.method(0x14, 0x1001).is_err());
    assert_eq!(memory.read(0x1000, 4).unwrap(), 8u32.to_le_bytes());
}

#[test]
fn range_generations_split_restore_and_ignore_empty_writes() {
    let mut memory = Memory::default();
    memory.map(0x1000, vec![0; 128]).unwrap();
    let initial = memory.generation(0x1000, 128).unwrap();
    memory.write(0x1020, &[1; 16]).unwrap();
    let first = memory.generation(0x1020, 16).unwrap();
    memory.write(0x1028, &[2; 16]).unwrap();
    let second = memory.generation(0x1028, 16).unwrap();
    assert!(initial < first && first < second);
    assert_eq!(memory.generation(0x1000, 32).unwrap(), initial);
    assert_eq!(memory.generation(0x1020, 8).unwrap(), first);
    assert_eq!(memory.generation(0x1038, 72).unwrap(), initial);
    assert_eq!(memory.generation(0x1020, 32).unwrap(), second);
    memory.write(0x1020, &[]).unwrap();
    assert_eq!(memory.generation(0x1020, 8).unwrap(), first);
    memory.write(0x1000, &[3; 128]).unwrap();
    assert_eq!(
        memory.generation(0x1000, 1).unwrap(),
        memory.generation(0x107f, 1).unwrap()
    );
}

#[test]
fn transform_feedback_methods_reject_without_mutating_state() {
    let mut engine = Engine::default();
    let mut memory = Memory::default();
    let mut backend = soju_switch_gpu::backend::Backend::default();
    for offset in [
        0x380, 0x384, 0x388, 0x38c, 0x390, 0x3e0, 0x3f0, 0x700, 0x704, 0x708, 0x738, 0x744, 0x2800,
        0x29fc,
    ] {
        for value in [0, 1, u32::MAX] {
            let error = engine.method(B197, offset, value, &mut memory).unwrap_err();
            assert!(error.0.contains("transform feedback"));
            assert!(engine.registers.0.is_empty());
            assert!(engine
                .method_into(B197, offset, value, &mut memory, &mut backend)
                .unwrap_err()
                .0
                .contains("transform feedback"));
        }
    }
}

#[test]
fn initial_stage_binding_groups_allow_explicit_overrides() {
    let mut r = soju_switch_gpu::engine::Registers::default();
    for (offset, group) in [
        (0x2010, 0),
        (0x2050, 0),
        (0x2090, 1),
        (0x20d0, 2),
        (0x2110, 3),
        (0x2150, 4),
    ] {
        assert_eq!(r.get(offset), group);
    }
    r.0.insert(0x2150, 0);
    assert_eq!(r.get(0x2150), 0);
}

#[test]
fn inactive_sampler_compare_function_does_not_reject_color_sampling() {
    for control in [0, 0x26000] {
        for function in 0..8 {
            let words = [control | (function << 10) | 0x92, 0x51, 0, 0, 0, 0, 0, 0];
            let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
            let sampler = soju_switch_gpu::texture::Tsc::parse(&bytes).unwrap();
            assert_eq!(sampler.address_u, 2);
            for unsupported in [1 << 9, 1 << 23] {
                let mut bytes = bytes.clone();
                bytes[..4].copy_from_slice(&(words[0] | unsupported).to_le_bytes());
                assert!(soju_switch_gpu::texture::Tsc::parse(&bytes).is_err());
            }
        }
    }
}

#[test]
fn anisotropic_sampler_decodes_all_documented_ratios() {
    for (encoding, ratio) in [1, 2, 4, 6, 8, 10, 12, 16].into_iter().enumerate() {
        for min in 1..=3 {
            let words = [
                0x92 | ((encoding as u32) << 20),
                0x42 | (min << 4),
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
            let sampler = soju_switch_gpu::texture::Tsc::parse(&bytes).unwrap();
            assert_eq!(sampler.max_anisotropy, if min == 3 { ratio } else { 1 });
        }
    }
}

#[test]
fn array_tic_accounts_for_every_layer_and_rejects_nonarray_depth() {
    let mut words = [8 | (2 << 7) | (2 << 10) | (2 << 13) | (2 << 16), 0x1000, 3 << 21, 0, (5 << 23) | 7, (1 << 31) | (1 << 16) | 7, 0, 0];
    let parse = |w: [u32; 8]| soju_switch_gpu::texture::Tic::parse(&w.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>());
    let tic = parse(words).unwrap();
    assert!(tic.array);
    assert_eq!(tic.layers, 2);
    assert_eq!(tic.surface().size(), 1024);
    words[4] = (1 << 23) | 7;
    assert!(parse(words).is_err());
    words[4] = (5 << 23) | 7;
    words[5] = (1 << 31) | (2048 << 16) | 7;
    assert!(parse(words).is_err());
}
