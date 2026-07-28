---
slug: salikhov-zudilin-pi-irrationality
category: algorithm
citation: "Zeilberger, D., Zudilin, W. The irrationality measure of pi is at most 7.103205334137... Moscow Journal of Combinatorics and Number Theory 9(4), 2020, pp. 407-419 (arXiv:1912.06345). Improving Salikhov, V. Kh., On the irrationality measure of pi, 2008 (bound 7.606308...)."
canonical: "https://arxiv.org/abs/1912.06345"
doi: "10.48550/arXiv.1912.06345"
archived: "https://web.archive.org/web/20260709091622/https://arxiv.org/abs/1912.06345"
archive-date: "2026-07-09"
retrieved: "2026-07-27"
sha256: n/a
license: "arXiv preprint (author posted); the journal version is MSP. Pointer and archive."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The current explicit irrationality measure of pi: mu(pi) <= 7.103205334137 (Zeilberger-Zudilin 2020, sharpening Salikhov 2008's 7.606308). The S5 spike's most tractable target: for huge argument trig, |x - k*pi| for canonical decimal128 x (so k up to ~10^6144) has a computable floor from the measure, giving a finite provable ladder cap near 10^4 digits on exactly the thin spot the S1 probe attacks, and a quantitative companion to the U384 pi/2 tripwire. The exponent applies for q beyond an effective threshold; the spike must handle the small q regime explicitly rather than wave at it."
---

# Zeilberger and Zudilin 2020 (irrationality measure of pi)

The one transcendence measure in the lane's toolkit that is small,
explicit, and directly load bearing: it bounds how close a decimal128
argument can sit to a multiple of pi, which bounds the cancellation
in trig argument reduction, which caps the precision any input can
demand. The S5 memo turns the exponent into a digits figure with the
constant bookkeeping shown, banner attached.
