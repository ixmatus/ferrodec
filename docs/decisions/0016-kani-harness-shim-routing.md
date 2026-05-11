# ADR-0016: Kani harnesses route through `_special_only_for_kani` shims, never production ops

- **Status**: accepted
- **Date**: 2026-05-11

## Context

ADR-0015 (Kani scope policy) recorded that ferrodec's special-case
harnesses use `<op>_special_only_for_kani` shims rather than the
production `<op>` entry points. The 2026-05-10 six-agent correctness
review surfaced one concrete violation of the policy (`pow`) and Slice
B closed it. Investigation during Slice C surfaced that the policy
had been *quietly violated* in many more places:

1. **`src/verify/nan_payload.rs`** — all four decimal128 harnesses
   called production `Decimal128::add` / `Decimal128::mul` directly.
   CBMC symbolically encoded the full `add_kernel` / `mul_kernel`,
   including `add_finite_finite`, `drop_excess_digits`, and the
   `U256::decimal_digit_count` loop. Even though every harness
   restricted its input to a NaN that short-circuits in
   `*_special_cases`, CBMC's `assume(false)` pruning isn't strong
   enough to elide the unreachable code from SAT encoding; each
   harness ran ≥ 5 minutes.

2. **All `ferrodec-decimal{32,64}/src/verify/*.rs` harnesses** — the
   sibling tree was ported without propagating the
   `_special_only_for_kani` convention. `addsub.rs`, `mul.rs`,
   `div.rs`, `sqrt.rs`, `rem.rs`, `fma.rs` all call production ops
   directly. Sibling Kani CI has been silently timing out the entire
   1.2.x cycle on representative harnesses like
   `add_no_panic_special_inputs` (60-second cap, never finishes).

The pattern is uniform: any verify harness that calls a production
arithmetic op (`add`, `sub`, `mul`, `div`, `sqrt`, `rem`, `fma`,
`pow`) directly will time out CBMC, even for inputs the harness
restricts to NaN / Infinity / Zero. The structural fix is to call
the `_special_only_for_kani` shim. With the shim, CBMC's encoding
never sees the finite-path code at all, and harnesses finish in
**< 1 second**.

The remediation was tried during this slice. Switching the four
nan_payload harnesses to shims dropped per-harness time from
≥ 5 min to ≤ 0.21s — a > 1000× speedup. The same conversion across
the sibling trees is mechanical: rename `a.<op>(b, rm)` to
`a.<op>_special_only_for_kani(b, rm).expect("…")` and assert on the
Some-arm.

## Decision

**Standing rule: Kani harnesses MUST NOT call production arithmetic
operations directly. They MUST route through the matching
`<op>_special_only_for_kani` shim.**

The rule applies to:

- `add`, `sub`, `mul`, `div`, `sqrt`, `rem`, `fma`, `pow` — every
  operation whose general path includes loops that CBMC cannot
  tractably encode.

The rule does NOT apply to:

- Predicates that are loop-free (`is_nan`, `is_signaling_nan`,
  `is_zero`, `is_infinite`, `classify_bits`, `partial_cmp`,
  `total_cmp` on bounded operands, `to_bits`, `from_bits`).
- Encoding helpers (`pack_finite`, `pack_quiet_nan`,
  `pack_signaling_nan`).
- Status-flag construction.

### Where the shims live

Each crate exposes `Decimal{32,64,128}::<op>_special_only_for_kani`
as a `#[cfg(kani)]` `#[doc(hidden)]` method. The body delegates to a
private `<op>_special_cases` helper that returns
`Option<(Self, Status)>`:

- `Some((result, status))` when an IEEE special-case rule fires
  (NaN propagation, infinity edge, ±0, 0×∞, etc.).
- `None` when the input requires the op's general path (finite-finite
  arithmetic).

Production code calls `<op>_special_cases` first, then falls through
to the general path. The Kani harness asserts on the shim's Option;
when the harness has restricted inputs to a class that resolves to
`Some`, the `.expect(...)` documents which IEEE rule the harness
expects to fire.

### Where this rule violates today

- decimal128: `src/verify/nan_payload.rs` (fixed in the commit preceding
  this ADR). All other decimal128 verify modules already follow the
  policy.
- decimal32: every harness in `ferrodec-decimal32/src/verify/*.rs`
  needs the shim. Per-crate ops/ files need the `_special_only_for_kani`
  entry points added.
- decimal64: same as decimal32.

The decimal32/decimal64 sweep is the bulk of Slice C of the 1.15
cycle.

## Consequences

**Wins.**

- Kani CI starts gating releases for the first time since 1.2.0. The
  current public posture (`feedback_kani_ci_timeout_ok.md`) becomes
  obsolete; releases can require a green Kani job.
- Each harness's "what it's actually checking" becomes
  self-documenting: `.expect("...")` on the shim's Some-arm names
  the IEEE rule the proof requires to fire.
- Decimal32 and decimal64 join the same proof methodology as
  decimal128. The earlier policy-divergence between the sibling
  trees disappears.

**Costs.**

- Each new IEEE arithmetic op gains the `_special_only_for_kani` +
  `_special_cases` split (a two-function refactor). For new ops
  this is one cycle's friction; for existing ops the refactor is
  mechanical and one-time.
- The decimal32/decimal64 ports require touching every
  `src/ops/<op>.rs` and every `src/verify/<op>.rs` in those crates.

**Drift.**

- A future contributor who writes a new Kani harness calling
  `a.<op>(b, rm)` directly will silently time out CI. The
  CONTRIBUTING-style guidance (or a clippy-equivalent for verify
  modules) should call out the rule explicitly.

## Related

- ADR-0015: Kani scope policy (this ADR refines the shim
  convention named there).
- Plan: 1.15 cycle plan at
  `~/.claude/plans/spawn-6-agents-explore-wondrous-hamster.md`
  (Slice C, revised scope).
- Commits: the immediately-preceding commit converts
  `src/verify/nan_payload.rs` to the shim pattern as the proof of
  concept; subsequent commits in this slice apply the same
  conversion across the sibling trees.
