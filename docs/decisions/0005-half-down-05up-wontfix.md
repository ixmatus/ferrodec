# ADR-0005: Will-not-fix `half_down` / `05up` rounding directives

- **Status**: accepted
- **Date**: 2026-05-06

## Context

The decTest conformance suite (Mike Cowlishaw's General Decimal Arithmetic Testcases, vendored at `tests/vectors/`) exercises seven rounding directives:

- The five IEEE 754:2019 §4.3 directions (`NearestEven`, `NearestAway`, `TowardZero`, `TowardPositive`, `TowardNegative`).
- A directional `up` mode (round magnitude away from zero, distinct from IEEE `TowardPositive`).
- Two GDA-only modes that aren't in IEEE 754: `half_down` (nearest, ties toward zero) and `05up` (round-zero-five-up, a banker's-rounding variant).

ferrodec ships only the five IEEE attributes, plus a runner-side two-pass emulation of `up`. Cases under `half_down` / `05up` were originally counted as conformance skips: at the 1.7.1 baseline, 101 of the 572 total skips fell into this category.

When 1.10.1 closed every other documented skip category, the residual was exactly these 101 cases (now 99 after 1.10.1's null-test handling absorbed two cases that happened to be under non-IEEE rounding directives).

## Decision

Don't add `half_down` or `05up` to ferrodec's `RoundingMode` enum. The conformance runner skips cases under those directives explicitly. Document the non-fix in `KNOWN_ISSUES.md` and this ADR.

## Consequences

**Wins:**

- The `RoundingMode` enum stays at five variants — exactly the IEEE 754:2019 set. No surface for users to accidentally reach the "right syntax, wrong spec" rounding modes.
- Embedded callers paying for the kernel size don't pay for two extra rounding-direction branches in the rounding pipeline.
- The ferrodec / decTest conformance ceiling sits at "every IEEE op under every IEEE rounding mode passes" — a clean, defensible claim. Adding GDA-only modes would convert that into "everything passes including some non-spec modes," which is messier to communicate.

**Costs:**

- 99 conformance cases stay in the skip bucket. The per-file totals (`dqQuantize` 64, `dqFMA` 26, `dqAdd` 8, `dqDivide` 1) appear as "skip" rows in every conformance report, requiring the explanation in `KNOWN_ISSUES.md`.
- A user porting from a GDA-conformant library that relied on `half_down` / `05up` has to either implement the modes externally or accept the IEEE alternatives. ferrodec doesn't supply them.

**Why this isn't reconsidered:**

The decTest skips are static — the 99 cases will stay 99 regardless of how many IEEE ops we add. The trade is a 1.1 % conformance footnote in exchange for keeping the `RoundingMode` surface aligned with IEEE 754:2019. If a downstream user files a real need for `half_down` (the more common of the two), it can be revisited then; absent that, the surface stays clean.

## Related

- `KNOWN_ISSUES.md` — categorises the 99 residual conformance skips.
- `tests/conformance.rs` — `CaseRounding::Unsupported` variant skips these cases at the runner level.
- `src/status.rs::RoundingMode` — the five-variant enum.
