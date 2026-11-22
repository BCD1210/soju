#!/usr/bin/env bash
# Shared read-only requirement check. This does not test game compatibility.
# D2R needs OS-side Rosetta fixes, independently of ROSETTA_ADVERTISE_AVX.
soju_d2r_macos_check() {
  local version major minor
  if ! version=$(sw_vers -productVersion 2>/dev/null) ||
     [[ ! "$version" =~ ^([0-9]+)\.([0-9]+)(\.[0-9]+)?$ ]]; then
    echo "D2R: could not determine the macOS version. Check About This Mac; macOS 26.4 or later is required."
    return 2
  fi
  major=$((10#${BASH_REMATCH[1]}))
  minor=$((10#${BASH_REMATCH[2]}))
  if (( major < 26 || (major == 26 && minor < 4) )); then
    echo "D2R requires macOS 26.4 or later for Rosetta fixes; this Mac runs $version. Update macOS in System Settings > General > Software Update, restart, then retry your existing game installation. ROSETTA_ADVERTISE_AVX does not replace these OS fixes."
    return 1
  fi
  echo "D2R macOS requirement met ($version); game compatibility is not verified by this check."
}
