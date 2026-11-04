#![cfg(all(target_os = "macos", feature = "desktop"))]
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "opens a native window to exercise the asynchronous swapchain"]
fn native_presenter_owns_inflight_frames_across_resize_and_reopen() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = std::env::temp_dir().join(format!("soju-presenter-checks-{}", std::process::id()));
    let prefix = std::env::var("SOJU_SDL_PREFIX").unwrap_or_else(|_| "/opt/homebrew".into());
    let result = Command::new("clang")
        .args(["-fobjc-arc", "-Wall", "-Wextra", "-Werror"])
        .arg(format!("-I{prefix}/include"))
        .arg(format!("-L{prefix}/lib"))
        .arg("-lSDL2")
        .args([
            "-framework",
            "Foundation",
            "-framework",
            "AppKit",
            "-framework",
            "GameController",
            "-framework",
            "AudioToolbox",
            "-framework",
            "Metal",
            "-framework",
            "QuartzCore",
        ])
        .arg(root.join("native/presenter_checks.m"))
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(&output)
        .env("MTL_DEBUG_LAYER", "1")
        .output()
        .unwrap();
    std::fs::remove_file(output).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("96 queued frames"));
}
