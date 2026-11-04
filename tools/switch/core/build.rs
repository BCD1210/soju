use std::{env, path::PathBuf, process::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native/context.c");
    if env::var_os("CARGO_FEATURE_HV").is_some() && env::var("CARGO_CFG_TARGET_OS")? == "macos" {
        let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
        if !Command::new("clang")
            .args([
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "native/context.c",
                "-o",
            ])
            .arg(out.join("context.o"))
            .status()?
            .success()
        {
            return Err("context compilation failed".into());
        }
        if !Command::new("ar")
            .arg("crs")
            .arg(out.join("libsoju_context.a"))
            .arg(out.join("context.o"))
            .status()?
            .success()
        {
            return Err("context archive failed".into());
        }
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static=soju_context");
    }
    println!("cargo:rerun-if-changed=native/host.m");
    println!("cargo:rerun-if-changed=native/presenter.m");
    println!("cargo:rerun-if-changed=native/microphone.m");
    println!("cargo:rerun-if-changed=native/microphone.plist");
    println!("cargo:rerun-if-env-changed=SOJU_SDL_PREFIX");
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return Ok(());
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
        Command::new("clang")
            .args([
                "-fobjc-arc",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "native/microphone.m",
                "-o"
            ])
            .arg(out.join("microphone.o"))
            .status()?
            .success(),
        "microphone compilation failed"
    );
    assert!(
        Command::new("clang")
            .args([
                "-fobjc-arc",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "native/presenter.m",
                "-o"
            ])
            .arg(out.join("presenter.o"))
            .status()?
            .success(),
        "presenter compilation failed"
    );
    let plist =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("native/microphone.plist");
    println!(
        "cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
    assert!(Command::new("ar")
        .arg("crs")
        .arg(out.join("libsoju_host.a"))
        .arg(out.join("host.o"))
        .arg(out.join("microphone.o"))
        .arg(out.join("presenter.o"))
        .status()
        .unwrap()
        .success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-search=native={prefix}/lib");
    println!("cargo:rustc-link-lib=static=soju_host");
    println!("cargo:rustc-link-lib=SDL2");
    for framework in [
        "Foundation",
        "AppKit",
        "GameController",
        "AudioToolbox",
        "AVFoundation",
        "Metal",
        "QuartzCore",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    Ok(())
}
