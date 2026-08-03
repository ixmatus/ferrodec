---
slug: lauter-lefevre-pow-boundary
category: algorithm
citation: "Lauter, C. Q., Lefèvre, V. An Efficient Rounding Boundary Test for pow(x,y) in Double Precision. IEEE Transactions on Computers 58(2), 2009, pp. 197-207."
canonical: "https://inria.hal.science/inria-00583988"
doi: "10.1109/TC.2008.202"
archived: "https://web.archive.org/web/20250815003030/https://inria.hal.science/inria-00583988"
archive-date: "2025-08-15"
retrieved: "2026-07-27"
sha256: n/a
license: "IEEE copyright; the HAL deposit is the open access author copy. Pointer and archive."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
  - ferrodec-transcend/src/exact.rs
  - ferrodec-transcend/src/pow.rs
verification:
  - ferrodec-test-support/tests/references_integrity.rs
  - tests/transcend_exact.rs
notes: "The binary64 pow exact and boundary case analysis: which (x, y) put x^y exactly on a rounding boundary, filtered before Ziv iteration so the loop provably terminates. ADR-0059's leg 1 needs the decimal analog, derived fresh for base 10 (x = 2^a * 5^b * m with m coprime to 10; x^(p/q) representable only if q divides both exponents and m is a perfect q-th power), with this paper as the shape of the argument and the oracle for what a complete classification covers. Derivation over analogy: the decimal case is rederived in exact.rs and its ADR, not transcribed. Landed at M7 (fd-4zo.15): pow_exact_input carries the criterion, its tie handling through the format rounder, and per-bail completeness proofs, cross-checked in unit tests against the retired ADR-0047 post-hoc witness; the root/sibling transcend_exact suites pin the exact rationals and the PRECISION + 1 ties (pow(5, 49), pow(2, -49))."
---

# Lauter and Lefèvre 2009 (pow rounding boundary classification)

The published proof that pow's boundary cases are completely
classifiable, which is the load bearing premise of the tripod's
first leg: a Ziv style loop (bounded or not) is only a correctness
mechanism once every on boundary input is disposed of beforehand.
The decimal128 classification in `ferrodec-transcend/src/exact.rs`
is the base 10 rederivation this entry grounds.
