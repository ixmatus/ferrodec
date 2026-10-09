---
slug: failure-pow-exponent-window
category: failure
citation: "ferrodec failure museum: pow(10, 2147483647) reached the format rounder with the exponent 2^31 - 1 (fd-clc, found 2026-08-05, fixed 6ad73ff)."
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
  - ferrodec-transcend/src/exact.rs
  - docs/decisions/0060-liouville-floors-algebraic-group.md
verification:
  - tests/transcend_exact.rs
notes: "Closed at 6ad73ff (fd-clc): pow's exact classifier gained the exponent window powi carried from day one (EXACT_EXPONENT_WINDOW, 99,999). Written late, by the 2026-10 review (reader D4, finding D4-4), which also found that the window exists as two constants (EXACT_EXPONENT_WINDOW and POWER_OF_TEN_EXPONENT_LIMIT, C3-5) and that the regression test's directed mode rows accept any finite value (C1-27)."
---

# Failure: `pow`'s exact classifier passed an unbounded exponent to the rounder

**What shipped.** `pow_exact_input` assembled the exact result's
exponent `w = α·a` without a window and handed values up to `i32::MAX`
to `round_and_pack_finite`, whose `qmin − unbiased_exp` wrapped: a
debug build panicked and a release build chose the wrong §7.4
disposition for `pow(10, 2147483647)`. The `powi` classifier, written
first, had the guard from day one; `pow`'s copy of the same assembly
did not inherit it.

**Why every guard missed it.** The D3 lane's own classifier corpus was
built around `powi`, where the guard lived; `pow`'s tests sampled
exponents far below the window. A precondition carried by one copy of
a four times copied block is invisible to the tests of the other three
copies.

**The lesson.** A twin that does not inherit its sibling's precondition
is the fd-aqs.2 shape again. One assembly function with one window
constant (the 2026-10 review's C3-5 closure) makes the guard a property
of the code rather than of the author's memory.

**Status.** Closed at 6ad73ff; the regression lives in
`tests/transcend_exact.rs`, whose directed mode rows the 2026-10 review
found accept any finite value (C1-27) and which the remediation epic
re-pins.
