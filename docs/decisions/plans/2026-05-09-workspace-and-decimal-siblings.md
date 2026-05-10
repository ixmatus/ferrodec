# Plan: ferrodec workspace conversion, then ferrodec-decimal32 and ferrodec-decimal64

> **Status**: completed 2026-05-10.
>
> All four phases shipped on the `workspace-conversion` branch (65
> commits). Phase A delivered the single-member workspace + lint /
> MSRV / metadata hoist (commits `d87ae0a` / `6bcc5d0` / `8b7c516`).
> Phase B shipped `ferrodec-decimal32` v1.0 through B19 (commits
> `9e34126` … `7006a24`). Phase C shipped `ferrodec-decimal64` v1.0
> through C19 (commits `c2058d6` … `6e3c4d8`). Phase D extracted
> `ferrodec-ieee` (`cca3bb4` / `6e3fe76`) and `ferrodec-test-support`
> (`35baa08` / `70e6c97`), recorded in ADR-0012 and ADR-0013.
>
> A 6-agent correctness review on 2026-05-10 surfaced 9 bugs and 2
> behavioural divergences; each is fixed under its own commit
> (`3964871` parse, `1df6292` clamp, `4a9d40d` zero+sticky,
> `56be535` pow value-equality, `09e2346` next_up signature,
> `66aad49` FMA, `60855d3` min/max, `dff6bac` Display ADR-0014).
> Low-severity polish in `cd90243` / `69e7053` / `4b30690`.
>
> Final versions: `ferrodec` 1.14.5, `ferrodec-decimal32` 1.2.0,
> `ferrodec-decimal64` 1.2.0, `ferrodec-ieee` 0.1.0,
> `ferrodec-test-support` 0.1.0 (publish = false).

## Context

ferrodec is a published Rust crate (v1.14.3) at `/Users/parnell/Development/ferrodec` implementing IEEE 754-2019 Decimal128: no_std, BID-encoded, 8,622 IBM decTest conformance cases passing, 15 Kani harnesses, 6 fuzz targets, and a property-test suite cross-checked against astro-float. Per `~/Development/plant-flag/PRINCIPLES.md`, the next-priority work is `ferrodec-decimal32` and `ferrodec-decimal64` (IEEE 754-2019 Decimal32 and Decimal64 within ferrodec's verification idiom). The catalog notes both as "largely re-parameterization rather than new design."

The exploration confirmed the framing but with two corrections. First, the multiword machinery (`src/multiword/U256/U384/U512`) is hardcoded to u128 limbs and Decimal128-specific; Decimal32's 7-digit coefficient fits in u32 and Decimal64's 16 digits fit in u64, so multiword does not transfer at all. Second, the math kernels (transcendentals, Extended type, Payne-Hanek table) are tightly coupled to Decimal128's working precision and need re-implementation at smaller widths — but at substantially lower cost than Decimal128's, because the smaller types need much less working precision.

What does transfer cleanly: `Status`, `RoundingMode`, and `IeeeClass` (in `src/status.rs`, ~214 LOC, fully platform-agnostic); the rounding shape in `src/ops/round.rs`; the conformance harness *pattern* in `tests/conformance.rs`; the property-test oracle pattern in `tests/common/mod.rs`; the Kani special-case-only harness pattern in `src/verify/`; the CI gates and the lint discipline.

User decisions from clarifying questions:
- **Workspace shape:** ferrodec stays at the repo root; siblings live as flat directories alongside (tokio/axum/serde precedent at this scale). Dual-purpose `Cargo.toml` with both `[workspace]` and `[package]` sections.
- **Shared-core extraction:** deferred. First sibling copy-pastes ~400 LOC of `Status` / `RoundingMode` / `IeeeClass` from ferrodec; `ferrodec-ieee` extraction lands as a cleanup commit after both siblings ship, informed by three concrete consumers. Honors "stand alone first; resist framework abstraction until 3 concrete uses exist."
- **Sibling order:** Decimal32 first. Smaller surface validates workspace and CI shape end-to-end faster; Decimal64 inherits proven patterns.
- **v1.0 scope:** transcendentals included in each sibling's v1.0 (no 0.x-forever dressed up as 1.0). Decimal32's transcendentals are genuinely cheaper than Decimal128's because working-precision needs are smaller — no U256 Extended, no U512 Payne-Hanek argument-reduction table.

## Open backlog from ferrodec's KNOWN_ISSUES.md

Surfaced explicitly per `~/.claude/CLAUDE.md` rule. Neither blocks this work:

- 99 conformance cases skipped under non-IEEE rounding directives (`half_down`, `05up`). Will-not-fix per ADR-0005. Sibling crates inherit the same posture for `dsXxx` and `ddXxx` analogues.
- Kani DPD round-trip harness times out (CBMC cannot unroll 11 iterations of symbolic `u128 % 1000`). Deferred per ADR-0009; property-test coverage substituted. For Decimal32 (3 declets) and Decimal64 (5 declets), the harness *may* terminate. Worth retrying on the smaller types — possible motivation for an ADR if it succeeds.
- Minor: `docs/decisions/README.md` index lists ADRs through 0009 but 0010 exists on disk. Trivial drift; fix in passing.

## Phase A — Workspace conversion (3 commits, no behavior change to ferrodec)

Each commit verifies clean against full CI before the next lands.

**A1. Convert ferrodec to a single-member workspace.**
- File changes: `Cargo.toml` only. Add at top:
  ```toml
  [workspace]
  members = ["."]
  resolver = "2"
  ```
- Verify: `cargo build`, `cargo test --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo build --target thumbv6m-none-eabi --no-default-features`. `Cargo.lock` diff inspected (resolver = "2" may shake out features if previously implicit "1").
- CHANGELOG `[Unreleased]` Internal entry. ferrodec → 1.14.4.
- ADR-0011 "Workspace structure for sibling decimal crates" documenting the flat layout decision and why `crates/` was rejected at this scale.

**A2. Hoist shared lints into `[workspace.lints]`.**
- File changes: `Cargo.toml` adds `[workspace.lints.rust]` and `[workspace.lints.clippy]` mirroring the current `[lints]` table; existing `[lints]` becomes `lints.workspace = true`.
- Verify: clippy diagnostics byte-identical to A1.

**A3. Hoist MSRV, edition, license, repository into `[workspace.package]`.**
- File changes: `Cargo.toml` adds `[workspace.package]` with `edition = "2021"`, `rust-version = "1.84"`, `license = "MIT OR Apache-2.0"`, `repository = "https://github.com/ixmatus/ferrodec"`. Existing `[package]` consumes via `*.workspace = true`. Keep `name`, `version`, `description`, `keywords`, `categories` per-crate.
- Verify: MSRV CI job and `cargo publish --dry-run` clean.

After A3 the workspace is "hot": adding a sibling is a directory plus a 30-line `Cargo.toml` that inherits from workspace.

## Phase B — ferrodec-decimal32 to v1.0

Each step is one commit unless noted. Per "one concern per commit," refactor commits and behavior commits are kept separate.

**B1. Skeleton.** `ferrodec-decimal32/Cargo.toml` (consuming workspace lints/edition/MSRV; package keywords `["decimal", "decimal32", "ieee754", "no_std", "embedded"]`; categories same as ferrodec); `src/lib.rs` with `#![no_std]` and a single `pub struct Decimal32(u32)`; README stub citing IEEE 754-2019 §3.5 Decimal32 parameters; CHANGELOG with `## [Unreleased]`. Workspace `members` list updated. CI matrix entry added.
- Verify: workspace builds; new crate builds clean on all targets including thumbv6m; `cargo build --no-default-features --target thumbv6m-none-eabi` clean.

**B2. Copy `Status`, `RoundingMode`, `IeeeClass` from ferrodec.** Drop them into `ferrodec-decimal32/src/status.rs` and `ferrodec-decimal32/src/classify_types.rs` verbatim. Add a comment at the top of each file: "Duplicated from ferrodec; extraction to ferrodec-ieee deferred until three concrete consumers exist." This is the principled debt declaration.
- Verify: both crates build; no behavior change to ferrodec.

**B3. BID parameters and packing.** `src/bid.rs` with constants for Decimal32 (PRECISION = 7, E_MAX = 96, E_MIN = -95, BIAS = 101, BIASED_EXP_MAX = 191, T_BITS = 20, COEFFICIENT_LIMIT = 10^7, COEFFICIENT_FIELD_LIMIT = 2^23). `pack_finite`, `pack_infinity`, `pack_quiet_nan`, `pack_signaling_nan`, `unpack_*`. Bit-pattern unit tests against IEEE 754-2019 §3.5.2 worked examples.
- Verify: unit tests pass; `cargo doc` clean.

**B4. Vendor IBM `ds*.decTest` vectors.** Download from speleotrove.com (the canonical source; same as ferrodec used for `dq*`); place under `ferrodec-decimal32/tests/vectors/`. Vendoring note recording source URL, retrieval date, and license. No harness yet.

**B5. Conformance harness.** Copy `tests/conformance.rs` and `tests/common/mod.rs` from ferrodec into `ferrodec-decimal32/tests/`. Rename Decimal128 → Decimal32 throughout the dispatch table. Per-file expected-pass count table starts populating; aggregate pass count reported separately. Same asymmetric-guard discipline as ADR-0010.
- Verify: harness compiles; runs over `dsBase.decTest` if present even before full op coverage (most cases will fail until B6+ land).

**B6. classify and parse/format.** `is_nan`, `is_infinite`, `is_finite`, `ieee_class`, `classify_bits`, `is_canonical`, `to_bits`, `from_bits`, `from_bits_truncated`. `parse_str` (gated behind `fmt`) and `Display` / `LowerExp` / `UpperExp` / engineering format. Verify against `dsBase.decTest`, `dsString.decTest`.

**B7. Add and Subtract.** Coefficient alignment over `u64` (Decimal32's working precision: 7 + max digit alignment ≈ 14 digits, fits comfortably in u64). Round to 7 digits using a precision-agnostic version of `src/ops/round.rs`'s logic. Verify against `dsAdd.decTest`, `dsSubtract.decTest`. NaN/Inf propagation; `INVALID` flag emission.

**B8. Multiply.** `u32 × u32 → u64`. Round to 7 digits. Verify against `dsMultiply.decTest`.

**B9. Divide and Remainder.** Long division at the chosen working precision. Verify against `dsDivide.decTest`, `dsRemainder.decTest`, `dsRemainderNear.decTest`.

**B10. Square root and FMA.** `sqrt` via Newton's method at extended working precision; `fma` via widening multiply then aligned add. Verify against `dsSquareRoot.decTest`, `dsFMA.decTest`.

**B11. Comparison and ordering.** `partial_cmp`, `total_cmp`, `compare_total_magnitude`, `min`, `max`. Verify against `dsCompare.decTest`, `dsCompareTotal.decTest`, `dsCompareTotalMag.decTest`, `dsMaxMag.decTest`.

**B12. Quantize, scaleb, logb, next_up, next_down, integral.** Verify against `dsQuantize.decTest`, `dsScaleB.decTest`, `dsLogB.decTest`, `dsNextPlus.decTest`, `dsNextMinus.decTest`, `dsRounding.decTest`.

**B13. Kani harnesses.** Mirror `ferrodec/src/verify/` patterns: special-case-only paths via `*_special_only_for_kani()` shims, bounded operand selectors over the 10-constant alphabet (NAN, sNaN, ±Inf, ±0, ±1, ±MAX, ±MIN). Same set as ferrodec: addsub, mul, div, sqrt, rem, cmp, classify, fma, canonical, encode, quantum, scaleb, logb, nan_payload. CI `kani` job extended to `cargo kani --package ferrodec-decimal32 --features=transcendentals` after B14.
- Bonus exploration: try the DPD round-trip harness on Decimal32's 3 declets (Kani may now terminate; ADR-0009 timed out at Decimal128's 11). If it terminates, propose superseding ADR-0009 for Decimal32 specifically.

**B14. Transcendentals.** `src/math/` with `consts.rs` (π, e, ln2, ln10 at Decimal32 precision), `extended.rs` (Extended type at ~16-digit working precision via u64 coefficient — no U256 needed), `exp.rs`, `ln.rs`, `cbrt.rs`, `sincos.rs`, `inverse_trig.rs`, `argred.rs` (simpler than Decimal128's Payne-Hanek; argument range is bounded enough that schoolbook 2/π reduction at u128 precision suffices), `hyperbolic.rs`, `pow.rs`. Property tests cross-check against astro-float at ~80-bit precision (well above 7-digit accuracy needs). Feature flags `trig`, `exp-log`, `hyperbolic`, `pow`, `transcendentals` mirror ferrodec.
- Decimal32's narrow exponent range (-101..96) means some transcendental kernels saturate sooner than Decimal128's; document saturation thresholds in kernel rustdoc, same convention as ferrodec's `tanh` saturation rationale (CHANGELOG 1.14.3).

**B15. Fuzz targets.** Copy `fuzz/fuzz_targets/{parse, arith, transcendentals, integral, total_cmp, encode}.rs` into `ferrodec-decimal32/fuzz/`, rename type. Each is a few-LOC change; the assertion shapes (no panic, round-trip identity, algebraic invariants) port directly.

**B16. Optional features.** One commit per feature: `binary-float` (f32/f64 conversions), `ops` (core::ops overloads), `serde`, `num-traits`, `dpd` (BID↔DPD interchange; Decimal32's 1 declet for the trailing significand). Each gates the same way as ferrodec.

**B17. Examples.** `examples/{money, rounding_modes, transcendentals}.rs` — analogues of ferrodec's, recalibrated for 7-digit precision (the money example becomes "small-ledger telemetry, exact cents, ~5-digit totals" rather than ferrodec's "exact accounting at any scale").

**B18. Benches.** `benches/{core_ops, comparison, conversions, transcendentals}.rs` — analogues of ferrodec's. Baseline numbers captured per ADR-0007 methodology.

**B19. README and v1.0 release.** README structured per ferrodec's TOC: What it is / Quick start / Feature surface / What you can call / Accuracy / Supported targets / Verification / Performance / Why no core::ops / Choosing between this and rust_decimal / Internals / MSRV / License / Reading list. CHANGELOG entry with conformance evidence (cases passing) and verification claims. Bump to 1.0.0.
- Workspace-level README at the repo root added in this commit: ~80 lines explaining the family, listing the three crates with one-paragraph each, pointing to per-crate READMEs for detail.

## Phase C — ferrodec-decimal64 to v1.0

Same shape as Phase B, item by item. Differences from B:
- BID parameters: PRECISION = 16, E_MAX = 384, E_MIN = -383, BIAS = 398, BIASED_EXP_MAX = 767, T_BITS = 50, COEFFICIENT_LIMIT = 10^16, COEFFICIENT_FIELD_LIMIT = 2^53.
- Storage `u64`; working precision for arithmetic uses `u128` (no multiword).
- Conformance vectors `dd*.decTest` from speleotrove.
- Transcendental kernels at higher working precision than Decimal32 but still well below Decimal128's (no U256 Extended; ~32-digit working precision via u128 coefficient suffices).
- DPD codec uses 5 declets (Decimal32's 1, Decimal128's 11 — the loop count is the only structural difference per ADR-0009's "boolean equations from §3.5.2" framing).

Estimated cadence per `~/Development/plant-flag/PRINCIPLES.md`: Decimal32 v1.0 in 4-6 weeks (transcendentals included), Decimal64 v1.0 in 4-6 weeks following. The user's own foundation-cluster cadence target is "weeks"; transcendentals stretch this slightly but remain in-bounds.

## Phase D — Post-siblings cleanup (informed by three concrete consumers)

**D1. Extract `ferrodec-ieee`.** Move `Status`, `RoundingMode`, `IeeeClass` (and any other types that turned out to be byte-identical across all three crates) into a new `ferrodec-ieee` workspace member at v0.1.0. ferrodec, ferrodec-decimal32, ferrodec-decimal64 each replace their copy with `pub use ferrodec_ieee::{Status, RoundingMode, IeeeClass};` for backward compatibility. Bump each consumer's patch version (re-export only). ADR-0012 "Extracting ferrodec-ieee after three consumers."
- Verify: every public re-export resolves to the same path; downstream users see no API break.

**D2. Extract `ferrodec-test-support`.** Workspace-internal dev-dep crate at `ferrodec-test-support/` with `publish = false`. Contains the astro-float oracle wrappers, `within_ulps`, `parse` helpers, and any property-test scaffolding that turned out identical across the three `tests/common/mod.rs` files. Each crate's `tests/common/mod.rs` becomes a thin re-export.
- Verify: all three crates' property tests still pass; no published-surface change.

**D3. Decide on conformance-harness consolidation.** With three concrete copies of `tests/conformance.rs`, the actual cross-cutting structure is now visible. If 90%+ is identical (parser, dispatch loop, expectation-table machinery) and only the type-specific dispatch arms differ, factor the shared parts into `ferrodec-test-support` with a small per-crate dispatch module. If divergence is genuine (each precision has meaningfully different op surfaces or expectation tables), leave the copies as-is and document the call. ADR records the decision either way.

D1, D2, D3 are independent commits; each one concern.

## Critical files

To read before starting Phase A:
- `/Users/parnell/Development/ferrodec/Cargo.toml` (target of A1-A3)
- `/Users/parnell/Development/ferrodec/.github/workflows/ci.yml` (extend matrix per crate)
- `/Users/parnell/Development/ferrodec/docs/decisions/template.md` (for ADR-0011)
- `/Users/parnell/Development/ferrodec/docs/decisions/README.md` (update index; fix 0010 drift)

To read before starting Phase B (Decimal32):
- `/Users/parnell/Development/ferrodec/src/status.rs` (B2 source)
- `/Users/parnell/Development/ferrodec/src/classify.rs` (B2 + B6 reference; classification predicates)
- `/Users/parnell/Development/ferrodec/src/bid.rs` (B3 reference; the constants pattern and BID layout doc-comments)
- `/Users/parnell/Development/ferrodec/src/ops/round.rs` (B7 reference; the precision-agnostic rounding)
- `/Users/parnell/Development/ferrodec/src/ops/addsub.rs` (B7 reference; alignment shape, with line 65 noted as the precision-specific friction point)
- `/Users/parnell/Development/ferrodec/tests/conformance.rs` (B5 source)
- `/Users/parnell/Development/ferrodec/tests/common/mod.rs` (B5 source)
- `/Users/parnell/Development/ferrodec/src/verify/addsub.rs` (B13 reference; the special-case-only-for-kani shim pattern)
- `/Users/parnell/Development/ferrodec/fuzz/fuzz_targets/parse.rs` (B15 reference)
- `/Users/parnell/Development/ferrodec/src/math/extended.rs` (B14 reference; how Extended is shaped)

External reference:
- IEEE 754-2019 (Parnell already has it per ferrodec's existence) — §3.5 (decimal interchange), §5 (operations), §7 (exception handling), §9 (recommended operations including transcendentals).
- IBM decTest distribution at speleotrove.com (`ds*.decTest` for B4, `dd*.decTest` for Phase C).

## Verification (per phase, end-to-end)

Phase A (each commit): `cargo build && cargo test --all-features && cargo clippy --all-features --all-targets -- -D warnings && cargo build --target thumbv6m-none-eabi --no-default-features && cargo publish --dry-run` — all green, no behavior change observable to ferrodec downstream.

Phase B (every commit): the matrix above plus `cargo test --package ferrodec-decimal32 --features=<incremental scope>`. Conformance test enforces 0-fail on the cases the implementation already covers, and per-file expected-pass counts that increase monotonically as ops land. Once B14 lands: `cargo kani --package ferrodec-decimal32 --features=transcendentals` runs in CI. Once B15 lands: `cargo +nightly fuzz run <target> -- -max_total_time=300` smoke in CI for at least one target per commit.

Phase B19 (v1.0 release): all conformance cases passing minus the documented `half_down`/`05up` skips (mirror ADR-0005 posture); all Kani harnesses discharged; all fuzz targets clean over a 1-hour pre-release run; `cargo publish --dry-run` for `ferrodec-decimal32` clean; README's verification section quotes the conformance-pass count and the Kani-harness count exactly.

Phase C: same as Phase B, scoped to `ferrodec-decimal64`.

Phase D (each commit): every dependent crate's published behavior unchanged (`cargo semver-checks` if the user uses it, otherwise spot-check public API hashes); workspace test suite passes end-to-end; no version of any published crate goes backwards.

## Out of scope for this plan

- Verus (deferred per ADR-0004; the toolchain blocks haven't moved).
- A `decimal-ieee` community-namespace crate (the Plan agent's argument against was decisive: downstream IEEE decimal authors are vanishingly few; downstream ferrodec users are many).
- A unifying `Decimal` trait across the three precisions (premature framework abstraction; revisit only if a real fourth use appears).
- Performance optimization of the new siblings beyond a clean baseline; perf passes follow ADR-0007/0008's "baseline first, candidates second, revert on neutral measurement" methodology and are tracked in their own ADRs.
- Promoting `tools/gen_argred.py` (Decimal128-specific Payne-Hanek table) to a workspace tool; sibling crates use simpler argument reduction and don't need it.
