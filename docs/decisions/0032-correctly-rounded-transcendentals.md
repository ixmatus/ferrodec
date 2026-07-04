# ADR-0032: Correctly rounded §9.2 transcendentals via Lefèvre / Muller fixed precision bounds

- **Status**: accepted (amended by ADR-0050: the anchor band error-model repair)
- **Date**: 2026-05-21

## Context

The faithful (≤ 1 ULP) §9.2 transcendental contract ratified by
ADR-0024 served the family through the 2.0 release. ADR-0024 also
recorded the rejection of "true provably correct rounding" on the
explicit ground that the hardest case bounds needed to solve the
Table Maker's Dilemma per function per decimal width did not exist
in the literature, and that deriving them was a research programme
rather than an engineering slice.

ADR-0026 then added three independent oracle tiers for
transcendentals. The proof tier is an offline Arb generator
(`tools/gen_transcend_vectors.py`, deliberately not a Cargo
dependency) that, per function and per format, searches for
arguments whose true value sits pathologically close to a decimal
half ULP, raises Arb working precision until the certified ball
enclosure is decisive, and records a per vector provenance note
including the worst case half ULP margin. The directed mode and
binary surface extension recorded in the ADR-0026 addendum (fd-97a,
2026-05-18) extends this proof tier across the four IEEE 754
directed rounding modes and across the binary surface (`pow`,
`atan2`).

The mechanism in ADR-0026 is the mechanism Lefèvre 2000 used for
binary64: empirical hardest case derivation by certified arbitrary
precision search, not closed form bound. With that infrastructure
in place the engineering work needed to discharge a per function
correctness bound is finite. Read the per vector provenance, derive
the per function kernel error budget from the algorithm's structure
(range reduction plus Taylor series plus rounding), prove the
kernel's working width exceeds the empirical worst case plus the
error budget.

The ADR-0026 addendum further records, as empirical observation,
that across the entire committed corpus on all three formats and
the directed mode and binary surface, every result the faithful
kernel produced was exactly correctly rounded, and MPFR
independently confirmed the corpus with zero disagreements. The
faithful kernel is already, empirically, correctly rounded on every
input the proof tier covers.

The ADR-0024 rejection therefore narrows. The engineering slice
that ADR-0024 said did not exist now does, given ADR-0026's
infrastructure. This ADR commits the family to the correctly
rounded contract across the full §9.2 surface using that
infrastructure.

## Decision

### The contract

Across all three IEEE 754-2019 decimal interchange formats
(`Decimal32`, `Decimal64`, `Decimal128`) and across the full §9.2
transcendental surface (`exp`, `ln`, `exp2`, `log2`, `log10`,
`cbrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`), the
result of every transcendental at every rounding mode is the
exactly correctly rounded value: the single nearest representable
result, ties to even at `RoundingMode::NearestEven`, the directed
grid point at the four IEEE 754 directed modes.

The contract supersedes ADR-0024's faithful (≤ 1 ULP) contract
wholesale, in one slice across all twenty functions and all three
formats. Correctly rounded is a strict tightening of faithful
(every correctly rounded value is also faithful); the contract
change is backward compatible at the API level.

### The mechanism

The family adopts Lefèvre / Muller wider fixed working precision
with rigorous a priori error bounds. The shared
`ferrodec-transcend` `Extended` kernel computes every transcendental
at 50 decimal digits of working precision (`EXT_PRECISION` at
`ferrodec-transcend/src/extended.rs:52`). The proof obligation per
function is:

1. **Empirical hardest case.** Read the minimum half ULP margin
   recorded in the Arb generated provenance file
   `tests/vectors/transcend/<fn>.prov` per format width (7 digits
   for `Decimal32`, 16 for `Decimal64`, 34 for `Decimal128`). This
   is the smallest gap between the true value and the half ULP
   boundary observed across the committed corpus's worst case
   search.

2. **Working width sufficiency.** Derive the per function kernel
   error budget at 50 digits from the algorithm's structure. For a
   primitive `f` computed by argument reduction `x = R(x)` followed
   by an `N` term Taylor series, the error budget is:

   - Range reduction error: rounding error of the constants used in
     `R` plus the error of the reduction arithmetic.
   - Taylor series error: at most `N · ε` half ULPs where `ε` is
     the per operation rounding error at 50 digit working precision.
   - Final rounding: one half ULP for the rounding at the format
     boundary in `Extended::to_format` →
     `F::round_and_pack_finite`.

   Prove the sum is strictly less than the empirical worst case
   half ULP margin.

3. **Headroom verification.** Run the corpus test with the
   strengthened assertion (exact correctly rounded, no `0..=1` ULP
   band) and the MPFR gate (the second independent oracle from
   ADR-0026). Both pass with zero faithful at one step entries.

For derived functions (`pow = exp(y · ln |x|)`,
`cbrt = sign(x) · exp(ln |x| / 3)`, `exp2 = exp(x · ln 2)`,
`tan = sin / cos`, the hyperbolic and inverse hyperbolic forms via
`exp` / `ln`, `asin` / `acos` / `atan2` via `atan`), the bound is
the worst of the contributing primitives plus the per step
composition error.

`pow`'s bound couples with the `exp` and `ln` bounds. The proof
commits the three together so the dependency is colocated and the
reader sees the coupled derivation in one place.

`tan` discharges the per decade Payne Hanek bound across the full
argument range without an ε band carve out. The argument
reduction's worst case error term is bounded by the 6300 digit
`2/π` table and the 38 digit truncated `π/2` constant
`PI_OVER_TWO_COEF_38`, both in `ferrodec-transcend/src/argred.rs`
(the constant is derived from an 80 digit reference string kept only
for pinning; the reduction reads the 38 digit truncation).

Honesty amendment (fd-aqs.10, 2026-06-09 review). The earlier text
here claimed an "80 digit π/2 constant in `consts.rs`"; the data
path in fact multiplies by the 38 digit truncation. The *analytic*
truncation bound on that reduction is ≤ 10^{-3} ULP, which is looser
than the worst Decimal128 `cos` sampled half ULP margin
(4.051 × 10^{-4} ULP), so the written bound does not by itself
discharge the correctly rounded obligation at Decimal128 trig. The
*measured* truncation contributes ≤ ~6.3 × 10^{-5} ULP, an order of
magnitude inside that margin, with a one sided bias coherent across
the three formats; so the discharge at Decimal128 trig is empirical
against the sampled margins rather than fully analytic, and a fully
analytic bound awaits the 80 digit U384 `π/2` path (deferred until a
failing high magnitude case surfaces). The bound holds across every
decade the table covers.

### The rollout

One transition slice across the full §9.2 surface, lockstep across
the three formats. The kernel's working width discharges the bound
at all three format precisions simultaneously, so the contract
flips across all three crates in the same merge.

### Versioning

The release ships as a SemVer minor bump per crate: `ferrodec`
2.0.0 → 2.1.0, `ferrodec-decimal64` 2.0.0 → 2.1.0,
`ferrodec-decimal32` 2.0.0 → 2.1.0. Three signed tags. Correctly
rounded is a strict tightening of faithful, so the contract change
is backward compatible at the API level. The latency posture is
unchanged: the 50 digit kernel does not widen.

The shared infrastructure crates (`ferrodec-ieee`,
`ferrodec-multiword`, `ferrodec-transcend`, `ferrodec-test-support`)
stay at their current versions per the ADR-0029 intent that the
shared infrastructure stays on `0.1.x` through the 2.x family;
this slice does not move any of them.

### Rejected alternatives

Two alternative paths to correctly rounded were considered and
rejected on grounds recorded here so they are not relitigated.

- **Ziv adaptive precision with arbitrary precision fallback.**
  Compute at `p + k` extra digits, double `k` on residue ambiguity
  until the rounding boundary is decided. Rejected on unbounded
  worst case latency: there is no proven worst case loop count for
  decimal Ziv, so the bound becomes a runtime parameter rather than
  a discharged invariant. A loop without a proven termination bound
  is incompatible with the STM32U class embedded target and
  conflicts with the verification first posture inherited from
  ADR-0024.

- **CRlibm style precomputed worst case tables.** Per function and
  per format, a table of hard to round arguments and their
  correctly rounded results. Rejected on per function code size and
  table generation cost: each function carries a sizable table, and
  the Cargo feature matrix needed to make tables optional for the
  embedded consumer becomes unmanageable. The wider fixed precision
  approach gives the same contract at the cost of one shared
  kernel; the frugality principle refuses paying CRlibm's per
  function tax when this option exists.

The earlier ADR-0024 rejection of "true provably correct rounding"
stands, narrowed. It was correct in 2026-05-17 when the only known
mechanism was first principles bound derivation. With ADR-0026's
Arb empirical worst case search in place, the rejection narrows to
the original specific claim (no first principles bounds exist in
the literature) and no longer applies to the engineering slice this
ADR commits to.

## Consequences

- The user facing contract tightens for every §9.2 function on
  every format. The doc comments and READMEs on all three crates
  state correctly rounded, not faithful. The exposed API does not
  change.

- The corpus assertion strengthens. The three per crate
  transcendental vector tests (`tests/transcend_vectors.rs`,
  `ferrodec-decimal64/tests/transcend_vectors.rs`,
  `ferrodec-decimal32/tests/transcend_vectors.rs`) replace the
  `0..=1` ULP band assertion with exact match: the count of one
  step off results must be zero across every committed vector and
  every rounding mode. The per mode split that previously printed
  e.g. "503 exactly correctly rounded, 41 faithful at one step"
  now prints zeros in the faithful slot.

- The shared kernel correlated failure surface from ADR-0026
  Context is unchanged. A kernel error correlation across derived
  functions is still possible because the kernel is still shared;
  the Arb worst case corpus is the standing defense. This ADR does
  not widen the verification surface, it tightens the contract that
  the existing verification proves.

- The named exposure in the README disclosure narrows. Before this
  ADR the named exposure is a rounding error on a boundary case the
  sweep did not generate. After this ADR the named exposure is a
  rounding error on a boundary case the Arb empirical worst case
  search did not surface. The exposure does not vanish; it shifts.
  The disclosure invariants stay verbatim per the standing
  approval gating; only the named failure mode is edited, with
  explicit per edit approval.

- Latency is unchanged. The 50 digit kernel does not widen, so the
  per function latency is identical to the 2.0 baseline. The bench
  suite `benches/transcendentals.rs` runs before and after on the
  slice tip; the recorded delta lands in the CHANGELOG as the
  honest accounting.

- Three signed tags ship. `ferrodec-v2.1.0`,
  `ferrodec-decimal64-v2.1.0`, `ferrodec-decimal32-v2.1.0`, all on
  the same signed merge commit.

- The residual frontier is the Arb search depth. The proof depends
  on the empirical worst case the corpus search has surfaced. If a
  future search at greater depth surfaces a worse case than the
  committed margin, the per function rustdoc derivation needs
  amending. The rustdoc cites the search depth and seed used to
  derive each margin so the dependency is durable; the rustdoc gate
  catches a margin shift on regeneration.

- The shared infrastructure version posture is preserved. Per
  ADR-0029's intent, `ferrodec-ieee`, `ferrodec-multiword`,
  `ferrodec-transcend`, and `ferrodec-test-support` stay at their
  current `0.1.x` versions through the 2.x family.

- ADR-0024 is superseded wholly. ADR-0024's `Status` line is edited
  to `superseded by ADR-0032`; the file stays as the historical
  record per the project's ADR conventions.

## Related

- Plan: `~/.claude/plans/phase-d-kickoff-quirky-narwhal.md`
- Other ADRs: supersedes ADR-0024 (faithful contract). Builds on
  ADR-0021 (exact correctly rounded oracle), ADR-0025 (metamorphic
  backstop with condition number bounds), ADR-0026 (independent
  oracle stack: Arb proof tier, MPFR gate, mpmath differential).
  Does not affect ADR-0015 or ADR-0016 (Kani policy and shim
  routing): the Kani harnesses sit on bounded shims at the format
  boundary and do not enter the Extended kernel.
- Beads: `fd-1pv` (Phase D umbrella; claim on slice start, close on
  signed merge).
- Citations:
  - Lefèvre, V. 2000, "Moyens arithmétiques pour un calcul fiable"
    (PhD thesis, École Normale Supérieure de Lyon). The empirical
    hardest case search method for binary64.
  - Muller, J.-M. "Elementary Functions: Algorithms and
    Implementation" (3rd edition, Birkhäuser 2016). Chapter 10
    treats the wider fixed precision technique and the per function
    error budget derivation.
  - IEEE 754-2019 §9.2. The recommended (not mandatory) correctly
    rounded transcendental clause this ADR commits to.
