# W4-D1 (reader D1): architecture across the workspace

- `matrix.sh`, `matrix.tsv`, `matrix2.sh`, `matrix2.tsv`: the feature
  matrix (every published crate, every single feature and documented
  pair, `cargo build --target thumbv6m-none-eabi --no-default-features
  --features X` plus host `cargo clippy --lib … -- -D warnings`); the
  narrow selections that fail clippy are `exp-log`, `hyperbolic`,
  `trig-pi` alone and their `trig` pairs, all through
  `ferrodec-transcend` dead code (coordinator spot check: 9 errors on
  the root at `exp-log`).
- `msrv.sh`, `msrv.tsv`: `cargo +1.84 check --locked` per crate, library
  and all targets (libraries pass; the dev graph needs `edition2024`).
- `sizes*.sh`, `sizes*.tsv`: per crate `.text`/`.rodata` on thumbv6m
  with LTO off and single run build times.
- `noalloc/`, `noalloc_feat/`, `noalloc_ctl/`: `#![no_std]` thumbv6m
  binaries with a hand written serializer and no `#[global_allocator]`;
  `noalloc_feat` (features `fmt,serde`, never serializes) fails to link
  with "no global memory allocator found", `noalloc_ctl` (`fmt` only)
  links (D1-1; coordinator rerun identical). Path dependencies are
  relative to the repository; each crate wants its own target directory.
- `seam/`: `DecimalFormat::nan_from(F::ONE)` on the three formats
  (debug: Decimal128 panics at nan_propagate.rs:22; release: `NaN1`
  against the siblings' canonical `NaN`; D1-4).
- `lists.sh`: the per selection test list counts behind the d32 "33
  tests run nowhere" row.
