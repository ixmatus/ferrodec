# ADR-0015: Kani scope policy — what we prove, what we delegate

- **Status**: accepted
- **Date**: 2026-05-10

## Context

ferrodec ships Kani proof harnesses under `src/verify/` (and the
sibling crates' `src/verify/`). The harnesses prove a deliberately
narrow class of claims; the rest of the correctness surface lives in
property tests, vector-driven conformance, and fuzz. Until this ADR,
the scoping rule was implicit — it lived in author memory
(`feedback_kani_strategy.md`) and was visible only to humans who knew
to ask. The 2026-05-10 six-agent correctness review surfaced two
problems that traced back to the missing written policy:

1. `pow_special_pool_total` invoked production `Decimal128::pow` over
   an 11-constant operand pool that included pairs (`(MAX, MAX)`,
   `(2, MAX)`) that drop into rule-8's `ln_extended` →
   `Extended::mul` → `exp_from_extended` pipeline. CBMC cannot
   tractably explore that pipeline; the harness drove the chronic CI
   timeout that the author memo (`feedback_kani_ci_timeout_ok.md`)
   recorded as "expected." Releases shipped on a red-modulo-kani CI
   signal — meaning the proofs were not gating any release.
2. ADR-0010 asserted "the full Kani proof run … well within the
   2-minute budget" — wrong at the time of writing, and the
   author-memory note recording the actual timing behaviour was
   private. Future readers had no way to discover that the budget
   claim no longer held.

Both problems were patched in the 1.15 cycle (ADR-0010 corrected;
`pow_special_only_for_kani` shim added; harness rewritten). This ADR
records the standing policy so future contributors can follow it
without consulting author memory.

## Decision

### What Kani proves

Kani harnesses cover **special-case dispatch** for the operations
that have a non-finite or boundary-value rule table. The harnesses
assert that the rule table is total (no `unreachable!()` panic on any
combination of distinguished inputs) and, where the spec pins a
concrete answer, that the closed-form result matches. Specifically:

- **Arithmetic specials** for `add`, `sub`, `mul`, `div`, `sqrt`,
  `rem`, `fma`: NaN propagation (qNaN and sNaN), infinity-with-zero
  edges, ±0 sign rules, INVALID / DIV_BY_ZERO flag emission.
- **Encoding round-trip and canonicalisation** for the BID layer
  (`encode`, `canonical`, `nan_payload`) and DPD layer
  (`dpd`)  — domain bounded enough to enumerate.
- **Comparison and classification predicates** (`cmp`, `classify`,
  `quantum`) — pure bit-pattern decisions, SMT-tractable.
- **`try_new` range checks** — a thin wrapper over `pack_finite`.
- **`pow`'s IEEE 754-2019 §9.2.1 rule table** (rules 1–7), through
  the `pow_special_only_for_kani` shim.

### Convention: the `_special_only_for_kani` shim

Every Kani-targeted op exposes a `#[cfg(kani)]` entry point named
`<op>_special_only_for_kani`. The shim wraps a private
`<op>_special_cases` helper that:

- Returns `Some((result, status))` when an IEEE-distinguished rule
  fires (NaN propagation, infinity edge, ±0, etc.).
- Returns `None` when the input requires the op's general path
  (finite-finite arithmetic, transcendental kernel, parser).
- Is loop-free and reads only constant-precision tables; CBMC can
  symbolically enumerate it.

Production code (`<op>_kernel` or the public `Decimal128::<op>`
method) calls the same `<op>_special_cases` helper first, then falls
through to the general path. The Kani harness asserts on the shim's
`Option` value — when `None`, the harness implicitly delegates to the
property-test suite.

### What Kani does NOT prove (proptest-delegated)

The following correctness surfaces are out of scope for Kani and
covered by property tests in `tests/property_*.rs` plus the vendored
Cowlishaw GDA `.decTest` conformance vectors:

- **Finite-finite arithmetic** for `add` / `sub` / `mul` / `div` /
  `sqrt` / `rem` / `fma` — alignment, sticky-bit accumulation,
  rounding, cohort selection. The proptest oracles include round-trip
  identities, `astro-float` reference comparison at 220–1000 bits,
  and `i128` integer-domain cross-checks.
- **All of `src/math/`** — transcendentals (`exp`, `ln`, `pow`'s
  rule-8 general path, `sincos`, `inverse_trig`, `hyperbolic`,
  `cbrt`), argument reduction (`argred`), and the `Extended`-
  precision kernel. CBMC cannot tractably explore the Newton /
  Taylor / argument-reduction loops these kernels rely on.
- **`src/convert/`** — `parse`, `format`, integer bridges, f64
  bridges. The proptest suite covers round-trip identities; the
  conformance vectors (`dq*.decTest`) cover parser edge cases.
- **`src/multiword/`** — U256/U384/U512 helpers. Property tests
  cover the boundary cases; the helpers' size makes Kani sluggish.

### Sibling-crate parity

`ferrodec-decimal32` and `ferrodec-decimal64` follow the same shim
convention. The 32- and 64-bit BID coefficients are SMT-tractable,
so the special-case harnesses can match the 128-bit family one-to-one
without the `_special_only_for_kani` shim being strictly necessary —
but the shim is still required for consistency and to factor the
rule table out of the production kernel. Closing the harness-family
gap (currently the sibling `src/verify/` trees miss eight families
from the decimal128 tree) is tracked under Slice C of the 1.15
cycle.

### Verus

The Verus experiment (`verus/EXPERIMENT.md`, ADR-0004) remains paused.
Reactivation is not triggered by this ADR; the named external
triggers in ADR-0004 still apply.

## Consequences

**Wins.**

- Future Kani additions follow a discoverable convention rather than
  oral tradition. New contributors see the `_special_only_for_kani`
  shim and know they're meant to mirror it.
- The CI-timeout class of bug becomes diagnosable: a harness that
  invokes the production op directly (instead of the shim) is the
  shape that drove the chronic timeout, and the policy makes that
  shape recognisably wrong.
- The "what the proofs cover" question has a written, citable answer
  rather than living in author memory.

**Costs.**

- Adding a Kani harness for a new op now requires a coordinated
  refactor: extract the special cases into `<op>_special_cases`,
  wire `<op>_kernel` to call it, add the `_special_only_for_kani`
  entry point. The refactor is mechanical but not free.
- The general-path proptest coverage matters more: when the policy
  says "general path is proptest-delegated", a thin proptest is a
  policy gap, not just a coverage gap. Slice F of the 1.15 cycle
  raises proptest depth specifically because of this dependency.

**Drift.**

- This ADR's claims need to stay accurate. If a future Kani harness
  is added without going through the shim (e.g., for a new op whose
  general path happens to be SMT-tractable), this ADR's policy
  paragraph should be updated rather than the harness being treated
  as an exception.

## Related

- ADR-0004: Verus pause (related — sets the boundary between Kani
  and Verus scope).
- ADR-0010: Testing strategy after the six-agent review (supersedes
  the contradicted 2-minute timing claim in ADR-0010).
- Plan: 1.15 cycle plan at
  `~/.claude/plans/spawn-6-agents-explore-wondrous-hamster.md` (Slice
  B).
- Memory: `feedback_kani_strategy.md` (private — the prior oral
  policy this ADR now writes down).
