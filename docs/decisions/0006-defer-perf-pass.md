# ADR-0006: Defer wholesale perf optimization until profile data exists

- **Status**: superseded by ADR-0007
- **Date**: 2026-05-06

## Context

When 1.10.0 was being scoped, "perf pass" was a candidate for the substantive minor-bump content. Without profile data against a real consumer workload, deciding which paths to optimize is guesswork: criterion benches in `benches/` exist as regression guards, not as optimization targets, and reading code without measurement biases toward "looks expensive" rather than "is expensive."

The 1.10.0 release shipped `Decimal128::rem_trunc` instead, with a CHANGELOG note explicitly framing the deferred perf work:

> A speculative perf pass was scoped out: meaningful optimization needs profiling against a real workload, and ferrodec's criterion benches are calibrated as regression guards rather than as optimization targets. A future release with concrete hot-path data can revisit.

## Decision (at the time)

Skip the perf pass for 1.10.0. Wait for an external trigger before optimizing.

## Consequences (as recorded then)

**Wins:**

- 1.10.0 stayed scoped: one new public API + measured conformance gain. Easy to communicate, easy to verify.
- No risk of speculative regressions in untested paths.

**Costs:**

- The criterion bench numbers in the README continue to reflect the 1.3.0 implementation across most ops. No calibrated baseline existed for future perf work to compare against.

## Why this was superseded

ADR-0007 (the perf-pass plan adopted 2026-05-06) reframes the trigger: the act of expanding the bench surface and capturing a comprehensive baseline *is* the profile data. The optimization candidates that emerge from that data are no longer speculative — each has a measured before/after delta committed to the audit log alongside the code change.

This ADR stays as a record that "we paused first, deliberately" — useful context for future readers who see the perf pass land suddenly and wonder why it didn't happen earlier.

## Related

- ADR-0007 — supersedes; introduces the structured perf-pass approach.
- 1.10.0 CHANGELOG entry — original deferral framing.
