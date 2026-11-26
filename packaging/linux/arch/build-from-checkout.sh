#!/bin/sh
# Build the Arch package from this checkout's current commit instead of the
# latest main on GitHub: the same PKGBUILD with its source pointed here. Used
# by CI and for testing a branch. Run as a normal user (makepkg refuses root).
# Usage: build-from-checkout.sh OUTPUT_DIR [makepkg options...]
set -eu

out=${1:?usage: build-from-checkout.sh OUTPUT_DIR [makepkg options...]}
shift
here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
commit=$(git -C "$repo" rev-parse HEAD)

mkdir -p "$out"
sed "s|^source=.*|source=(\"\$_name::git+file://$repo#commit=$commit\")|" \
  "$here/PKGBUILD" >"$out/PKGBUILD"
grep -q "#commit=$commit" "$out/PKGBUILD" || {
  echo "build-from-checkout: the PKGBUILD's source line was not replaced" >&2
  exit 1
}
cd "$out"
makepkg --noconfirm "$@"
