#![cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
use sha2::{Digest, Sha256};
use soju_switch_core::{
    ipc::{router::Hub, types::Frame},
    nro::Nro,
    runtime::{Runtime, Stop},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn boot(path: &Path, home: &Path) -> (Runtime, Hub) {
    let nro = Nro::read(&mut std::fs::File::open(path).unwrap()).unwrap();
    let mut rt = Runtime::new(&nro.protected_homebrew().unwrap()).unwrap();
    let hub = Hub::new(path, home).unwrap();
    rt.kernel_mut().program_id = hub.program_id();
    (rt, hub)
}
fn frame(rt: &mut Runtime, hub: &mut Hub) -> Frame {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let stop = rt.run(hub, Duration::from_millis(2), 10000).unwrap();
        if let Some(frame) = hub.take_frame() {
            return frame;
        }
        assert_eq!(stop, Stop::BudgetExpired);
        assert!(Instant::now() < deadline, "frame timeout");
    }
}
fn exit(rt: &mut Runtime, hub: &mut Hub) {
    assert!(hub.request_exit(rt.kernel_mut()).unwrap());
    let stop = rt.run(hub, Duration::from_secs(5), 1_000_000).unwrap();
    assert!(
        matches!(stop, Stop::ProcessExited(0) | Stop::AllThreadsExited),
        "{stop:?}"
    );
    hub.finish().unwrap();
}
#[test]
#[ignore = "requires local 2048 NRO and hypervisor entitlement"]
fn three_moves_save_restart_and_exit() {
    let Some(path) = std::env::var_os("SOJU_SWITCH_TEST_NRO") else {
        return;
    };
    let path = Path::new(&path);
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dist")
        .join(format!("acceptance-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    let start = Instant::now();
    let (mut rt, mut hub) = boot(path, &home);
    let first = frame(&mut rt, &mut hub);
    println!(
        "Rust first frame: {:.6}s, {}x{}",
        start.elapsed().as_secs_f64(),
        first.width,
        first.height
    );
    let mut last = first;
    let start = Instant::now();
    for _ in 0..60 {
        last = frame(&mut rt, &mut hub);
    }
    println!(
        "Rust steady: 60 frames in {:.6}s ({:.3} frames/s)",
        start.elapsed().as_secs_f64(),
        60. / start.elapsed().as_secs_f64()
    );
    for key in [
        soju_switch_input::mapping::keys::LEFT,
        soju_switch_input::mapping::keys::DOWN,
        soju_switch_input::mapping::keys::RIGHT,
    ] {
        hub.set_input(soju_switch_input::mapping::from_keyboard(&[key]));
        let before = last.pixels.clone();
        for _ in 0..4 {
            last = frame(&mut rt, &mut hub);
        }
        hub.set_buttons(0);
        for _ in 0..4 {
            last = frame(&mut rt, &mut hub);
        }
        assert_ne!(before, last.pixels, "input {key} did not change board");
    }
    exit(&mut rt, &mut hub);
    drop(hub);
    drop(rt);
    let hash = format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()));
    let snapshot: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join("saves").join(hash).join("sd-state.json")).unwrap(),
    )
    .unwrap();
    assert!(
        !snapshot["files"]["/switch/2048/state"]
            .as_str()
            .unwrap()
            .is_empty(),
        "2048 did not write its save"
    );
    let (mut rt, mut hub) = boot(path, &home);
    let restored = frame(&mut rt, &mut hub);
    assert_eq!(
        restored.pixels, last.pixels,
        "saved board did not restore exactly"
    );
    exit(&mut rt, &mut hub);
    drop(hub);
    drop(rt);
    std::fs::remove_dir_all(home).unwrap();
}
