#!/usr/bin/env python3
"""Create a user-local code-signing identity once, in a dedicated keychain.

Stable signatures keep macOS privacy grants (Local Network) across rebuilds;
ad-hoc signatures change with every build and make macOS ask again.
The dedicated keychain's password lives only in a 0600 file, so builds can
unlock it without touching the login keychain or asking for a password.
"""
import os
import pathlib
import secrets
import subprocess
import tempfile

NAME = "Soju Local Signing"
KEYCHAIN = pathlib.Path.home() / "Library/Keychains/soju-signing.keychain-db"
PASSWORD_FILE = pathlib.Path.home() / ".config/soju/signing-keychain.pass"


def run(*args, capture=True, **kwargs):
    try:
        return subprocess.run(args, check=True, timeout=30, text=True,
                              capture_output=capture, **kwargs)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"{args[0]} {args[1]} failed (exit {error.returncode})") from None
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"{args[0]} {args[1]} timed out (keychain interaction?)") from None


def password():
    PASSWORD_FILE.parent.mkdir(parents=True, exist_ok=True)
    if not PASSWORD_FILE.exists():
        fd = os.open(PASSWORD_FILE, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as handle:
            handle.write(secrets.token_hex(24))
    PASSWORD_FILE.chmod(0o600)
    return PASSWORD_FILE.read_text().strip()


def search_list():
    out = run("security", "list-keychains", "-d", "user").stdout
    return [line.strip().strip('"') for line in out.splitlines() if line.strip()]


def fingerprint():
    """SHA-1 of the identity, or None. Untrusted is fine: codesign signs by hash."""
    if not KEYCHAIN.exists():
        return None
    out = run("security", "find-identity", "-p", "codesigning", str(KEYCHAIN)).stdout
    for line in out.splitlines():
        if f'"{NAME}"' in line:
            return line.split()[1]
    return None


def unlock():
    run("security", "unlock-keychain", "-p", password(), str(KEYCHAIN))


def setup():
    secret = password()
    if not KEYCHAIN.exists():
        run("security", "create-keychain", "-p", secret, str(KEYCHAIN))
    run("security", "set-keychain-settings", str(KEYCHAIN))  # no auto-lock timeout
    unlock()
    if str(KEYCHAIN) not in search_list():
        run("security", "list-keychains", "-d", "user", "-s", *search_list(), str(KEYCHAIN))
    if fingerprint() is None:
        old = os.umask(0o077)
        try:
            with tempfile.TemporaryDirectory(prefix="soju-signing-") as directory:
                root = pathlib.Path(directory)
                config, key, cert, bundle = (root / n for n in ("c.cnf", "k.pem", "c.pem", "i.p12"))
                config.write_text(f"""[req]
prompt = no
distinguished_name = subject
x509_extensions = extensions
[subject]
CN = {NAME}
[extensions]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
""")
                run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-x509", "-sha256",
                    "-days", "3650", "-config", str(config), "-keyout", str(key), "-out", str(cert))
                transfer = secrets.token_hex(16)
                env = dict(os.environ, SOJU_P12=transfer)
                export = ["openssl", "pkcs12", "-export", "-inkey", str(key), "-in", str(cert),
                          "-name", NAME, "-out", str(bundle), "-passout", "env:SOJU_P12"]
                try:
                    run(*export[:3], "-legacy", *export[3:], env=env)
                except RuntimeError:
                    run(*export, env=env)
                run("security", "import", str(bundle), "-k", str(KEYCHAIN), "-P", transfer,
                    "-T", "/usr/bin/codesign", "-x")
                run("security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
                    "-s", "-k", secret, str(KEYCHAIN))
        finally:
            os.umask(old)
    identity = fingerprint()
    if identity is None:
        raise RuntimeError("Signing identity missing after setup")
    return identity


def sign(path, identifier):
    """Sign with the stable identity; fall back to ad-hoc with a fixed identifier."""
    try:
        identity = setup()
        run("codesign", "--force", "--keychain", str(KEYCHAIN), "--sign", identity,
            "--identifier", identifier, str(path))
        return identity
    except RuntimeError as error:
        print(f"warning: stable signing unavailable ({error}); using ad-hoc signature, "
              "macOS may ask for Local Network access again")
        run("codesign", "--force", "--sign", "-", "--identifier", identifier, str(path))
        return "-"


if __name__ == "__main__":
    try:
        print(f"{NAME}: {setup()} ({KEYCHAIN})")
    except RuntimeError as error:
        raise SystemExit(str(error))
