---
slug: c23-draft
category: standard
citation: "ISO/IEC 9899:2024 (C23), Annex F (IEC 60559 floating-point arithmetic), F.10.1.9-F.10.1.12 and F.10.4.6-F.10.4.9 (the sinpi/cospi/tanpi and asinpi/acospi/atanpi/atan2pi special-value rows). Free proxy: WG14 working draft N3220."
canonical: "https://www.open-std.org/jtc1/sc22/wg14/www/docs/n3220.pdf"
doi: n/a
archived: "https://web.archive.org/web/20260727064913/https://www.open-std.org/jtc1/sc22/wg14/www/docs/n3220.pdf"
archive-date: "2026-07-27 (existing capture, verified live 2026-08-10; save API rate limited)"
retrieved: "2026-08-10"
sha256: n/a
license: "ISO copyright; the published standard is paywalled. WG14 drafts are publicly posted by the committee; pointer-only, never vendored."
vendor-status: pointer-only
rot-risk: standards-body
provenance: primary-proxy
consumers:
  - ferrodec-transcend/src/exact_pi.rs
  - ferrodec-transcend/src/sincospi.rs
  - ferrodec-transcend/src/inverse_trig_pi.rs
  - docs/decisions/0061-pi-scaled-family-design.md
verification:
  - tests/transcend_sinpi.rs (and the six sibling per-op suites, x3 formats)
---

# C23 draft (N3220), Annex F pi-scaled special-value rows

C23 standardized the half-revolution trigonometric functions
(`sinpi` .. `atan2pi`) with explicit Annex F special-value tables.
ferrodec implements IEEE 754-2019 §9.2's sinPi family, whose
special-value prose is terse; the Annex F rows are the second of the
two independent proxies (with the MPFR 4.2.2 manual) used to
transcribe and cross-check every §9.2.1 special-value arm in the D4
kernels. The two proxies agree on every row the kernels transcribe;
ADR-0061 requires any future disagreement between them to escalate
to the standard holder rather than be resolved by picking one. The
published ISO text is paywalled, so the entry is pointer-only and
cites the committee's public working draft; clause numbers above are
the draft's.
