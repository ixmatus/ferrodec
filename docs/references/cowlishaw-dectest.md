---
slug: cowlishaw-dectest
category: conformance
citation: "Cowlishaw, M. F. General Decimal Arithmetic Testcases, version 2.62, IBM/Cowlishaw, 2010."
canonical: "https://speleotrove.com/decimal/dectest.zip"
doi: none
archived: "https://web.archive.org/web/20250928040220/https://speleotrove.com/decimal/dectest.zip"
archive-date: "2025-09-28"
retrieved: "2026-06-11"
sha256: b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926
license: "Copyright (c) Mike Cowlishaw, 1981, 2010. All rights reserved. Parts copyright (c) IBM Corporation, 1981, 2008. The testcases are offered on an as-is basis; passing them is not a conformance guarantee."
vendor-status: "vendored-at-path tests/vectors/ (and the three sibling vectors directories, ADR-0042)"
rot-risk: academic-personal
provenance: primary
consumers:
  - tests/conformance.rs
  - ferrodec-decimal64/tests/conformance.rs
  - ferrodec-decimal32/tests/conformance.rs
  - ferrodec-decimal/tests/conformance.rs
  - KNOWN_ISSUES.md
  - docs/decisions/0010-testing-strategy-after-six-agent-review.md
  - docs/decisions/0042-vendored-fixture-integrity.md
verification:
  - tests/vendored_integrity.rs
  - ferrodec-decimal64/tests/vendored_integrity.rs
  - ferrodec-decimal32/tests/vendored_integrity.rs
  - ferrodec-decimal/tests/vendored_integrity.rs
notes: "The only published conformance vector suite for decimal floating point. Vendored unmodified across four directories (dq*, dd*, ds*, and the general precision-driven files), each with a SHA256SUMS manifest enforced by a default-on test (ADR-0042) and per-file pass-count pins (ADR-0010). The live archive's SHA-256 was re-fetched and matched the pin on 2026-06-11."
---

# General Decimal Arithmetic Testcases (decTest, suite 2.62)

The decTest suite is the conformance backbone: 27 591 cases across the
family at last full count, 0 failures, with every per-file pass count
pinned so a silent trade between files fails the build. Each vendored
directory's README records the upstream archive provenance (URL, size,
SHA-256, retrieval dates, extraction subset) and the license verbatim;
this entry is the registry-level home and the coverage-gap statement.

The license is "all rights reserved" with an as-is offer, not an open
license grant. The suite is customarily vendored by implementations
(CPython carries it the same way); the registry records that standing
tension honestly rather than papering over it. No second copy is kept
under `docs/references/vendor/`; the four `tests/vectors/` directories
with their hash manifests are the vendored copy.

## Coverage gaps

What the suite does not exercise feeds the README disclosure's named
failure mode ("rounding errors on boundary cases the decTest suite did
not cover"). The known gaps, from the suite's own structure and from
what the 2026-06-09 rigorous review proved:

1. **The §9.2 transcendental surface on the fixed formats.** The
   format-specific files (dq*, dd*, ds*) cover arithmetic, comparison,
   quantum, logical, and conversion operations only. exp, ln, log10,
   and power vectors exist solely in the general precision-driven
   files consumed by `ferrodec-decimal`; nothing in the suite
   exercises the correctly rounded transcendental contract on
   Decimal32/64/128. That contract rests entirely on the Arb frozen
   corpus, the anchor band corpus, and the Decimal32 exhaustive sweep
   (see the verification-map entry).
2. **Anchor-band and directed-mode boundary classes.** The review
   found real value defects (ln/log10/log2 below 1, atanh/asinh small
   arguments, asin/acos near plus or minus 1, pow near-1 bases,
   directed-mode overflow/underflow gates, grid-stuck small
   arguments) that no decTest vector could have caught, transcendental
   coverage being absent. ADR-0050/0051 closed them and
   `tests/vectors/transcend/anchor_bands/` now pins the class.
3. **Operand patterns the suite never draws.** The Decimal64 quantize
   pad-width defect (fd-aqs.2) survived because no ddQuantize vector
   pads more than 9 digits; the wrong MIN_POSITIVE_NORMAL constants
   (fd-aqs.1) survived because named constants are not operations and
   no vector reads them. Passing the suite bounds neither class.
4. **Vendored subset gaps.** ferrodec vendors a subset of the archive;
   the Decimal128 directory currently lacks dqBase, dqCopy variants,
   dqRemainder, dqToIntegral, dqPlus, dqMinMag, and dqMaxMag
   (fd-aqs.11 tracks vendoring them), so suite coverage of those
   operations is not currently exercised in-tree at that format.
5. **Deliberate skips.** 99 cases under the GDA-only half_down and
   05up rounding directives (will not fix, ADR-0005) and the
   BID-structural CLAMPED residual (20 on Decimal128, 35 on
   Decimal64; intrinsic to the encoding, ADR-0048). Here the suite
   covers what ferrodec declines or cannot raise; KNOWN_ISSUES.md
   holds the per-file taxonomy.
6. **The suite's own disclaimer.** The license text itself warns that
   achieving the same results is not a guarantee of conformance: the
   vectors sample the input space, they never exhaust it. The exact
   integer oracle, the differential oracles, and the exhaustive
   Decimal32 work exist because of this gap.
