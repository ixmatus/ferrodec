---
slug: registry-rounding-modes
category: registry
citation: "ferrodec registry: the rounding-direction attributes, generated from ferrodec_ieee::RoundingMode."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - ferrodec-ieee/src/status.rs
  - docs/decisions/0005-half-down-05up-wontfix.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The block between the GENERATED markers is rendered by the pin test from RoundingMode itself; an exhaustive match with no wildcard arm makes a sixth variant a compile error before it can be a stale document. Edit the block only by pasting the test's expected output."
---

# Registry: rounding-direction attributes

ferrodec implements exactly the five IEEE 754-2019 §4.3 rounding
direction attributes, across every operation of every format, with
`NearestEven` as the default.

<!-- BEGIN GENERATED: rounding-modes -->
- `NearestEven`: IEEE 754-2019 roundTiesToEven, to nearest with ties to even (the default).
- `NearestAway`: IEEE 754-2019 roundTiesToAway, to nearest with ties away from zero.
- `TowardZero`: IEEE 754-2019 roundTowardZero, truncation.
- `TowardPositive`: IEEE 754-2019 roundTowardPositive, ceiling.
- `TowardNegative`: IEEE 754-2019 roundTowardNegative, floor.
<!-- END GENERATED: rounding-modes -->

Two GDA-only directives are deliberately unsupported: `half_down`
(ties toward zero) and `05up` (round-zero-five-up). ADR-0005 records
the decline; the 99 decTest cases selecting them are skipped, not
coerced, and the cowlishaw-dectest coverage-gap statement counts
them. `RoundingMode::for_negation` (the directed-mode swap under
negation) is a derived helper, not a sixth direction.
