# ADR-0010: Testing strategy after the 6-agent correctness review

- **Status**: accepted
- **Date**: 2026-05-09

## Context

A 6-agent correctness review (Opus 4.7, six general-purpose subagents
partitioned across addsub/mul/fma, div/rem/sqrt, NaN/cmp/status,
multiword/BID/DPD, transcendentals, and conformance/Kani audit) ran
against the 1.12.0 release. It surfaced six HIGH-severity bugs and a
catalog of MEDIUM/LOW issues.

The HIGH bugs were instructive less for what they were than for where
they hid:

1. **`pow(-1, ±∞) → unreachable!()`**. Single-line panic; no test
   ever called pow with a negative base and infinite exponent. The
   existing pow tests covered every other rule of IEEE 754-2019 §9.2.1
   except this one corner. A property test enumerating the spec's
   special-value rule table would have caught it on the first run.

2. **Parse silently dropped magnitude on integer literals beyond
   76 digits.** A TODO comment in the parser acknowledged the
   problem but the long-mantissa test happened to use an input
   shape where the bug was numerically invisible. The lesson is
   that "we have a long-mantissa test" is not the same as "we have
   a test that would catch any long-mantissa bug" — the test must
   compare against an oracle, not just check `INEXACT`.

3. **`to_dpd_bytes` panicked on non-canonical Form-A coefficients.**
   The decoder's docstring promised arithmetic would canonicalize;
   no kernel actually did. No test exercised arbitrary 128-bit
   inputs through `from_bits`, so the gap survived from the
   foundations layer onward. The fix lives one level deeper than
   the symptom: canonicalize on decode in `classify_bits`, so every
   downstream consumer is safe by construction.

4. **Arithmetic on non-canonical Form A coefficients produced
   ~3.8% wrong results.** Same root cause as #3.

5. **FMA's sub-ULP effective-subtraction directional rounding.**
   `sub_ulp_round` was correct for same-sign sub-ULP (epsilon
   pushes magnitude up) but silently wrong for opposite-sign
   sub-ULP (epsilon pulls magnitude down). decTest had four cases
   that would have caught it (dqadd36466 / 36476 / 36506 / 36516)
   but the runner's pass-floor regression guard is one-sided —
   any 4-case trade-off (e.g. fix one bug while regressing another
   by the same count) slips through unnoticed.

6. **`to_f64` swallowed sNaN signals.** A unit test
   (`to_f64_nan_passes_through`) actively *pinned* the wrong
   behaviour. Pinning a behaviour you haven't checked against the
   spec is a hazard, not a guard.

The audit-leg agent also flagged that the `*_special_only_for_kani`
shims in arithmetic harnesses prove only the special-case path —
finite-finite kernels (where rounding bugs live) have zero symbolic
coverage. That's by design (CBMC budget) and `feedback_kani_strategy.md`
captures it, but it means any "Kani-proven" claim about an arithmetic
op needs the qualifier "special-case dispatch only".

## Decision

Add three layers of guard, each closing a class of failure mode the
review surfaced:

### 1. Per-file conformance expectation, not just a global floor

Replace the single `PASS_FLOOR` integer in `tests/conformance.rs` with
an exhaustive per-file `(name, expected_passes, expected_skips)` table.
The runner compares each file's totals to its row and panics on any
divergence. The asymmetry is intentional: a *legitimate* increase in
pass count requires bumping the table (one-line edit, makes the
intent explicit in git history); a *silent* trade-off (`pass↑file_a
+ pass↓file_b`) becomes a hard failure. The aggregate `FAIL_CEILING
= 0` stays.

This closes the regression-guard gap that masked Phase D's four
dqFMA passes from being a forced "fix or regress" choice.

### 2. Property tests against the surface, not the well-trodden inputs

Add three new property tests, each driven by `proptest::prop::strategy`
fuzzing rather than hand-curated inputs:

- **`tests/property_from_bits.rs`** — generate arbitrary `u128` and
  assert: (a) `Decimal128::from_bits(b).classify_bits()` round-trips
  through `pack_*`; (b) `to_dpd_bytes` never panics on any input
  including non-canonical encodings; (c) `is_canonical` matches the
  IEEE 754-2019 §3.5.2 predicate; (d) arithmetic on a non-canonical
  input produces a result numerically equal to arithmetic on its
  `canonicalize()`d form. Catches the H3/H4 surface in 2048 cases
  per property.

- **`tests/property_pow_specials.rs`** — enumerate IEEE 754-2019 §9.2.1's
  special-value rule table over `(x, y, rm)` with `x, y` drawn from a
  small set of distinguished constants (±0, ±1, ±MIN, ±MAX, ±∞, qNaN,
  sNaN) and `rm` drawn from all five modes. Asserts the spec rule for
  every combination. Would have caught H1 on first run.

- **`tests/property_fma_oracle.rs`** — cross-check `a.fma(b, c, rm)`
  against `mul`-then-`add` for every input where the two are
  algebraically required to agree (no overflow / no sub-ULP / no
  catastrophic cancellation), and against `astro-float` at extended
  precision otherwise. Catches the H5 shape directly: opposite-sign
  sub-ULP fma where the directional rounding picked the wrong
  candidate.

### 3. Two new Kani harnesses for the surface that proptest can't cover

Symbolic execution catches the *totality* properties that fuzzing can
miss. Add:

- **`src/verify/pow.rs`** — proves `pow(±1, ±∞) = 1` and
  `pow(x, ±0) = 1` for any non-sNaN `x` in the special-constant pool.
  Bounded but exhaustive on the special inputs that matter.

- **`src/verify/nan_payload.rs`** — proves NaN payload propagation
  for `add`/`mul` over symbolic 8-bit-bounded payloads. The bound is
  intentional (full 110-bit symbolic blows the CBMC budget per
  `feedback_kani_strategy.md`); 8 bits is enough to expose any
  payload-dropping bug because the payload threading is uniform on
  width.

The `*_special_only_for_kani` shim convention stays. The new
harnesses are explicit about what they do and don't prove (in their
docstrings) so future readers don't infer "Kani-proven" guarantees
beyond the actual claim.

## Consequences

**Wins.**

- The bug shape behind H3/H4 (non-canonical `from_bits` inputs) is
  now a fuzzed surface, not an implicit contract. Future kernels
  will fail fast on the property test rather than at a
  downstream consumer.
- The regression-guard one-sidedness that masked the H5 fix
  is closed. Any future trade-off requires explicit table
  edit in conformance.rs.
- pow's spec table is now machine-checked end-to-end. Any future
  pow refactor will have to thread the special-case rules
  correctly or fail the property test.
- FMA gets oracle-grade cross-checks (was: decTest only).

**Costs.**

- Conformance per-file table is ~25 lines of constants. Each
  legitimate pass-count change is one line; the cost is per-edit
  not amortized.
- The three new property test files run in ~2 seconds aggregate
  (proptest defaults). No CI-time concern.
- The two new Kani harnesses do not add measurable cost on their
  own. The original timing claim ("well within the 2-minute budget")
  proved wrong: `pow_special_pool_total` invoked production
  `Decimal128::pow` over an 11-constant pool that included
  general-path inputs (MAX, MIN, from_i32(2)), dragging the
  `ln_extended` / `exp_from_extended` pipeline through CBMC
  symbolically and driving the full Kani run into the chronic
  timeout. The remediation lands in 1.15 (ADR-0015): rules 1–7
  factor into `pow_special_cases`, the harness routes through
  `pow_special_only_for_kani`, and the general path is delegated to
  `tests/property_pow.rs`'s astro-float oracle. See ADR-0015 for the
  standing Kani scope policy.
- ADR drift: this ADR's claims need to stay accurate. If the
  per-file table or property tests are removed, this ADR should be
  marked superseded.

**Explicitly out of scope (deferred to follow-ups).**

- The MEDIUM findings from the review (NaN payload tie-breaker
  fragility in `total_cmp`, `sqrt(0)` quantum, `to_f32`
  double-rounding, `to_unsigned(-0.4) = INVALID`, FMA `0×Inf`
  drops sNaN payload, FMA "ab dominates in-range" double-rounds,
  `exp` underflow asymmetry, `argred::FRAC_DIGITS` margin) are
  correctness issues but not reachable through the public API
  with sufficient impact to block 1.12.1. Tracked under separate
  TODO items; will be addressed in 1.13.
- Coverage gaps the audit identified for `copySign`, `logB`
  proptest, and `round_to_integral` symbolic coverage are
  legitimate but lower priority than the FMA oracle gap; they
  remain on the testing-debt list.

## Related

- Commits: `f0b6a16` (H3/H4), `96f6d3d` (H1), `7b7a0fd` (H2),
  `105fe40` (H5), `67bd45c` (H6), and the Phase F commit landing
  this ADR plus the new tests.
- Other ADRs: complements ADR-0004 (skip Verus graduation) and
  ADR-0009 (DPD interchange's totality proof model). Builds on
  the `feedback_kani_strategy.md` and `feedback_close_known_issues.md`
  conventions in author memory.
- KNOWN_ISSUES.md: unchanged; the 99 GDA-only rounding-mode skips
  remain the only open backlog category.
