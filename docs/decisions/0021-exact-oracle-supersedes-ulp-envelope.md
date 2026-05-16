# ADR-0021: An exact correctly-rounded oracle supersedes the ULP-tolerance envelope

- **Status**: accepted
- **Date**: 2026-05-16

## Context

The property suite cross-checked arithmetic against an astro-float
binary-float oracle with a `within_ulps(got, want, 1)` tolerance
(`tests/common/mod.rs`). The slack was not inherent to decimal
arithmetic. It existed only to absorb the oracle's own noise: the test
rendered an astro-float `BigFloat` to a fifty-digit string and
re-parsed it, and that round trip perturbs the low digits. IEEE
754-2019 §4.3 admits zero tolerance: an operation returns the
representable value nearest the infinitely precise result, broken per
the rounding-direction attribute, or the implementation is wrong. A
one-ULP envelope passes an implementation that is systematically
biased by half a ULP, which is precisely the failure shape of the
static-alignment-window family closed in ADR-0018, ADR-0019, and
ADR-0020.

A second gap: Kani routes every arithmetic harness through
`_special_only_for_kani` shims (ADR-0016) so CBMC never encodes the
finite path. The rounding *decision* every kernel funnels through, and
the finite rounding *kernel* itself, had no formal coverage at all,
even though every real correctness defect has lived there.

## Decision

**The oracle is exact, and the comparison is bit exact.** A new
`ferrodec-test-support::oracle` module computes the correctly-rounded
result with arbitrary-precision integers. For `add`, `subtract`,
`multiply`, and `fusedMultiplyAdd` the infinitely precise result is a
finite decimal, formed exactly. For `divide` and `squareRoot` the
quotient and root are expanded to `precision + 2` significant digits
with an exact integer remainder, so the round and sticky decision is
exact rather than tolerance bounded; these are *exact* oracles, not
the residual approximation the plan first sketched. IEEE remainder is
exact, and the oracle models the General Decimal Arithmetic
`Division_impossible` condition (NaN with `Invalid_operation` when the
integer quotient exceeds `precision` digits) that `ferrodec` and
decTest both raise.

The rounding decision is transcribed fresh from §4.3.3, independent of
`ferrodec::should_round_up`, and the cohort is selected from the GDA
ideal-exponent rules, so the oracle does not inherit a defect from the
code it audits.

**Results are compared by decoding the bit pattern, never by
formatting.** `Display` and `{:e}` do not preserve a zero's quantum
(every zero prints `0e+0`), which gave false cohort mismatches on zero
and on Etiny-underflow results. `oracle::decode_decimal128` is the
faithful inverse of the BID `pack_finite` layout, and equality is
checked on sign, coefficient, and quantum exponent.

**The oracle is itself pinned against the specification's reference.**
`tests/oracle_soundness.rs` replays every `add` / `subtract` /
`multiply` / `fma` / `divide` / `remaindernear` decTest case at
precision 34 through the oracle and asserts agreement with Mike
Cowlishaw's vectors, with no `ferrodec` arithmetic in the loop. More
than two thousand cases are checked.

**Transcendentals keep a declared faithful-rounding contract.** IEEE
754-2019 §9.2 recommends but does not require correctly-rounded
transcendentals. S5 asserts a faithful-rounding contract (the result
is one of the two representable values adjacent to the true value),
stated explicitly per rounding direction rather than hidden behind a
symmetric envelope.

**Kani gains decision and kernel coverage.** S6 proves
`should_round_up` equals the §4.3.3 table over its entire input
domain. S7 adds a bounded-domain kernel-equivalence proof. Neither
violates the ADR-0016 shim rule: the new harnesses target the
loop-free decision function and a width-bounded kernel, not the
production U256 pipeline.

**Out of scope, by prior decision.** ADR-0005's `half_down` and `05up`
skips remain. They are non-IEEE GDA directives, not a coverage gap,
and no slice in this engagement touches them.

## Consequences

**Wins.** Faithfulness is now a zero-tolerance, cohort-exact,
status-exact statement over the full finite domain and every rounding
direction, validated transitively against the spec author's own
vectors. Removing the envelope immediately surfaced three genuine
`Decimal128` FMA correctness defects that the tolerance test had
masked, all in the effective-subtraction sub-ULP family: a raw pack
without an exponent clamp, a missing `INEXACT` plus a one-ULP
directional error when the product divides evenly (fd-7nf), and a
missing `UNDERFLOW` when the true value is tiny before rounding but
rounds back up to the smallest normal (fd-99f). `add`, `subtract`,
`multiply`, `divide`, `remainder`, and `squareRoot` were proven
correctly rounded over the full domain. The rounding decision is now
proven exhaustively rather than sampled.

**Costs.** A pure-Rust big-integer dev dependency (`num-bigint`) now
sits in `ferrodec-test-support`, which is `publish = false` and
reaches no shipped artifact. The oracle is itself code that can be
wrong, so it ships with hand-verified vectors and the decTest
soundness gate before any property test trusts it; that gate caught
three oracle defects during development (signed-zero sign, cohort
clamp range, before-rounding tininess).

**Drift.** Two reproducers were filed as `ferrodec` bugs and proven
not to be: fd-d66 (`ferrodec` div correctly returns `0E-6176` for
underflow to zero; the test's formatted cohort read was wrong) and
fd-71c (`ferrodec` correctly raises `Division_impossible`; the oracle
was missing it). The lesson is recorded: compare via the bit decoder,
and the oracle must model GDA's defined-undefined cases, not just the
bare mathematics.

## Related

- Plan: `~/.claude/plans/we-re-at-a-really-cozy-rose.md`
- Commits: `e58d9e7` (S1 oracle), `4897b2d` (S6 decision proof),
  `bec3658` (S2 add/sub/mul), `544f11c` (fd-7nf), `9088841` (S3 FMA),
  `117ffb1` (fd-99f), `85cad72` (S4 sqrt/div/rem, decoder,
  Division_impossible)
- Other ADRs: refines ADR-0015 and ADR-0016 (Kani scope); supersedes
  the `within_ulps` approach introduced alongside ADR-0010; does not
  supersede ADR-0018 / ADR-0019 / ADR-0020; preserves ADR-0005.
