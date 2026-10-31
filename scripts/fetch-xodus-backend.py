#!/usr/bin/env python3
"""Build the pinned Xodus backend (xodus-cli) from source on this Mac.

Xodus is GPL-3.0 and publishes no macOS binaries, so Soju never ships one:
the source is cloned at a fixed commit, the MIT-licensed Nexus/MACxus patches
that add the library, package and progress commands are downloaded from a
fixed commit and checked against SHA-256 values in resources/xodus-backend.json,
and the result is built locally with the pinned Rust toolchain. The patched
source tree stays in build/xodus-src as the corresponding source.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

MANIFEST = Path(__file__).resolve().parents[1] / "resources/xodus-backend.json"
# Homebrew's rustup formula is keg-only, so its bin/ is listed explicitly.
PATH = os.pathsep.join([str(Path.home() / ".cargo/bin"), "/opt/homebrew/opt/rustup/bin", "/opt/homebrew/bin", "/usr/local/bin",
                        "/usr/bin", "/bin", "/usr/sbin", "/sbin"])


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def recipe(manifest):
    """One value for everything that decides the binary: commit, toolchain and patches."""
    keys = ("version", "repository", "commit", "rust", "cargo_lock_sha256", "deployment_target")
    value = {k: manifest[k] for k in keys}
    value["patches"] = [[p["file"], p["sha256"]] for p in manifest["patches"]]
    value["patch_commit"] = manifest["patch_source"]["commit"]
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def environment():
    env = {k: v for k, v in os.environ.items() if not k.startswith(("DYLD_", "WINE", "CX_", "RUSTFLAGS", "CARGO_"))}
    env.update(PATH=PATH, XODUS_LOG="error")
    return env


def run(args, **kwargs):
    kwargs.setdefault("env", environment())
    return subprocess.run(args, check=True, **kwargs)


def works(cli):
    """The binary starts and carries the library command Soju imports from."""
    try:
        for args in (["--version"], ["library", "--help"], ["resolve-package", "--help"]):
            run([str(cli)] + args, capture_output=True, timeout=20)
        return True
    except (OSError, subprocess.SubprocessError):
        return False


def ready(target, manifest):
    try:
        return ((target / ".soju-xodus").read_text().strip() == recipe(manifest)
                and works(target / "bin/xodus-cli"))
    except OSError:
        return False


def ensure_service(target):
    """xodus-service is the same binary as xodus-cli; the file name selects the role
    (as MACxus ships it), so both share one code signature and Keychain grant."""
    cli, service = target / "bin/xodus-cli", target / "bin/xodus-service"
    if service.is_file() and digest(service) == digest(cli):
        return
    temporary = service.with_name(".xodus-service.tmp")
    shutil.copy2(cli, temporary)
    temporary.chmod(0o755)
    os.replace(temporary, service)


def tool(name, formula):
    if shutil.which(name, path=PATH):
        return
    if not shutil.which("brew", path=PATH):
        raise RuntimeError(name + " is required. Install Homebrew from https://brew.sh, then retry.")
    print("Installing " + formula + " with Homebrew (build dependency)…", flush=True)
    run(["brew", "install", formula])
    if not shutil.which(name, path=PATH):
        raise RuntimeError(name + " is still missing after brew install " + formula + ".")


def download(url, dest):
    run(["/usr/bin/curl", "--fail", "--silent", "--show-error", "--location", "--proto", "=https",
         "--proto-redir", "=https", "--retry", "3", "--connect-timeout", "20", "--max-time", "300",
         url, "--output", str(dest)])


def prepare_source(work, manifest):
    """Clone, verify and patch. Nothing outside work/ changes if any check fails."""
    src = work / "xodus-src"
    print("Fetching Xodus source at " + manifest["commit"][:12] + "…", flush=True)
    run(["git", "-c", "advice.detachedHead=false", "clone", "--quiet", "--filter=blob:none",
         manifest["repository"], str(src)])
    run(["git", "-C", str(src), "-c", "advice.detachedHead=false", "checkout", "--quiet", manifest["commit"]])
    head = run(["git", "-C", str(src), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if head != manifest["commit"]:
        raise ValueError("Xodus source is not at the pinned commit; nothing was changed")
    if digest(src / "Cargo.lock") != manifest["cargo_lock_sha256"]:
        raise ValueError("Xodus Cargo.lock does not match the pinned recipe; nothing was changed")
    patches = work / "patches"
    patches.mkdir()
    print("Verifying " + str(len(manifest["patches"])) + " backend patches…", flush=True)
    for item in manifest["patches"]:
        name = item["file"]
        if "/" in name or not name.endswith(".patch"):
            raise ValueError("Invalid patch name in the backend recipe")
        path = patches / name
        download(manifest["patch_source"]["base_url"] + name, path)
        if digest(path) != item["sha256"]:
            raise ValueError("Patch checksum mismatch (" + name + "); nothing was changed")
    for item in manifest["patches"]:
        run(["git", "-C", str(src), "apply", "--whitespace=nowarn", str(patches / item["file"])])
    return src


def build(src, manifest):
    tool("rustup", "rustup")
    tool("protoc", "protobuf")
    toolchains = run(["rustup", "toolchain", "list"], capture_output=True, text=True).stdout
    if not any(line.startswith(manifest["rust"]) for line in toolchains.splitlines()):
        print("Installing Rust " + manifest["rust"] + " (build toolchain)…", flush=True)
        run(["rustup", "toolchain", "install", manifest["rust"], "--profile", "minimal"])
    print("Building the Xodus backend. This takes a few minutes the first time…", flush=True)
    env = environment()
    env["MACOSX_DEPLOYMENT_TARGET"] = manifest["deployment_target"]
    env["CARGO_TARGET_DIR"] = str(src / "target")
    run(["rustup", "run", manifest["rust"], "cargo", "build", "--locked", "--release", "-p", "xodus-cli"],
        cwd=src, env=env)
    return src / "target/release/xodus-cli"


def install(base, src, binary, manifest):
    target = base / "xodus"
    stage = base / ".xodus-stage"
    shutil.rmtree(stage, ignore_errors=True)
    (stage / "bin").mkdir(parents=True)
    cli = stage / "bin/xodus-cli"
    shutil.copy2(binary, cli)
    cli.chmod(0o755)
    run(["codesign", "-f", "-s", "-", str(cli)], capture_output=True)
    ensure_service(stage)
    if not works(cli):
        shutil.rmtree(stage, ignore_errors=True)
        raise RuntimeError("The new Xodus backend does not start; the previous one is unchanged")
    (stage / ".soju-xodus").write_text(recipe(manifest) + "\n")
    # The patched tree is the corresponding source of the installed binary.
    shutil.rmtree(src / "target", ignore_errors=True)
    sources = base / "build/xodus-src"
    sources.parent.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(sources, ignore_errors=True)
    shutil.move(str(src), str(sources))
    backup = base / "xodus.previous"
    shutil.rmtree(backup, ignore_errors=True)
    if target.exists():
        target.rename(backup)
    try:
        stage.rename(target)
    except OSError:
        if backup.exists():
            backup.rename(target)
        raise
    shutil.rmtree(backup, ignore_errors=True)


def fetch(base, manifest):
    target = base / "xodus"
    if ready(target, manifest):
        ensure_service(target)
        print("Xodus backend ready (" + manifest["version"] + ")")
        return target
    base.mkdir(parents=True, exist_ok=True)
    (base / "build").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".xodus-", dir=base / "build") as temp:
        work = Path(temp)
        src = prepare_source(work, manifest)
        binary = build(src, manifest)
        install(base, src, binary, manifest)
    print("Xodus backend installed in " + str(target))
    return target


if __name__ == "__main__":
    try:
        manifest = json.loads(MANIFEST.read_text())
        base = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
        if sys.argv[1:] == ["--check"]:
            sys.exit(0 if ready(base / "xodus", manifest) else 1)
        fetch(base, manifest)
    except (OSError, ValueError, RuntimeError, KeyError, subprocess.SubprocessError) as error:
        print("Xodus backend: " + str(error), file=sys.stderr)
        sys.exit(1)
