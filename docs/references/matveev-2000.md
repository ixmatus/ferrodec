---
slug: matveev-2000
category: algorithm
citation: "Matveev, E. M. An explicit lower bound for a homogeneous rational linear form in the logarithms of algebraic numbers. II. Izvestiya: Mathematics 64(6), 2000, pp. 1217-1269."
canonical: "https://www.mathnet.ru/eng/im314"
doi: "10.1070/IM2000v064n06ABEH000314"
archived: "none (no Wayback snapshot exists for the mathnet.ru record or the ADS mirror as of 2026-07-27, and the save endpoint was unreachable from the citing environment; the DOI was verified resolving to the canonical URL the same day. A manual web.archive.org/save of the canonical URL is the outstanding hedge.)"
archive-date: "n/a"
retrieved: "2026-07-27"
sha256: n/a
license: "Izvestiya RAN / IOP English translation is paywalled; the mathnet.ru record is the journal's own registry. Pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The fully explicit Baker type lower bounds for linear forms in logarithms of algebraic numbers: every constant computable from the degrees and heights involved. The S5 spike's tool for the exp, log, pow, and atan2 boundary families: |f(x) - y| for decimal128 x and boundary point y reduces to a linear form in logarithms whose Matveev bound, however enormous, is a finite provable ladder cap. The expected honest outcome is caps in the 10^8 to 10^12 digit range: useless at runtime, decisive as a termination theorem. Constant bookkeeping (degrees, heights, the Gaussian rational reduction for atan) is number theory past ordinary engineering review; the spike memo carries a not specialist verified banner and no ADR cites a derived cap as a theorem without external verification."
---

# Matveev 2000 (explicit linear forms in logarithms)

The strongest fully explicit bounds in the Baker theory, and the
only published route to a provable finite precision cap for the
transcendental functions' rounding problem at decimal128. The S5
spike applies it per function family and writes the resulting cap or
the documented failure; either lands here as a consumer update.
