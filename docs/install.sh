#!/usr/bin/env bash
# Short-URL entry point for the Soju installer (GitHub Pages, fronted by soju.snack-wrap.com).
#   curl -fsSL soju.snack-wrap.com/install.sh | bash
# Runs install.sh from main, which installs the newest release zip after checking its SHA-256.
set -euo pipefail
curl -fsSL "https://raw.githubusercontent.com/BCD1210/soju/main/install.sh" | bash
