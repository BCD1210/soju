# Security

## Reporting a problem

Report it privately, not in a public issue:

- GitHub: [Report a vulnerability](https://github.com/BCD1210/soju/security/advisories/new)
- Email: hello@didimlabs.com

Say what you found, which Soju version, and how to reproduce it. Soju has one maintainer, so expect a
reply in days, not hours.

## What is in scope

- The Soju app and the `soju` scripts, including the installer (`install.sh`) and the updater
- How Soju stores sign-ins and keys on your Mac (Keychain entries, files under `~/.battlenet-macos`)
- The download checks Soju does (SHA-256 and signatures for the engine, runtime components and store games)
- The didimlabs.com site and its worker (`site-worker/`)

Out of scope: bugs in Wine, in Apple's Game Porting Toolkit, or in the store clients themselves, unless
Soju's own setup is what makes them exploitable.

## Supported versions

Fixes go into the next release. Older releases are not patched; update to the latest one.
