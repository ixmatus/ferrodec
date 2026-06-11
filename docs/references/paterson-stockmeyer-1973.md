---
slug: paterson-stockmeyer-1973
category: algorithm
citation: "Paterson, M. S., Stockmeyer, L. J. On the number of nonscalar multiplications necessary to evaluate polynomials. SIAM Journal on Computing 2(1), 1973."
canonical: "https://doi.org/10.1137/0202007"
doi: "10.1137/0202007"
archived: "none (paywalled SIAM paper; the DOI is the canonical pointer)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Copyright SIAM. Pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - ferrodec-decimal/src/transc/series.rs
  - docs/decisions/0044-decbig-perf-pass-results.md
verification:
  - ferrodec-decimal/tests/conformance.rs
notes: "The rectangular (baby-step giant-step) polynomial evaluation scheme: O(2 sqrt(N)) expensive nonscalar multiplications instead of N, which is the right trade when series terms are cheap rationals and full precision multiplies dominate. ferrodec-decimal's atanh power series kernel (series.rs) uses it above a precision threshold; ADR-0044 records the measured win for the rectangular ln series."
---

# Paterson and Stockmeyer 1973 (rectangular series evaluation)

The 1973 result that polynomial evaluation needs only about
2 sqrt(N) nonscalar multiplications underlies the `series.rs`
rectangular split: at DecBig precisions the full-width multiply is
the unit of cost, so reorganizing the `atanh` series into blocks of
powers cuts the dominant term of the ln profile. ADR-0044's
distinction between rectangular splitting (this) and binary splitting
(haible-papanikolaou-1998) is load bearing: the first reduces
multiply count at fixed precision, the second restructures exact
rational accumulation; the perf pass learned not to conflate them.
