#!/usr/bin/env bash
# Short-URL entry point for the Soju installer (served by GitHub Pages).
#   curl -fsSL bcd1210.github.io/soju/install.sh | bash
# Downloads and runs the real installer from the main branch.
set -euo pipefail
curl -fsSL "https://raw.githubusercontent.com/BCD1210/soju/main/install.sh" | bash
