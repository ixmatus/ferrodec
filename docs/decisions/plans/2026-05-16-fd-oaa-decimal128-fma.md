# fd-oaa: Decimal128 FMA correctness defect — accepted plan

> Archived plan for the fd-oaa slice (ADR-0020). Companion findings
> doc: `2026-05-16-fd-oaa-decimal128-fma-findings.md`. Session scratch
> plan: `~/.claude/plans/reflective-wishing-kernighan.md`.

## Context

`fd-oaa` surfaced incidentally during the decimal32 slice: a
workspace test run persisted a shrunk `property_fma_oracle`
counterexample for the parent `Decimal128`. Scoped out of decimal32
(parent crate untouched), filed for its own triage. The question:
genuine FMA kernel bug at large biased exponents, or astro-float ULP
envelope too tight. Decide with evidence first.

## Phase 1 finding

Genuine kernel bug. `fma(1e33, 1, 3.0)` with `3.0` in cohort
`coef 3000000000000000, exp -15` returns `…000` instead of the exact
`1000000000000000000000000000000003`, with a spurious `INEXACT`.
3 ULP error on an exactly representable result; the 2-ULP envelope is
not at fault. Root cause: the static `shift_ab > SHIFT_LIMIT`
disjunct in `src/ops/fma.rs`, the static-alignment-window
anti-pattern of ADR-0018 / ADR-0019. Scope gate (`add`,
`mul`-then-`add` both sound) confirms FMA-only.

## Decisions (locked, plan-mode Q&A)

- Scope: focused root-cause fix with an empirical scope gate, not a
  full six-agent re-review. One ADR.
- Release: `ferrodec` 1.15.1 patch; CHANGELOG, KNOWN_ISSUES, signed
  tag, ready-to-publish checkpoint in this slice (stop short of
  `cargo publish`).
- fd-fq6: final separate one-concern commit on this branch.

## Plan

Branch `fd-oaa-decimal128-fma` off `main`. Unsigned branch commits,
explicit-path staging, one concern per commit, batch-sign at the
single signed merge boundary (prompted; user away, so all phases
land unsigned and the signed merge is the last step).

1. Phase 1: pin the deterministic reproducer
   (`tests/regression_fd_oaa.rs`) and the empirical scope gate;
   findings doc; KNOWN_ISSUES note; failing case `#[ignore]` to keep
   the per-commit gate green.
2. Phase 2: drop the static raw-shift disjuncts; the sub-ULP trigger
   becomes the dynamic `*_grown_digits > BUFFER_DIGIT_LIMIT` bound,
   mirroring the corrected decimal64/decimal32 admission rule.
   Re-derive helper precondition comments. Un-ignore the reproducer.
3. Phase 3: regression breadth (cohort-depth sweep,
   effective-subtract, all rounding modes, both-side genuine sub-ULP
   non-regression, decimal64 sibling parity).
4. Phase 4: ADR-0020; `ferrodec` 1.15.0 → 1.15.1; CHANGELOG;
   KNOWN_ISSUES; archive plan. Ready-to-publish checkpoint.
5. fd-fq6: drop redundant `dep:libm` from `ferrodec-decimal32`'s
   `num-traits` feature (final separate commit).

## Per-commit gate

`cargo fmt`; `clippy --workspace --all-targets --all-features
-D warnings`; `RUSTDOCFLAGS=-D warnings cargo doc`; `cargo test
--workspace --all-features` (property_fma_oracle green, no tolerated
failure); `cargo kani -p ferrodec`; thumbv6m no_std. Independently
re-run at HEAD; never trust a delegated agent's green.
