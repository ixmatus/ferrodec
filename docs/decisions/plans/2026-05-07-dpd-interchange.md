# ferrodec DPD interchange (1.12.0 candidate)

## Context

ADR-0001 picked BID-128 as ferrodec's storage encoding for `Decimal128`. The decision still holds — the embedded calculator target has no decimal-FP hardware, BID gives a clean `u128` envelope for arithmetic, and the `<50 KB` `thumbv6m-none-eabi` floor is real. But ADR-0001 explicitly named one cost: "Interop with libraries that prefer DPD requires byte-pattern conversion. ferrodec doesn't ship that adapter; users wanting cross-format exchange handle it externally."

Two findings sharpen that gap into a concrete win:

1. **Mike Cowlishaw's upstream test archive ships two decimal128 test files ferrodec does not vendor**: `dqEncode.decTest` (367 `apply` testcases) and `dqCanonical.decTest` (13 `apply` cases plus richer copy-op coverage). Both are DPD-encoded — the file headers describe the DPD layout literally (`110 bits coefficient continuation`, `Total coefficient length 114 bits`), and the test vectors use hex bit patterns like `#A20780000000000000000000000003D0` which round-trip to decimal values. ferrodec's conformance runner has never seen these files; without a DPD codec it cannot. License is ICU, same as the 20 `dq*.decTest` files already in `tests/vectors/`.

2. **Cost is bounded.** IEEE 754-2008 §3.5.2 specifies the declet ↔ BCD conversion as pure boolean equations — no lookup tables required. Eleven declets per `Decimal128` (33 trailing digits + leading digit from the combination field). The full codec lands in ~300 LOC of `no_std`-compatible logic, behind a feature flag, with a property test that goes through `astro-float` for value preservation.

The deeper scope (DPD-as-storage, parallel `Decimal128Dpd` type, full duplicated arithmetic kernels) is *not* on the table here. Its wins target consumers ferrodec is not built for (z/Architecture decimal-FP hardware, IBM mainframe pipelines), and its costs hit budgets ferrodec promises to keep (Kani full-suite under 2 minutes, single conformance test surface, embedded code size). Re-litigated honestly during planning, the BID-arithmetic argument from ADR-0001 still wins.

## Out of scope

- DPD-as-storage encoding. `Decimal128` stays BID-encoded internally.
- A parallel `Decimal128Dpd` newtype. Users who want DPD interchange call `to_dpd_bytes` / `from_dpd_bytes`.
- Decimal32 / Decimal64 DPD support.
- Lookup tables for the declet codec. Boolean equations only. (Re-evaluated only if profiling on `thumbv6m-none-eabi` shows a hot path — out of scope for v1 of the codec.)
- `serde_dpd` analog of `serde_bid`. Adapter-level only; user can wrap if needed.

## Phase 0 — declet codec (≈ 2 hours, one commit)

**File**: `src/dpd.rs` (new). Module is `pub(crate)` for now; the public surface lives behind `Decimal128` methods in Phase 1.

**Surface**:

```rust
/// Encode three BCD digits (each 0..=9, packed as 12 bits `d0 d1 d2`,
/// most-significant first) into a 10-bit declet.
pub(crate) fn encode_declet(bcd_12: u16) -> u16;

/// Decode a 10-bit declet into three BCD digits (12-bit packed). Per
/// IEEE 754-2008 §3.5.2, *every* 10-bit pattern decodes to a valid BCD
/// triple — non-canonical declets (where `a∈{1,2,3}`, `b∈{6,7,e,f}`,
/// `c∈{e,f}`) decode the same as a canonical equivalent. The codec
/// itself never errors.
pub(crate) fn decode_declet(declet_10: u16) -> u16;

/// Number of declets in a decimal128 trailing significand. Always 11.
pub(crate) const DECLET_COUNT: usize = 11;
```

**Implementation**: pure boolean equations from IEEE 754-2008 §3.5.2 Tables 3.4 and 3.5 (or equivalently the Cowlishaw paper "A Summary of Densely Packed Decimal encoding"). No tables. Each function is ~30 bit-ops; both compile to straight-line code on AArch64 / x86-64 and to a small number of `lsl` / `orr` / `and` on Cortex-M0+.

**Tests** (in-module, behind `#[cfg(test)]`):
- Exhaustive round-trip over all 1000 canonical BCD triples (`d0,d1,d2 ∈ 0..=9`): `decode(encode(bcd)) == bcd`.
- Exhaustive `decode` over all 1024 declet patterns: result is always a valid BCD triple (each digit ≤ 9).
- Spot-check 8 known declet → digit mappings from the IEEE 754-2008 tables.
- Non-canonical declet behaviour: confirm the specific patterns called out in `dqCanonical.decTest`'s comment block (`abc` where `a∈{1,2,3}`, `b∈{6,7,e,f}`, `c∈{e,f}`) all decode to the same digit triples as their canonical equivalents.

**Stop-loss**: if the boolean equations from spec text don't pass the exhaustive round-trip on the first try, switch to a 1024-entry decode table (~2 KB) generated at compile time. Faster to land, larger code size; the choice is local to `src/dpd.rs` and doesn't affect the rest of the plan.

## Phase 1 — `Decimal128` surface + property test (≈ 2 hours, one commit)

**Files**:
- `src/decimal.rs` — add the public methods.
- `src/lib.rs` — feature-gate `mod dpd;` and re-export.
- `Cargo.toml` — register `dpd` feature.
- `tests/property_dpd.rs` — new property test.

**Cargo feature**:
```toml
[features]
dpd = []
```

Off by default. `dpd` is independent of `fmt` / `transcendentals` / `binary-float` and adds no new dependency.

**Public methods on `Decimal128`** (gated by `#[cfg(feature = "dpd")]`):

```rust
impl Decimal128 {
    /// Encode this `Decimal128` as 16 bytes in IEEE 754 DPD layout,
    /// big-endian. The same value stored in BID and DPD has different
    /// bytes; arithmetic uses BID, this is interchange only.
    pub fn to_dpd_bytes(self) -> [u8; 16];

    /// Decode 16 bytes in IEEE 754 DPD layout (big-endian) into a
    /// `Decimal128`. Non-canonical DPD inputs (uncanonical declets,
    /// uncanonical leading-digit combination patterns) are accepted
    /// and canonicalized per IEEE 754-2019 §3.5.2; the returned
    /// `Status` is empty in that case (no IEEE flag is raised, since
    /// canonicalization on input is not a numerical operation).
    pub fn from_dpd_bytes(bytes: [u8; 16]) -> Self;
}
```

Note: `from_dpd_bytes` returns `Self` rather than `(Self, Status)`. IEEE 754-2019 specifies that decoding a non-canonical encoding yields the canonical equivalent value with no exception raised. There is no error path — every 128-bit pattern decodes to *some* valid `Decimal128`. Matches the existing `from_bits` (BID) signature.

**Endianness**: big-endian by spec convention (the "interchange" word in IEEE 754 means network byte order). Users on little-endian hosts byte-swap themselves; `to_be_bytes` / `from_be_bytes` on the `u128` is the obvious internal step.

**Implementation sketch**:

`to_dpd_bytes`:
1. Decode self into `(sign, biased_exp, coef_113)` using the existing `bid::classify_bits` + helpers.
2. Map BID's 5-bit type field + leading-digit-of-coefficient into DPD's 5-bit combination field per IEEE 754-2008 Table 3.6 (the leading digit splits across the combination field's `G0..G4` bits).
3. Slice the trailing 33 digits of `coef_113` into 11 BCD triples (high-to-low) using `div_rem` by `1000` eleven times — or by repeated `% 1000`, `/= 1000`. The existing `bid::pow10` table already has `10^k` so the digit extraction is cheap.
4. Encode each triple via `dpd::encode_declet`.
5. Reassemble 128 bits: `sign << 127 | combination << 122 | exp_continuation << 110 | trailing_declets`.

`from_dpd_bytes`:
1. Read the 128-bit pattern (big-endian).
2. Decode the 5-bit combination field into `(class, leading_digit, exp_high_2_bits)` per IEEE 754-2008 Table 3.6. NaN / Inf classifications are the same set as BID; the bit positions differ.
3. Decode 11 declets via `dpd::decode_declet` into 33 digits (BCD).
4. Reassemble the 113-bit binary coefficient: `coef = leading_digit * 10^33 + sum(digit_i * 10^i)`.
5. Pack as BID (`bid::pack_finite` / `bid::pack_quiet_nan` / etc., already-existing functions).

**Property test** (`tests/property_dpd.rs`):
- `bid_dpd_roundtrip`: for any `Decimal128` constructed from a parsed string, `Decimal128::from_dpd_bytes(d.to_dpd_bytes()) == d` exactly (bit-equal).
- `dpd_value_preservation`: parse a decimal string `s` → encode to DPD → decode back → format as string. Result equals the canonical formatting of `s`. Cross-checks against `astro-float` for the value.
- `non_canonical_robust`: synthesize a random 128-bit pattern with deliberately non-canonical declets in the trailing field. Decoding succeeds; re-encoding produces a *different* pattern (the canonical one) but the same numerical value. Property: decode-then-encode is idempotent (a canonicalization step).
- Cohort preservation: `from_dpd_bytes(to_dpd_bytes(d))` preserves the quantum exponent (`q == d.quantum_exponent()`), not just the value.

**Code-size check**: `cargo build --target thumbv6m-none-eabi --no-default-features --features=fmt,dpd --release` and compare the `.text` size against `--features=fmt` alone. Record the delta. Budget: under 2 KB. Stop-loss: if delta exceeds 4 KB, escalate (probably means the BCD-triple extraction loop didn't get unrolled; the codec itself is small enough that the rest is the digit-extraction).

## Phase 2 — vendor encoding vectors + runner extension (≈ 3 hours, one commit)

**Files**:
- `tests/vectors/dqEncode.decTest` — vendored from upstream archive (`https://speleotrove.com/decimal/dectest.zip`, v2.62).
- `tests/vectors/dqCanonical.decTest` — same source.
- `tests/vectors/README.md` — document the two new files and the `dpd` feature requirement.
- `tests/conformance.rs` — extend operand/expected parsing to accept `#hex32` literals (32 hex chars after `#`, representing 128 bits big-endian DPD).

**Runner extension**:

The existing parser already handles bare `#` (null operand sentinel) and decimal-string operands. The new form is `#` followed by 32 hex characters. Hook it into `parse_value` (or whatever the operand parser is named — confirm in implementation) before the decimal-string branch:

```rust
fn parse_dpd_hex(s: &str) -> Option<Decimal128> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 32 { return None; }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    let bytes = parse_hex_be(hex);  // small helper
    Some(Decimal128::from_dpd_bytes(bytes))
}
```

Apply the same shape to the *expected* result parser. For `apply`-op cases the comparison is value-equality at the BID level (existing `to_bits()` comparison works) — once both sides are decoded into `Decimal128`, the existing comparator does the rest.

**Test gating**: the entire `dqEncode` + `dqCanonical` pair is only run when the `dpd` feature is enabled. Use `#[cfg(feature = "dpd")]` on a new sub-runner, or filter the file list at runtime based on `cfg!(feature = "dpd")`. Without the feature, the runner skips both files (no regression in the existing 8 622 / 0 / 99 baseline).

**Conformance count update**: with the feature on, the new pass floor is `8622 + 367 + 13 + N_canonical_apply` (≈ 8 800–9 000 depending on which `dqCanonical` cases route through `apply`). Update `PASS_FLOOR` accordingly under `#[cfg(feature = "dpd")]`. The failure ceiling stays 0.

**Stop-loss**: if `dqCanonical.decTest` includes operations beyond `apply` / `copy*` that ferrodec doesn't currently dispatch (e.g. `compare` of uncanonical operands), count those as `skip` rather than fail. Document the skip categorisation in `KNOWN_ISSUES.md` (a new section) and revisit in a follow-up if the count is meaningful.

## Phase 3 — Kani harness for codec round-trip (≈ 1-2 hours, one commit)

**File**: `src/verify/dpd.rs` (new), gated by `#[cfg(kani)]`.

**Harnesses** (one each):
- `dpd_roundtrip_canonical`: bounded over a constrained `Decimal128` (the existing `cfg(kani)` operand selectors), assert `from_dpd_bytes(to_dpd_bytes(d)) == d` bit-equal.
- `dpd_decode_total`: for any `[u8; 16]`, `from_dpd_bytes` returns a value where `to_bits()` is a valid BID encoding (specifically: classification is one of Finite/Zero/Inf/NaN, and finite results have `coef < 10^34`). Total function — never panics.
- `declet_decode_total`: for any `u16` masked to 10 bits, `decode_declet` returns three digits each `≤ 9`.

**Budget**: full-suite Kani currently ≈ 2 minutes (per memory). Three new harnesses, each bounded — should add under 30 seconds. If aggregate Kani runtime climbs above 3 minutes, narrow the harness bounds.

**Stop-loss**: if `dpd_roundtrip_canonical` doesn't terminate within the existing per-harness budget (probably 60s), drop it and rely on the property test for that invariant. The two `*_total` harnesses are the higher-value ones (they prove totality, which property tests can't).

## Phase 4 — docs + ADR + ship (≈ 1 hour, one commit + release commit)

**New ADR**: `docs/decisions/0009-dpd-interchange.md`.

- Status: accepted.
- Context: cite this plan and ADR-0001's named gap.
- Decision: ship `to_dpd_bytes` / `from_dpd_bytes` behind the `dpd` feature; storage encoding stays BID.
- Consequences:
  - Wins: closes the ADR-0001 gap; unlocks ~380 upstream conformance testcases; gives ferrodec a credible "full IEEE 754 decimal128 interchange" story.
  - Costs: +~2 KB code size (measured) at `--features=dpd`; +1 feature flag; +2 vendored test files; +1 module + 1 property test + 1 Kani harness file.
- Related: this plan, the two new vendored vector files, the new module path.

**ADR-0001 amendment**: add a `Superseded-by-section` line referencing ADR-0009 *for the interchange-cost paragraph specifically*. ADR-0001's core decision (BID for arithmetic / storage) stays accepted — only the "ferrodec doesn't ship that adapter" sentence is now superseded by the new ADR. Use a short `## Update (2026-05-XX)` block at the bottom of ADR-0001 rather than rewriting history.

**README.md**: under the existing feature-flag table, add `dpd` row. Brief paragraph in the IEEE 754 conformance section noting that DPD interchange is supported via opt-in feature, with a one-line code example.

**CHANGELOG.md**: `[1.12.0]` entry (minor bump — additive feature, no breaking change).

**Version bump**: `1.11.0 → 1.12.0` in `Cargo.toml`. Standard release flow per past releases.

## Stop-loss

- **Phase 0 budget**: 2 hours. If boolean equations don't yield exhaustive round-trip on first try, switch to a 1 KB decode table (still no_std, still small).
- **Phase 1 budget**: 2 hours. If code size at `--features=dpd` exceeds 4 KB on `thumbv6m-none-eabi`, pause and review.
- **Phase 2 budget**: 3 hours. If runner extension to handle `#hex32` requires deeper restructuring than expected, land Phases 0 + 1 first as a `1.11.1` patch (codec only, no conformance vectors), and ship Phase 2 as a separate `1.12.0`.
- **Phase 3 budget**: 2 hours. Kani harnesses are nice-to-have; a property-tested codec is shippable without them. Skip Phase 3 if it threatens the release.
- **Total wall-clock**: 1 day, possibly 2.

## Critical files

### New
- `src/dpd.rs` — declet codec.
- `tests/property_dpd.rs` — property tests.
- `tests/vectors/dqEncode.decTest` — vendored upstream vectors.
- `tests/vectors/dqCanonical.decTest` — vendored upstream vectors.
- `src/verify/dpd.rs` — Kani harnesses (Phase 3).
- `docs/decisions/0009-dpd-interchange.md` — ADR.
- `docs/decisions/plans/2026-05-07-dpd-interchange.md` — this plan, archived.

### Modified
- `src/lib.rs` — `#[cfg(feature = "dpd")] mod dpd;`.
- `src/decimal.rs` — `to_dpd_bytes` / `from_dpd_bytes` methods.
- `Cargo.toml` — register `dpd` feature; bump version `1.11.0` → `1.12.0`.
- `tests/conformance.rs` — `#hex32` operand/expected parser, feature-gated sub-runner, updated `PASS_FLOOR`.
- `tests/vectors/README.md` — note the two new files and the `dpd` requirement.
- `docs/decisions/0001-bid-over-dpd.md` — `## Update` block superseding the interchange-cost paragraph.
- `README.md` — feature table + interchange paragraph.
- `CHANGELOG.md` — `[1.12.0]` entry.
- `KNOWN_ISSUES.md` — note any `dqCanonical` cases skipped (if any).

### Reused (no changes expected)
- `src/bid.rs::pack_finite`, `pack_quiet_nan`, `pack_signaling_nan`, `pow10`, `decimal_digit_count` — used by the BID-side reassembly in `from_dpd_bytes`.
- `src/bid.rs` constants (`BIAS`, `PRECISION`, `COEFFICIENT_LIMIT`) — same.
- `tests/conformance.rs` `Context` parser, `dispatch_op` — unchanged; only the value-parser is extended.

## Verification

After each phase's commit:
- `cargo test --features=transcendentals,binary-float,serde,ops,num-traits` — existing test surface still green.
- `cargo test --features=dpd` — new property test passes.
- `cargo test --features=transcendentals,dpd --test conformance` — pass count rises by ≈ 380 with no failures (Phase 2 onward).
- `cargo build --target thumbv6m-none-eabi --no-default-features --features=fmt,dpd` — embedded floor still builds; record `.text` size delta.
- `cargo clippy --all-features --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.

For the final ship:
- All of the above plus `cargo kani --features=dpd` — 3 new harnesses verify (under the 3-minute aggregate budget).
- `cargo bench --bench core_ops` — no regression on existing benches (the codec is feature-gated, so the default build shouldn't change at all).
- README + CHANGELOG reflect 1.12.0.
- ADR-0009 records the decision; ADR-0001 has its `## Update` block.

## What success looks like

**Narrow win**: codec ships behind `dpd` feature, exhaustive round-trip property tests pass, the two new conformance files run clean. Upstream test count rises by ~380. Embedded code-size delta under 2 KB.

**Wide win**: same as narrow plus Kani harnesses prove decode totality (no DPD bit pattern can panic the library), and the runner extension generalizes cleanly enough that future encoding work (e.g. `dsEncode.decTest` for decimal32 if anyone ever wants it) is a small follow-up.

**Null win**: codec works but vendoring the conformance vectors uncovers a non-trivial set of cases ferrodec mishandles (e.g. uncanonical-combination-field handling). Ship Phase 0 + 1 as `1.11.1` (codec only), document the conformance gap as a known issue, file follow-up. The codec itself is still useful for users who only need `to_dpd_bytes` / `from_dpd_bytes` interop.

**Beyond v1**: the codec opens the door to a `serde_dpd` analog of `serde_bid` if any user actually requests it (currently nobody has). It also makes a future "DPD-encoded variant for hardware-decimal-FP targets" plausible without requiring a parallel arithmetic stack — the value-level conversion is already in place, and a thin storage-shim could sit on top. None of that is committed; the codec is the foundation that makes it cheap to revisit *if* a real consumer appears.
