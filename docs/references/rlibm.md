---
slug: rlibm
category: algorithm
citation: "Lim, J. P., Nagarakatte, S. One Polynomial Approximation to Produce Correctly Rounded Results of an Elementary Function for Multiple Representations and Rounding Modes. Proc. ACM Program. Lang. 6(POPL), 2022, pp. 1-28. (RLIBM project, Rutgers.)"
canonical: "https://people.cs.rutgers.edu/~sn349/papers/rlibmall-popl-2022.pdf"
doi: "10.1145/3498664"
archived: "https://web.archive.org/web/20241212135237/https://people.cs.rutgers.edu/~sn349/papers/rlibmall-popl-2022.pdf"
archive-date: "2024-12-12"
retrieved: "2026-07-27"
sha256: n/a
license: "ACM published; the Rutgers hosted PDF is the author copy. Pointer and archive."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The RLIBM program (PLDI 2021 and POPL 2022 distinguished papers) generates polynomials that approximate the correctly rounded value rather than the real value, solving a linear program over the rounding intervals; one polynomial then serves multiple representations and rounding modes via round to odd intermediates. Binary formats only, and feasible exactly because binary32 sized domains admit exhaustive interval enumeration; decimal128 does not. ADR-0059 cites it beside core-math for the field position, and its prove the rounding interval, not the value framing is the closest published relative of the lane's rounding boundary predicate."
---

# RLIBM (correctly rounded by interval construction, binary only)

The other contemporary correctly rounded program. Its central move
(target the rounding interval of the correctly rounded result, not
the function value) parallels the ladder's boundary predicate: both
reduce correct rounding to a decidable distance from boundary
question. RLIBM decides it offline per representable input, which is
exactly what decimal128's cardinality forecloses; the ladder decides
it at runtime per call. Registered for that contrast.
