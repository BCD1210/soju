#!/usr/bin/env python3
"""Install a user-only development signing identity once, in the login keychain."""
import os
import pathlib
import secrets
import subprocess
import tempfile

NAME = "Soju Local Development"
KEYCHAIN = pathlib.Path.home() / "Library/Keychains/login.keychain-db"


def run(*args, **kwargs):
    try:
        return subprocess.run(args, check=True, timeout=20, **kwargs)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"{args[0]} {args[1]} failed (exit {error.returncode}); no private material retained") from None
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(
            "Keychain operation needs interaction. If macOS shows a login-keychain "
            "dialog, enter your Mac login password and click Allow (or Unlock). "
            "If it asks to modify certificate trust, authenticate that change. "
            "Then rerun scripts/setup-local-signing.py; existing certificates are reused."
        ) from error


def setup():
    identities = run("security", "find-identity", "-v", "-p", "codesigning",
                     str(KEYCHAIN), capture_output=True, text=True).stdout
    if f'"{NAME}"' in identities:
        print(f"Already installed: {NAME}")
        return
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix="soju-local-signing-") as directory:
        root = pathlib.Path(directory)
        cert, key, bundle = (root / name for name in ("cert.pem", "key.pem", "identity.p12"))
        existing = subprocess.run(
            ["security", "find-certificate", "-c", NAME, "-p", str(KEYCHAIN)],
            capture_output=True, text=True, check=False)
        if existing.returncode == 0:
            cert.write_text(existing.stdout)
        elif existing.returncode == 44:
            config = root / "openssl.cnf"
            config.write_text("""[req]
prompt = no
distinguished_name = subject
x509_extensions = extensions
[subject]
CN = Soju Local Development
[extensions]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
""")
            run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
                "-x509", "-sha256", "-days", "3650", "-config", str(config),
                "-keyout", str(key), "-out", str(cert), capture_output=True)
            key.chmod(0o600)
            password = secrets.token_hex(24)
            export_env = dict(os.environ, SOJU_P12_PASSWORD=password)
            run("openssl", "pkcs12", "-export", "-inkey", str(key), "-in", str(cert),
                "-name", NAME, "-out", str(bundle), "-passout", "env:SOJU_P12_PASSWORD", env=export_env, capture_output=True)
            run("security", "import", str(bundle), "-k", str(KEYCHAIN),
                "-P", password, "-T", "/usr/bin/codesign", "-x")
        else:
            raise RuntimeError(f"Cannot inspect login keychain (security exit {existing.returncode})")
        # Omit -d: trust belongs to this user, never the system/admin domain.
        run("security", "add-trusted-cert", "-r", "trustRoot", "-p", "codeSign",
            "-k", str(KEYCHAIN), str(cert))
    identities = run("security", "find-identity", "-v", "-p", "codesigning",
                     str(KEYCHAIN), capture_output=True, text=True).stdout
    if f'"{NAME}"' not in identities:
        raise RuntimeError("Certificate exists but is not a valid signing identity; inspect it in Keychain Access. No duplicate was created.")
    print(f"Installed: {NAME} (login keychain, user code-signing trust only)")


if __name__ == "__main__":
    try:
        setup()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
