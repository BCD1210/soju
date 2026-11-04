#![cfg(all(feature = "desktop", target_os = "macos", target_arch = "aarch64"))]
use soju_switch_core::{
    host_audio::Audio,
    ipc::router::Hub,
    nro::{BASE, Nro},
    runtime::{Runtime, Stop},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn imm(code: &mut Vec<u32>, reg: u32, value: u64) {
    code.push(0xd2800000 | ((value as u32 & 65535) << 5) | reg);
    for i in 1..4 {
        let v = ((value >> (16 * i)) & 65535) as u32;
        if v != 0 {
            code.push(0xf2800000 | (i << 21) | (v << 5) | reg);
        }
    }
}
fn request(id: u32, data: &[u8], send: Option<(u64, u32)>, receive: Option<(u64, u32)>) -> Vec<u8> {
    let mut msg = vec![0u8; 256];
    word(
        &mut msg,
        0,
        4 | if send.is_some() { 1 << 20 } else { 0 } | if receive.is_some() { 1 << 24 } else { 0 },
    );
    let mut offset = 8;
    for (addr, size) in [send, receive].into_iter().flatten() {
        word(&mut msg, offset, size);
        word(&mut msg, offset + 4, addr as u32);
        word(&mut msg, offset + 8, ((addr >> 32) as u32 & 15) << 28);
        offset += 12;
    }
    let raw = offset;
    offset = (offset + 15) & !15;
    word(&mut msg, offset, 0x49434653);
    word(&mut msg, offset + 8, id);
    msg[offset + 16..offset + 16 + data.len()].copy_from_slice(data);
    word(
        &mut msg,
        4,
        ((offset + 16 + data.len() - raw).div_ceil(4)) as u32,
    );
    msg
}
fn send(code: &mut Vec<u32>, message: &[u8], new_handle: bool) {
    for (i, bytes) in message.chunks_exact(8).enumerate() {
        imm(code, 9, u64::from_le_bytes(bytes.try_into().unwrap()));
        code.push(0xf9000000 | ((i as u32) << 10) | (20 << 5) | 9); // str x9,[x20,#i*8]
    }
    code.extend([0xaa1303e0, 0xd4000421]); // mov x0,x19; SendSyncRequest
    code.extend([0xb4000040, 0xd40004c1]); // cbz x0,+8; Break on transport failure
    // CMIF result is at offset 24, with or without one moved handle.
    code.extend([0xb9401a89, 0x34000049, 0xd40004c1]);
    if new_handle {
        code.push(0xb9400e93);
    } // ldr w19,[x20,#12]
}
fn tone_nro() -> Vec<u8> {
    let mut image = vec![0; 0x1b000];
    image[0x10..0x14].copy_from_slice(b"NRO0");
    for (offset, value) in [
        (0, 0x14000020),
        (0x18, 0x1b000),
        (0x24, 0x1000),
        (0x28, 0x1000),
        (0x2c, 0x1000),
        (0x30, 0x2000),
        (0x34, 0x19000),
    ] {
        word(&mut image, offset, value);
    }
    image[0x1000..0x1004].copy_from_slice(b"sm:\0");
    for (i, v) in [0, BASE + 0x3000, 96000, 96000, 0].iter().enumerate() {
        image[0x1200 + i * 8..0x1208 + i * 8].copy_from_slice(&v.to_le_bytes());
    }
    for i in 0..24000 {
        // Half a second, 440 Hz, -18 dBFS, identical stereo channels.
        let sample = ((i as f64 * 440. * std::f64::consts::TAU / 48000.).sin() * 4096.) as i16;
        for ch in 0..2 {
            image[0x3000 + i * 4 + ch * 2..0x3002 + i * 4 + ch * 2]
                .copy_from_slice(&sample.to_le_bytes());
        }
    }
    let mut code = vec![0xd53bd074]; // mrs x20,tpidrro_el0
    imm(&mut code, 1, BASE + 0x1000);
    code.extend([0xd40003e1, 0xaa0103f3]); // ConnectToNamedPort; x19=handle
    send(&mut code, &request(0, &[], None, None), false);
    send(&mut code, &request(1, b"audout:u", None, None), true);
    let format: Vec<_> = [48000u32, 2]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    send(
        &mut code,
        &request(
            1,
            &format,
            Some((BASE + 0x1100, 256)),
            Some((BASE + 0x2000, 256)),
        ),
        true,
    );
    send(
        &mut code,
        &request(3, &42u64.to_le_bytes(), Some((BASE + 0x1200, 40)), None),
        false,
    );
    send(&mut code, &request(1, &[], None, None), false);
    imm(&mut code, 0, 1_000_000_000);
    code.push(0xd4000161); // SleepThread for host playback
    send(&mut code, &request(2, &[], None, None), false);
    code.extend([0xd2800000, 0xd40000e1]);
    assert!(0x80 + code.len() * 4 <= 0x1000);
    for (i, instruction) in code.iter().enumerate() {
        word(&mut image, 0x80 + i * 4, *instruction);
    }
    image
}
#[test]
#[ignore = "requires signed desktop build and an available CoreAudio output device"]
fn synthetic_tone_nro_reaches_audiounit_callback() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dist")
        .join(format!("audio-tone-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let path = root.join("tone.nro");
    std::fs::write(&path, tone_nro()).unwrap();
    let nro = Nro::read(&mut std::fs::File::open(&path).unwrap()).unwrap();
    let mut runtime = Runtime::new(&nro.protected_homebrew().unwrap()).unwrap();
    let mut hub = Hub::new(&path, &root.join("home")).unwrap();
    let mut audio = Audio::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let stop = runtime
            .run(&mut hub, Duration::from_millis(5), 10000)
            .unwrap();
        audio.pump(&mut hub).unwrap();
        if stop == Stop::ProcessExited(0) {
            break;
        }
        assert_eq!(stop, Stop::BudgetExpired);
        assert!(Instant::now() < deadline);
    }
    assert!(
        audio.nonzero_samples() >= 47000,
        "only {} nonzero samples rendered",
        audio.nonzero_samples()
    );
    println!(
        "Synthetic 440 Hz NRO: {} nonzero PCM samples consumed by CoreAudio",
        audio.nonzero_samples()
    );
    hub.finish().unwrap();
    drop(audio);
    drop(hub);
    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
}
