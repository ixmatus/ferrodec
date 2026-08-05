---
slug: shidlovskii-transcendence
category: algorithm
citation: "Shidlovskii, A. B. Transcendental Numbers. De Gruyter Studies in Mathematics 12, de Gruyter, 1989 (transl. Koblitz, N.)."
canonical: "https://www.degruyter.com/document/doi/10.1515/9783110889055/html"
doi: "10.1515/9783110889055"
archived: "https://web.archive.org/web/20250118021649/https://www.degruyter.com/document/doi/10.1515/9783110889055/html"
archive-date: "2025-01-18"
retrieved: "2026-07-27"
sha256: n/a
license: "de Gruyter, paywalled monograph. Pointer only; cite chapter and theorem numbers."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
  - ferrodec-transcend/src/exp.rs
  - ferrodec-transcend/src/ln.rs
  - ferrodec-transcend/src/sincos.rs
  - ferrodec-transcend/src/inverse_trig.rs
  - ferrodec-transcend/src/hyperbolic.rs
  - ferrodec-transcend/src/ln.rs (logp1 family exactness citations, ADR-0059 Track D)
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The Siegel-Shidlovskii theory of E-functions: transcendence and transcendence measures for values of exp, sin, cos, sinh, cosh at algebraic points. Grounds two distinct things in the lane. First, the tripod's no ties fact for the exponential family (nonzero rational arguments give transcendental values, so no representable input lands on a rounding boundary). Second, the S5 spike's question of whether the E-function transcendence measures are explicit enough to yield computable ladder caps; the anticipated honest finding is effective but not explicit, in which case matveev-2000's log forms route supplies the explicit caps instead and this entry records the negative result."
---

# Shidlovskii 1989 (E-function transcendence measures)

The standard monograph for the E-function method. Registered at lane
charter time because ADR-0059's classification leg cites the
transcendence facts per function, and the S5 spike must engage the
measure chapters before concluding that only the linear forms route
gives explicit constants. Chapter and theorem citations accrete into
this entry as the spike reads them.
