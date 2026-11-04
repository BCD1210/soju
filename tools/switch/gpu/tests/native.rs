#![cfg(target_os = "macos")]
#[test]
fn native_cache_corruption_pressure_and_async_saturation() {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/native-cache-checks");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    let source = concat!(
        include_str!("../native/profile.swift"),
        "\n",
        include_str!("../native/protocol.swift"),
        "\n",
        include_str!("../native/worker.swift"),
        "\n",
        include_str!("../native/pipeline_cache.swift"),
        "\n",
        include_str!("../native/memory_cache.swift"),
        "\n",
        include_str!("../native/parallel.swift"),
        "\n",
        include_str!("../native/shared_depth.swift"),
        "\n",
        include_str!("native_checks.swift")
    );
    let swift = root.join("checks.swift");
    std::fs::write(&swift, source).unwrap();
    let binary = root.join("checks");
    let output = std::process::Command::new("xcrun")
        .args(["swiftc", "-O", "-parse-as-library", "-import-objc-header"])
        .arg(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native/protocol.h"))
        .arg(swift)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new(binary)
        .env("SOJU_GPU_CACHE_DIR", root.join("cache"))
        .env("SOJU_GPU_DISK_CACHE_BYTES", "65536")
        .env("SOJU_GPU_ASYNC_PIPELINES", "1")
        .env("SOJU_GPU_MERGE_DEPTH", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
}
