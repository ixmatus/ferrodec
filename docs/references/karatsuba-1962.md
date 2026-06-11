---
slug: karatsuba-1962
category: algorithm
citation: "Karatsuba, A., Ofman, Yu. Multiplication of multidigit numbers on automata. Doklady Akademii Nauk SSSR 145, 1962 (English translation: Soviet Physics Doklady 7, 1963)."
canonical: "Soviet Physics Doklady 7 (1963), 595 to 596"
doi: none
archived: "none (Soviet era journal; no canonical free copy to archive; Knuth §4.3.3 is the consulted text)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Historical journal publication. Pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - ferrodec-multiword/src/decbig.rs
  - docs/decisions/0044-decbig-perf-pass-results.md
verification:
  - ferrodec-multiword/tests/decbig.rs
notes: "The original divide and conquer multiplication. Standalone entry so ADR-0044's threshold discussion has a stable slug, but the consulted text is Knuth TAOCP §4.3.3 (knuth-taocp-v2); nobody read the Doklady original for this work, and the entry says so rather than implying otherwise."
---

# Karatsuba 1962

`DecBig::mul` switches from schoolbook to one-level Karatsuba above
a limb threshold tuned in the ADR-0044 pass (2.8x at 4000 digits).
The registry keeps the original as its own entry for citation
honesty: the algorithm's name and idea trace to the 1962 paper, the
implementation derives from Knuth's presentation, and the threshold
and the recursion through `mul` are the project's own. The
cross-check tests recover factors through the independent Algorithm D
division, so the Karatsuba path is verified against a kernel that
shares no code with it.
