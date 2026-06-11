---
slug: registry-status-flags
category: registry
citation: "ferrodec registry: the exception status flags, generated from ferrodec_ieee::Status."
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
  - docs/decisions/0002-per-op-status.md
  - docs/decisions/0048-clamped-fidelity-and-bid-residual.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The block between the GENERATED markers is rendered by the pin test from Status's public constants and predicates, including the live bit positions; a bit-count assertion forces this document to move with any seventh flag. Edit the block only by pasting the test's expected output."
---

# Registry: exception status flags

Operations return `(result, Status)`; nothing is global or thread
local (ADR-0002). The six IEEE 754-2019 §7 flags, packed one bit
each:

<!-- BEGIN GENERATED: status-flags -->
- `INVALID` (bit 0, predicate `invalid()`): the operation has no useful definition.
- `DIV_BY_ZERO` (bit 1, predicate `div_by_zero()`): finite non-zero numerator divided by zero.
- `OVERFLOW` (bit 2, predicate `overflow()`): the rounded result exceeds the largest finite magnitude.
- `UNDERFLOW` (bit 3, predicate `underflow()`): the result is tiny, below the smallest normal magnitude.
- `INEXACT` (bit 4, predicate `inexact()`): the rounded result differs from the infinitely precise result.
- `CLAMPED` (bit 5, predicate `clamped()`): the preferred quantum was clamped (informational, IEEE 754-2019 §7.4).
<!-- END GENERATED: status-flags -->

CLAMPED is informational rather than exceptional, and the one flag a
BID storage format structurally cannot always raise where decNumber's
wide working exponent can; ADR-0048 and the cowlishaw-dectest
coverage-gap statement record that residual. Traps do not exist in
this API; accumulation is by `merge` or the `|=` operator.
