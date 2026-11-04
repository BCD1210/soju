use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=native/host.m");
    println!("cargo:rerun-if-env-changed=SOJU_SDL_PREFIX");
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "macos",
        "desktop requires macOS"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let prefix = env::var("SOJU_SDL_PREFIX").unwrap_or_else(|_| "/opt/homebrew".into());
    let status = Command::new("clang")
        .args([
            "-fobjc-arc",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-c",
            "native/host.m",
            "-o",
        ])
        .arg(out.join("host.o"))
        .arg(format!("-I{prefix}/include"))
        .status()
        .unwrap();
    assert!(status.success(), "native host compilation failed");
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(out.join("libsoju_host.a"))
            .arg(out.join("host.o"))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-search=native={prefix}/lib");
    println!("cargo:rustc-link-lib=static=soju_host");
    println!("cargo:rustc-link-lib=SDL2");
    for framework in ["Foundation", "GameController", "AudioToolbox"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
