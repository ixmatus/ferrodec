# ADR-0009: DPD interchange behind the `dpd` feature

- **Status**: accepted
- **Date**: 2026-05-08

## Context

[ADR-0001](0001-bid-over-dpd.md) chose BID-128 as ferrodec's storage and arithmetic encoding. The decision still holds — embedded targets have no decimal-FP hardware, BID gives a clean `u128` envelope for arithmetic, and the `<50 KB thumbv6m-none-eabi` floor is real. ADR-0001 named exactly one cost: "Interop with libraries that prefer DPD requires byte-pattern conversion. ferrodec doesn't ship that adapter; users wanting cross-format exchange handle it externally."

Two findings made closing that gap concrete:

1. **The upstream decTest archive ships two DPD-encoded files ferrodec was not vendoring**: `dqEncode.decTest` (367 `apply` + 1 `multiply` testcase, all in DPD `#hex` form) and `dqCanonical.decTest` (~370 cases including 13 canonical-form `apply` checks plus copy/logical/canonicality coverage). Both are licensed under the same ICU terms as the 20 vendored `dq*.decTest` files. Without a DPD codec ferrodec could not run them.

2. **Cost is bounded.** IEEE 754-2008 §3.5.2 gives the declet ↔ BCD conversion as pure boolean equations — no lookup tables required. Eleven declets per `Decimal128`, ~30 bit-operations per declet per direction, plus an outer 11-iteration `% 1000` / `/ 1000` loop on the trailing-significand u128.

The deeper alternatives (DPD-as-storage; a parallel `Decimal128Dpd` newtype; full duplicated arithmetic kernels) were re-litigated honestly and rejected. Their wins target consumers ferrodec is not built for (z/Architecture decimal-FP hardware, IBM mainframe pipelines), and their costs hit budgets ferrodec promises to keep (Kani full-suite under 2 minutes, single conformance test surface, embedded code size).

## Decision

Ship `Decimal128::to_dpd_bytes(self) -> [u8; 16]` and `Decimal128::from_dpd_bytes(bytes: [u8; 16]) -> Self` behind a new opt-in cargo feature, `dpd`. Both directions are total. The codec is implemented as IEEE 754-2008 §3.5.2 boolean equations transcribed verbatim from Mike Cowlishaw's *A Summary of Densely Packed Decimal Encoding* — no lookup tables, no algebraic minimization, line-by-line auditable against the spec text.

Storage encoding for arithmetic stays BID. The codec is a byte-level adapter; `Decimal128`'s `Eq` / `Hash` / `PartialOrd` / arithmetic methods are unchanged, and the BID `from_bits` / `to_bits` round-trip is unchanged.

The conformance runner (`tests/conformance.rs`) gains a per-file `Encoding` flag dispatched on filename. `dqEncode.decTest` and `dqCanonical.decTest` decode `#hex` literals via `from_dpd_bytes`; every other file keeps the existing BID `from_bits` interpretation. With the `dpd` feature off, both files are skipped at the file gate and the existing 8 622 / 0 / 99 baseline is unchanged.

## Consequences

**Wins:**

- **Conformance unlocks ~458 upstream cases**. With the `dpd` feature on, the conformance runner climbs from 8 622 to 9 080 passes (zero failures, zero regressions). Breakdown: `dqEncode.decTest` 368 / 0 / 0 (full coverage on first try) and `dqCanonical.decTest` 90 / 0 / 154. The 154 dqCanonical skips are GDA bit-level operations (`copy`, `copyabs`, `copynegate`, `copysign`, `and`, `or`, `xor`, `invert`, `rotate`, `shift`, `nexttoward`, `comparesig`) that ferrodec does not currently dispatch; they're outside the DPD-interchange scope and tracked under the existing skip-categorization story.
- **Closes the gap ADR-0001 named.** ferrodec can now produce and consume the IEEE 754-2019 DPD byte pattern that IBM decNumber, z/Architecture decimal-FP hardware, and IBM POWER all use as their wire format.
- **Three new Kani harnesses** (`declet_decode_total`, `from_dpd_bytes_total`, `dpd_roundtrip_specials`) prove totality and special-value round-trip in 12s aggregate. The codec cannot panic on any 128-bit input.
- **Property test coverage** in `tests/property_dpd.rs` (5 properties) covers canonical round-trip, cohort preservation, totality, and decode→encode→decode idempotence over the proptest sample space.

**Costs:**

- **+7 KB `.text` on `thumbv6m-none-eabi`** when `dpd` is enabled, measured against a `--features=fmt` baseline using a `staticlib` crate-type with LTO + opt-z + strip + panic=abort exercising the public API from a `#[no_mangle] extern "C"` shim. Users who don't enable the feature pay zero (the entire module is `#[cfg(feature = "dpd")] mod dpd;` in `lib.rs`).

  The plan's original budget was 2 KB and stop-loss was 4 KB. The plan's stop-loss said "pause and review" rather than "abandon"; the review outcome was to accept +7 KB because the cost is dominated by 11 iterations of `u128 % 1000` / `u128 / 1000` in `encode_trailing` / `decode_trailing` plus the boolean expansion of `encode_declet` × 11, which is intrinsic to the algorithm shape on no-hw-divide targets.

  An explicit attempt to shrink it (split the inner loop into u128 / u64 / u32 width phases as the value shrinks past 10^18 then 10^9) measured *worse*: +10 286 bytes vs the +7 198 single-loop version, because the divides eliminated weren't paying separately (compiler-builtins `__udivti3` is shared across the binary; ferrodec already pulls it in via `multiword/div.rs` for BID arithmetic) while the multi-phase structure prevented LLVM from sharing the unified declet-encoding boolean expansions across phases. Reverted unstaged per the strict-revert rule for perf candidates. Documented in this ADR rather than committed; recoverable from this paragraph if a future reader wants to revisit on a different toolchain or target.

- **One feature flag, two vendored test files, one new module, one new property test, one new Kani harness file.** No new dependencies.

- **Kani full-suite +12s.** A fourth harness (`dpd_roundtrip_via_try_new`, full canonical-finite round-trip) was drafted, ran for 10+ minutes without termination on a developer laptop, and was dropped per the plan's explicit stop-loss for that exact harness. CBMC could not unroll 11 iterations of symbolic `u128 % 1000` over the boolean expansion of `encode_declet` × 11 in finite time. The round-trip property is covered by `tests/property_dpd.rs` instead. The omission is documented inline in `src/verify/dpd.rs` so a future reader can revisit if Kani's solver improves.

## Related

- Plan: [`plans/2026-05-07-dpd-interchange.md`](plans/2026-05-07-dpd-interchange.md).
- Predecessor: [ADR-0001](0001-bid-over-dpd.md) (the BID storage choice; this ADR closes its named interop gap without superseding the core decision).
- Commits:
  - `b86fca4` (Phase 0 — declet codec primitive),
  - `fa260e0` (Phase 1 — `to_dpd_bytes` / `from_dpd_bytes` surface),
  - `2d9bc3b` (Phase 2 — vendored vectors + conformance runner extension),
  - `1d8a9b6` (Phase 3 — Kani harnesses).
- Vendored vectors: `tests/vectors/dqEncode.decTest`, `tests/vectors/dqCanonical.decTest`.
- Codec module: `src/dpd.rs`. Property tests: `tests/property_dpd.rs`. Kani harnesses: `src/verify/dpd.rs`.
- Release: `1.12.0`.
