---
slug: slz-worst-cases
category: algorithm
citation: "Stehlé, D., Lefèvre, V., Zimmermann, P. Searching Worst Cases of a One-Variable Function Using Lattice Reduction. IEEE Transactions on Computers 54(3), 2005, pp. 340-346."
canonical: "https://inria.hal.science/inria-00000379"
doi: "10.1109/TC.2005.55"
archived: "https://web.archive.org/web/20250815003048/https://inria.hal.science/inria-00000379"
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
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The lattice reduction (Coppersmith style) worst case search that beat Lefèvre's exhaustive method to binary64 sized domains, and the method behind lefevre-stehle-zimmermann-d64-exp. Registered as the honest boundary of the field: SLZ reaches ~2^64 sized domains; decimal128's ~10^38 per function is beyond it, which is why ADR-0059's lane rests on a runtime ladder plus certified sampling rather than a completed worst case table, and why the decimal128 hard case corpus the lane accretes is unpublished territory."
---

# SLZ 2005 (lattice reduction worst case search)

The algorithmic frontier of the table maker's dilemma search program.
Cited by ADR-0059 for the negative fact that matters: no known search
method reaches decimal128 sized domains, so hardest case knowledge
there cannot be a prerequisite for correct rounding claims. The
lane's mechanism is designed around that boundary, and the S5 spike's
transcendence measure caps are the only proof shaped alternative on
the table.
