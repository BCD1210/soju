use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return Ok(());
    }
    let out = PathBuf::from(env::var("OUT_DIR")?);
    println!("cargo:rerun-if-changed=native/profile.c");
    if !Command::new("clang")
        .args(["-O2", "-c", "native/profile.c", "-o"])
        .arg(out.join("profile.o"))
        .status()?
        .success()
        || !Command::new("ar")
            .arg("crs")
            .arg(out.join("libsoju_profile.a"))
            .arg(out.join("profile.o"))
            .status()?
            .success()
    {
        return Err("profile compilation failed".into());
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=soju_profile");
    let mut source = String::new();
    for name in [
        "protocol.swift",
        "profile.swift",
        "worker.swift",
        "pipeline_cache.swift",
        "memory_cache.swift",
        "parallel.swift",
        "shared_depth.swift",
    ] {
        let path = format!("native/{name}");
        println!("cargo:rerun-if-changed={path}");
        source.push_str(&fs::read_to_string(path)?);
        source.push('\n');
    }
    println!("cargo:rerun-if-changed=native/protocol.h");
    fs::write(out.join("worker.swift"), source)?;
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => return Err(format!("unsupported Metal worker architecture: {other:?}").into()),
    };
    let output = Command::new("xcrun")
        .args(["swiftc", "-O", "-g", "-emit-library", "-target"])
        .arg(format!("{arch}-apple-macosx15.0"))
        .args(["-import-objc-header", "native/protocol.h"])
        .args(["-framework", "Metal", "-framework", "Foundation"])
        .arg(out.join("worker.swift"))
        .arg("-o")
        .arg(out.join("metal-worker.dylib"))
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Metal worker compilation: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}
