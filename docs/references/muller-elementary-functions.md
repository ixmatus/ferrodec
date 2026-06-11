---
slug: muller-elementary-functions
category: algorithm
citation: "Muller, J.-M. Elementary Functions: Algorithms and Implementation, 3rd edition. Birkhäuser, 2016."
canonical: "https://doi.org/10.1007/978-1-4899-7983-4"
doi: "10.1007/978-1-4899-7983-4"
archived: "none (paywalled Springer book; no archivable free copy)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Copyright Springer/Birkhäuser. Pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - ferrodec-decimal/src/transc/consts.rs
verification:
  - ferrodec-decimal/tests/conformance.rs
notes: "Cited in the ferrodec-decimal constants module for the range reduction analysis behind the working precision model of the atanh-based logarithm constants. The Handbook covers the proofs; this book covers the per-function algorithmic playbook, and the constants pipeline cites it where the two diverge."
---

# Elementary Functions (Muller, 3rd ed.)

The per-function companion to the Handbook: range reduction
strategies, series versus iteration trade-offs, and the error budget
bookkeeping for implementing individual elementary functions. The
`ferrodec-decimal` transcendental constants module cites it for the
range reduction reasoning that sets the guard digit budget on the
`atanh(1/m)` constant evaluations.
