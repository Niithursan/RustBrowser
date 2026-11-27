# Linux packages

Two packages install the same files and use the distribution's own WebKitGTK,
so engine security updates come from the distribution:

| Path | Installed as |
|---|---|
| the release build | `/usr/bin/rust-browser` |
| `packaging/linux/rust-browser.desktop` | `/usr/share/applications/rust-browser.desktop` |
| `packaging/windows/rust-browser.svg` | `/usr/share/icons/hicolor/scalable/apps/rust-browser.svg` |

The launcher is the one Settings > System > Make default browser would write
for `/usr/bin/rust-browser`; `linux_default::tests` keeps them equal. When the
installed launcher runs the browser that asks, Make default browser uses it and
writes nothing to the user's folders.

## Ubuntu and Debian (.deb)

Built with [cargo-deb](https://github.com/kornelski/cargo-deb) from the
`[package.metadata.deb]` table in `Cargo.toml`:

```bash
cargo install cargo-deb --version 3.8.0 --locked
cargo build --locked --release
cargo deb --locked --no-build
sudo apt install ./target/debian/rust-browser_*.deb
```

cargo-deb adds the libraries the program links to (WebKitGTK, GTK, libsoup,
libsecret) as dependencies; the X11 keyboard and XCB libraries winit loads at
run time are listed by hand. Those dependencies take their minimum versions from
the system that builds the package: the release's `.deb` is built on Ubuntu 24.04,
so it needs `libc6 (>= 2.39)` and does not install on Debian 12 or Ubuntu 22.04. A
`.deb` built on the system that will run it does not have that limit. Remove with `sudo apt remove rust-browser`.

## Arch Linux (PKGBUILD)

`arch/PKGBUILD` is `rust-browser-git`: it builds the latest `main` from GitHub.

```bash
cd packaging/linux/arch
makepkg -si
```

`makepkg` runs the tests too (`--nocheck` skips them). To build the commit you
have checked out instead, for example a branch under test, run
`sh packaging/linux/arch/build-from-checkout.sh <output folder> -si`.
Remove with `sudo pacman -R rust-browser-git`.

## Checks

CI builds both packages on every full run, installs each in a clean container
(Ubuntu 24.04 and Arch) and runs `check-installed.sh`, which checks the files,
that every library is present, that the launcher is valid and that
`rust-browser --version` reports the version in `Cargo.toml`. The packages are
kept as the run's artifacts (`rust-browser-linux-deb`, `rust-browser-arch-package`).

The release workflow (`.github/workflows/release.yml`) builds both again from an
approved release tag (`check-release-tag.sh`), with the same install checks, and
attaches them to the release as `rust-browser-v<version>-linux-amd64.deb` and
`rust-browser-v<version>-arch-x86_64.pkg.tar.zst`, each with a `.sha256` file.
The Arch package keeps the name `rust-browser-git` inside. v0.23.0 is the first
release to carry them.
