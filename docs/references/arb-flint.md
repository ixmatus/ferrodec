---
slug: arb-flint
category: oracle
citation: "Johansson, F. Arb: efficient arbitrary-precision midpoint-radius interval arithmetic. IEEE Transactions on Computers 66(8), 2017. Merged into FLINT 3."
canonical: "https://flintlib.org/"
doi: "10.1109/TC.2017.2690633"
archived: "https://web.archive.org/web/20260607085830/https://flintlib.org/"
archive-date: "2026-06-07"
retrieved: "2026-06-11"
sha256: n/a
license: "GNU LGPL v2.1 or later. Build-time tool only (python-flint); never enters the Cargo dependency graph."
vendor-status: pointer-only
rot-risk: community-run
provenance: secondary
consumers:
  - tools/gen_transcend_vectors.py
  - tools/certify_anchor_floor.py
  - tools/d32_exhaustive_sweep.py
  - tools/d32_exhaustive_compute_outputs.py
  - docs/decisions/0026-independent-transcendental-oracles.md
  - docs/decisions/0032-correctly-rounded-transcendentals.md
  - docs/decisions/0033-worst-case-margin-completeness.md
verification:
  - tests/vectors/transcend
  - tests/vectors/transcend/acos_d32_exhaustive.prov
notes: "The certified enclosure engine under the correctly rounded claim: ball arithmetic gives rigorous lower and upper bounds, so where both bounds round to the same p-digit value the result is established, not estimated (the Ziv/TMD problem solved offline). Chosen over raw MPFR for the certification semantics and over mpmath for rigor; the legacy standalone site arblib.org is archived at https://web.archive.org/web/20260305102239/https://arblib.org/."
---

# Arb / FLINT (python-flint)

Arb is the proof-tier oracle. `tools/gen_transcend_vectors.py` uses
its certified ball enclosures to find and freeze hard-to-round cases
(the committed corpus with per-value `.prov` provenance, ADR-0026),
and the Decimal32 exhaustive campaign (ADR-0033/0034) pushed the same
machinery across every canonical input: 42 billion inputs, every §9.2
function plus sqrt, margins proven rather than sampled. It runs only
offline through python-flint; the committed corpus is what CI
consumes, so the LGPL C library never touches the build graph.
Since FLINT 3 the Arb code lives inside FLINT, so flintlib.org is the
canonical home and arblib.org is the archived legacy site.
