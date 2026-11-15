---
slug: zeilberger-zudilin-2020
category: algorithm
citation: "Zeilberger, D., Zudilin, W. The irrationality measure of pi is at most 7.103205334137... (arXiv:1912.06345; published in Moscow J. Combin. Number Theory, citation to be confirmed at verification)."
canonical: "https://arxiv.org/abs/1912.06345"
doi: none
archived: "https://web.archive.org/web/20260709091622/https://arxiv.org/abs/1912.06345"
archive-date: "2026-07-09"
retrieved: "2026-08-10"
sha256: n/a
license: "arXiv non-exclusive distribution; pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/plans/2026-08-10-s5-transcendence-measure-memo.md
verification:
  - docs/decisions/plans/2026-08-10-s5-transcendence-measure-memo.md
notes: "Title, authors, and the 7.103205334137 bound were checked against the live arXiv abstract on 2026-08-10; the effective constants (C, q0) have not been extracted. The archive is an existing 2026-07-09 Wayback capture verified live on 2026-08-10. The published DOI (Moscow J. Combin. Number Theory) is not recorded and the citation is to be confirmed at verification; the arXiv identifier is the pinned locator. S5 spike only; NOT specialist verified. The memo's own record of what was checked is the only verification artifact, so the verification list points at the memo."
---

# Zeilberger-Zudilin 2020: mu(pi) <= 7.1032...

The sharpest published irrationality measure of pi, the input to the
S5 memo's huge-argument trig cap: an explicit mu turns the trig
reduction's worst-case cancellation into a finite, unconditional
depth bound of shape (mu - 1)(adj + 34) digits. The proof refines
Salikhov's integral construction, so the effective constants are
believed extractable; that extraction is the named specialist step
before any shipped language changes.
