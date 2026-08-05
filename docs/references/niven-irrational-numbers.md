---
slug: niven-irrational-numbers
category: algorithm
citation: "Niven, I. Irrational Numbers. Carus Mathematical Monographs No. 11, Mathematical Association of America, 1956."
canonical: "MAA Carus Mathematical Monographs No. 11"
doi: none
archived: "none (printed monograph; no archivable free copy)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Copyright MAA. No paper copy owned; pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - ferrodec-transcend/src/exact.rs
  - ferrodec-transcend/src/exp.rs
  - ferrodec-transcend/src/ln.rs
  - ferrodec-transcend/src/sincos.rs
  - ferrodec-transcend/src/inverse_trig.rs
  - ferrodec-transcend/src/hyperbolic.rs
  - ferrodec-transcend/src/ln.rs (logp1 family exactness citations, ADR-0059 Track D)
  - ferrodec-transcend/src/exp.rs (expm1 family exactness citations, ADR-0059 Track D D2)
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The accessible source for the irrationality corollaries the per-function Exactness-and-ties rustdoc cites (ADR-0059 M7): e^r, ln, the trig and hyperbolic values at nonzero rational arguments are irrational (the Lindemann-Weierstrass consequences; shidlovskii-transcendence carries the full transcendence theory), and the rational-power arguments behind the exp2 / log2 / log10 / pow classifiers. Theorem-level pin cites are deliberately absent: no copy is on hand, and citing chapter or theorem numbers from memory would violate the provenance discipline. They accrete here when a physical or licensed copy is checked. The elementary unique-factorization derivations in exact.rs are self-contained and do not depend on the book; the entry grounds the named corollaries only. Niven's theorem on rational sines becomes load bearing for the Track D pi-scaled family (sinPi through atan2Pi), which is the reason the book, not just the L-W literature, is registered."
---

# Niven 1956 (Irrational Numbers, Carus Monograph 11)

The accessible citation behind every "no exact cases, no ties"
rustdoc block in the transcendental kernels: irrationality of the
elementary functions' values at nonzero rational arguments, stated at
the level a maintainer can check without the full Siegel-Shidlovskii
machinery. The Track D pi-scaled family will lean on Niven's theorem
proper (the rational values of sine at rational multiples of pi),
which makes this a two-arc registration.
