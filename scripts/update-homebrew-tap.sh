#!/bin/bash
# Point the Homebrew tap (BCD1210/homebrew-soju) at a released version:
#   scripts/update-homebrew-tap.sh [version]        (default: VERSION)
# The formula installs the source of tag v<version>, so the tag must exist on
# GitHub first. Run by scripts/publish-release.sh; safe to run again.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-$(cat "$ROOT/VERSION")}"
REPO="${SOJU_REPO:-BCD1210/soju}"
TAP="${SOJU_TAP_REMOTE:-git@github.com:BCD1210/homebrew-soju.git}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Not a version: $VERSION"; exit 64; }
WORK=$(mktemp -d "${TMPDIR:-/tmp}/soju-tap.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
URL="https://github.com/$REPO/archive/refs/tags/v$VERSION.tar.gz"
curl -fsSL "$URL" -o "$WORK/src.tar.gz" || { echo "Tag v$VERSION is not on GitHub yet: $URL"; exit 1; }
SHA=$(shasum -a 256 "$WORK/src.tar.gz" | awk '{print $1}')
# everything the formula installs must be in the tarball
for f in install.sh scripts docs patches tools third_party app resources VERSION LICENSE NOTICE; do
  tar tzf "$WORK/src.tar.gz" "soju-$VERSION/$f" >/dev/null 2>&1 || { echo "v$VERSION has no $f, which the formula installs"; exit 1; }
done
git clone -q --depth 1 "$TAP" "$WORK/tap"
F="$WORK/tap/Formula/soju.rb"
sed -i '' -E \
  -e "s#(url \"https://github.com/$REPO/archive/refs/tags/v)[0-9.]+(\.tar\.gz\")#\1$VERSION\2#" \
  -e "s#(sha256 \")[0-9a-f]{64}(\")#\1$SHA\2#" \
  -e "s#(version \")[0-9.]+(\")#\1$VERSION\2#" "$F"
grep -q "tags/v$VERSION.tar.gz" "$F" && grep -q "$SHA" "$F" && grep -q "version \"$VERSION\"" "$F" \
  || { echo "Could not rewrite $F"; exit 1; }
ruby -c "$F" >/dev/null
if git -C "$WORK/tap" diff --quiet; then
  echo "Homebrew tap already at $VERSION"
else
  git -C "$WORK/tap" commit -q -am "soju $VERSION"
  git -C "$WORK/tap" push -q origin HEAD
  echo "Homebrew tap -> $VERSION"
fi
