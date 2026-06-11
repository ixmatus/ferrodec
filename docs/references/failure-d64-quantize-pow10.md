---
slug: failure-d64-quantize-pow10
category: failure
citation: "ferrodec failure museum: Decimal64 quantize rejected representable pads of 10 to 15 digits because its power table was Decimal32 sized (found 2026-06-09; fix tracked as fd-aqs.2)."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - docs/archive/REPORT-rigorous-review-2026-06-09.md
verification:
  - ferrodec-decimal64/tests/conformance.rs
notes: "Open at write time pending fd-aqs.2; recorded now per the at-find-time ritual, fix pointer to follow."
---

# Failure: Decimal64 quantize pads above 9 return NaN

**What shipped.** `1 quantize 1E-10` should produce `1.0000000000`
with `OK`; it produced `(NaN, INVALID)`. The `POW10_U64` pad table in
the Decimal64 quantum module held ten entries, the Decimal32 size,
while Decimal64 legitimately pads up to 15 digits (any
`digits + pad <= 16`). Every pad in `10..=15` failed.

**Why every guard missed it.** A coverage hole alignment: the
vendored ddQuantize corpus never exercises a successful pad above 9,
so the zero-fail pin had nothing to see; quantize sits outside the
exact integer oracle (it is not one of the closed arithmetic ops),
outside the cross precision check, and outside the finite Kani
surface. A sibling-ported table kept its parent's size because no
test asked the Decimal64-specific question.

**The lesson.** Ported code inherits its origin's constants until a
format-parameterized test asks otherwise; the dectest coverage-gap
statement (cowlishaw-dectest entry) now names this class, and
per-format boundary sweeps of pad widths belong with the fix.

**Status.** Open at write time; fd-aqs.2 carries the fix. This entry
gains the fix commit pointer when it lands.
