---
slug: c23-draft
category: spec
citation: "ISO/IEC 9899:2024 (C23), Annex F (IEC 60559 floating-point arithmetic), F.10.1.8-F.10.1.11 (acospi, asinpi, atanpi, atan2pi) and F.10.1.12-F.10.1.14 (cospi, sinpi, tanpi) special-value rows. Free proxy: WG14 working draft N3220, 2024-02-21 (the archived PDF's metadata; header ISO/IEC 9899:2024 working draft). Clause numbers corrected by the 2026-10 review (D4-2) against the archived draft."
canonical: "https://www.open-std.org/jtc1/sc22/wg14/www/docs/n3220.pdf"
doi: none
archived: "https://web.archive.org/web/20260727064913/https://www.open-std.org/jtc1/sc22/wg14/www/docs/n3220.pdf"
archive-date: "2026-07-27"
retrieved: "2026-08-10"
sha256: n/a
license: "ISO copyright; the published standard is paywalled. WG14 drafts are publicly posted by the committee; pointer-only, never vendored."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - ferrodec-transcend/src/exact_pi.rs
  - ferrodec-transcend/src/sincospi.rs
  - ferrodec-transcend/src/inverse_trig_pi.rs
  - docs/decisions/0061-pi-scaled-family-design.md
verification:
  - tests/transcend_sinpi.rs
  - tests/transcend_cospi.rs
  - tests/transcend_tanpi.rs
  - tests/transcend_asinpi.rs
  - tests/transcend_acospi.rs
  - tests/transcend_atanpi.rs
  - tests/transcend_atan2pi.rs
  - ferrodec-decimal64/tests/transcend_sinpi.rs
  - ferrodec-decimal64/tests/transcend_cospi.rs
  - ferrodec-decimal64/tests/transcend_tanpi.rs
  - ferrodec-decimal64/tests/transcend_asinpi.rs
  - ferrodec-decimal64/tests/transcend_acospi.rs
  - ferrodec-decimal64/tests/transcend_atanpi.rs
  - ferrodec-decimal64/tests/transcend_atan2pi.rs
  - ferrodec-decimal32/tests/transcend_sinpi.rs
  - ferrodec-decimal32/tests/transcend_cospi.rs
  - ferrodec-decimal32/tests/transcend_tanpi.rs
  - ferrodec-decimal32/tests/transcend_asinpi.rs
  - ferrodec-decimal32/tests/transcend_acospi.rs
  - ferrodec-decimal32/tests/transcend_atanpi.rs
  - ferrodec-decimal32/tests/transcend_atan2pi.rs
notes: "The committee's public working draft (N3220) stands in for the paywalled ISO text; the content is the committee's own, so provenance is primary. The archive is an existing 2026-07-27 Wayback capture verified live on 2026-08-10 (the save API was rate limited). rot-risk stable-publisher is the nearest schema class for a standards body's long lived document server. No DOI is assigned to the draft. Verification: the seven per-op pi-family suites in each of the three format crates."
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
