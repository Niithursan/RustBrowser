# uBlock Origin (built-in ad blocker on Windows)

`uBlock0_1.77.2.chromium.zip` is the official uBlock Origin 1.77.2 release for
Chromium, unchanged, as published at
https://github.com/gorhill/uBlock/releases/tag/1.77.2.

- SHA-256: `636cfa6c640b29ab7251b765225d6c0f580bd9fe830d1319005ea7093cf4303c`
  (the digest GitHub shows for the release file; `src/bundled.rs` tests it).
- License: GNU General Public License v3.0. The full text is `LICENSE.txt` inside the
  ZIP, and the extension's files are its source. uBlock Origin's own source
  repository is https://github.com/gorhill/uBlock.
- Rust Browser embeds this file in `rust-browser.exe` on Windows and unpacks it into
  the profile's `built-in\ublock-origin` folder, where WebView2 runs it like an
  extension added from a folder. Rust Browser's own code stays MIT-licensed; the two
  are shipped side by side, not combined.

## Updating

1. Download `uBlock0_<version>.chromium.zip` from the release page and compare its
   SHA-256 with the digest GitHub shows (`(Get-FileHash <file>).Hash` in PowerShell).
2. Replace the ZIP here, and update `VERSION`, `SHA256`, `SOURCE` and the
   `include_bytes!` path in `src/bundled.rs` and this file.
3. Run `cargo test --lib bundled` and
   `native_built_in_ublock_origin_blocks_and_turns_off`. A profile with the previous
   release unpacks the new one at its next start and adds it again.
