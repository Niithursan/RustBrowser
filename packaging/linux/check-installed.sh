#!/bin/sh
# Check an installed Rust Browser package (.deb or Arch): every file is where
# the launcher expects it, every library the program needs is present, the
# launcher is valid and the program runs. Used by CI in clean containers.
# Usage: check-installed.sh EXPECTED_VERSION
set -eu

expected=${1:?usage: check-installed.sh EXPECTED_VERSION}
fail() {
  echo "check-installed: $*" >&2
  exit 1
}

for file in \
  /usr/bin/rust-browser \
  /usr/share/applications/rust-browser.desktop \
  /usr/share/icons/hicolor/scalable/apps/rust-browser.svg; do
  [ -f "$file" ] || fail "missing $file"
done
[ -x /usr/bin/rust-browser ] || fail "/usr/bin/rust-browser is not executable"

missing=$(ldd /usr/bin/rust-browser | grep 'not found' || true)
[ -z "$missing" ] || fail "libraries not found: $missing"

# Loaded at run time by winit, so ldd does not list them.
for lib in libxkbcommon-x11.so.0 libX11-xcb.so.1; do
  ldconfig -p | grep -q "$lib" || fail "$lib is not installed"
done

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate /usr/share/applications/rust-browser.desktop
fi

version=$(/usr/bin/rust-browser --version)
case "$version" in
  *"$expected"*) echo "check-installed: $version" ;;
  *) fail "--version printed '$version', expected $expected" ;;
esac
