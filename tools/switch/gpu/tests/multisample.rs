#![cfg(target_os = "macos")]
#[test]
fn native_multisample_transfer_preserves_samples_and_resolves_colors() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = root.join("target/multisample-checks");
    std::fs::create_dir_all(&work).unwrap();
    let source = concat!(
        "import Foundation\nimport Metal\n",
        "struct Failure: Error { let description: String; init(_ s: String) { description = s } }\n",
        "func need(_ value: Bool, _ message: String) throws { if !value { throw Failure(message) } }\n",
        include_str!("../native/multisample.swift"),
        "\n",
        include_str!("multisample_checks.swift")
    );
    std::fs::write(work.join("checks.swift"), source).unwrap();
    let build = std::process::Command::new("xcrun")
        .args(["swiftc", "-O", "-parse-as-library"])
        .arg(work.join("checks.swift"))
        .arg("-o")
        .arg(work.join("checks"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = std::process::Command::new(work.join("checks"))
        .env("MTL_DEBUG_LAYER", "1")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&run.stdout));
}
