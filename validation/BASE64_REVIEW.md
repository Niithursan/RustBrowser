# Separate dependency review: PR #4

Reviewed after PR #19. PR #4 remains open at
`4cf34efb5e53982cc7e2661e11142cf583ecf6a8`. No dependency file or PR was changed,
rebased, rerun or merged. Closed PRs #2 and #3 were left alone.

Recommendation: defer the update during qualification. Review it separately after
stabilizing the candidate, with an explicit feature policy and new exact-head CI.
It is not a security fix needed by this investigation.

## API and execution paths

The direct dependency is 0.22.1. Production uses only
`base64::engine::general_purpose::STANDARD.encode(ui::HTML)` to form trusted local
toolbar data URLs. The test-only comparison fixture uses the same API.

The [0.23.1 source/release notes](https://github.com/marshallpierce/rust-base64/blob/v0.23.1/RELEASE-NOTES.md)
and [feature manifest](https://github.com/marshallpierce/rust-base64/blob/v0.23.1/Cargo.toml)
show these distinctions:

- `STANDARD` is still a scalar `GeneralPurpose` engine. Updating the crate does not
  by itself switch that call to SIMD.
- The default feature set now includes `simd-unsafe`. Separate `Simd` detects AVX2
  at runtime on x64 and falls back to scalar. Explicit `Avx2`/`Neon` engines have
  different hardware contracts. Do not replace the application engine as part of
  a routine dependency bump.
- `DecodeError::InvalidLastSymbol(usize, u8)` becomes a struct variant with more
  information. Pattern matching is an API break even though this browser does not
  use it. Custom padding and new configuration constants also expand the API.
- The declared MSRV is 1.71.0, below this project's pinned 1.99.1. The 0.23.1 fix
  concerns upstream tests on non-SIMD architectures, not a browser bug fix.

Cargo.lock already includes 0.23.1 for another platform. However, `cargo tree
--locked --target x86_64-pc-windows-msvc -e features -i base64@0.23.1` reports no
matching dependency in the current Windows graph. Removing a duplicate from the
all-platform lockfile is not evidence of a Windows runtime or performance gain.
The new direct version's default feature policy therefore deserves review.

## Limited local probe

A separate offline scratch Cargo project used exact aliases for 0.22.1 and 0.23.1.
For lengths 0 through 4096, deterministic binary inputs produced identical output
from both `STANDARD.encode` calls and the new runtime-detected `Simd` engine;
SIMD decode round-tripped every case. One test passed on this AVX2-capable x64 host.
The browser's Cargo.toml and Cargo.lock were unchanged.

This is API/parity evidence, not an unsafe-code audit, fuzz campaign, benchmark,
non-AVX2 fallback test or ARM64 validation. Disabling default features on a future
direct dependency may retain scalar-only policy, but Cargo feature unification
must also be reviewed across the actual Windows graph. Do not silently change
features or dependencies to resolve this review.
