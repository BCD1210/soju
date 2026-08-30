#!/usr/bin/env bash
# Short-URL entry point for the Soju installer (GitHub Pages, fronted by soju.snack-wrap.com).
#   curl -fsSL soju.snack-wrap.com/install.sh | bash
# Downloads and runs the real installer from the main branch.
set -euo pipefail
curl -fsSL "https://raw.githubusercontent.com/BCD1210/soju/main/install.sh" | bash
