use super::*;

#[test]
fn rgb565_uses_two_byte_pitch_and_expands_channels_across_gobs() {
    let mut f = Fixture::new();
    let buffer = frame::GraphicBuffer {
        id: 7,
        allocation: f.nv.resolve(ClientId(1), 7).unwrap(),
        width: 33,
        height: 9,
        pitch: 128,
        plane: 512,
        block: 0,
        format: 4,
    };
    // Independent GOB addresses: first pixel, next GOB column, next GOB row.
    for (offset, pixel) in [(512, 0xf800u16), (1024, 0x07e0), (1536, 0x001f)] {
        f.memory
            .write(PIXELS + offset, &pixel.to_le_bytes())
            .unwrap();
    }
    let ctx = Context {
        kernel: &mut f.kernel,
        memory: &mut f.memory,
        object: 0,
        session: 0,
        client: ClientId(1),
        now_ns: 0,
        tls: TLS,
        nv: Some(&f.nv),
    };
    let frame = buffer.pixels(&ctx).unwrap();
    assert_eq!(&frame.pixels[..4], &[255, 0, 0, 255]);
    assert_eq!(&frame.pixels[32 * 4..33 * 4], &[0, 255, 0, 255]);
    assert_eq!(&frame.pixels[8 * 33 * 4..8 * 33 * 4 + 4], &[0, 0, 255, 255]);
    assert_eq!(&frame.pixels[4..8], &[0, 0, 0, 255]);
}
