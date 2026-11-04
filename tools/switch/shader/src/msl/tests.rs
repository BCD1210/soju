use super::*;
fn binary(words: &[u64]) -> Vec<u8> {
    let mut h = [0u32; 20];
    h[0] = 2 | (5 << 10);
    h[18] = 15;
    h.into_iter()
        .flat_map(u32::to_le_bytes)
        .chain(words.iter().flat_map(|w| w.to_le_bytes()))
        .collect()
}
#[test]
fn cbuf_binding_rz_and_predicated_exit() {
    let b = binary(&[
        0,
        0x4c980000000700ff | 2 << 34 | 15 << 39,
        0xe30000000000000f,
        0xe30000000007000f,
    ]);
    let m = crate::translate(&b, &Options::default()).unwrap();
    assert_eq!(m.constant_buffers, [2]);
    assert!(m.source.contains("cb2 [[buffer(2)]]"));
    assert!(m.source.contains("(void)(cb2[0])"));
    assert!(m.source.contains("if (p[0])"));
    assert!(!m.source.contains("\nr[255] ="));
}
#[test]
fn rejects_unimplemented_modes() {
    let b = binary(&[0, 0x5c58000000070000 | 1 << 47, 0xe30000000007000f]);
    assert!(crate::translate(&b, &Options::default())
        .unwrap_err()
        .0
        .contains("modifier cc"));
    let b = binary(&[0, 0xef48000000070000, 0xe30000000007000f]);
    assert!(crate::translate(&b, &Options::default()).is_err());
}
