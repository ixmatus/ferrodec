---
slug: core-math
category: algorithm
citation: "Sibidanov, A., Zimmermann, P., Glondu, S. The CORE-MATH Project. ARITH 2022 (29th IEEE Symposium on Computer Arithmetic), 2022. Project: core-math.gitlabpages.inria.fr."
canonical: "https://core-math.gitlabpages.inria.fr/"
doi: none
archived: "https://web.archive.org/web/20260726183836/https://core-math.gitlabpages.inria.fr/"
archive-date: "2026-07-26"
retrieved: "2026-07-27"
sha256: n/a
license: "MIT (the CORE-MATH code); the ARITH paper is open access via HAL (hal-03721525). Pointer and archive; nothing vendored."
vendor-status: pointer-only
rot-risk: community-run
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "CRlibm's successor: correctly rounded C99 functions for the binary formats, aimed at the mandatory correct rounding prospect in the next 754 revision, with worst case aware per function proofs. Registered as the contemporary state of the art ADR-0059 positions against: binary only, which is the flag ferrodec plants in decimal. Previously mentioned only inside crlibm.md's notes; the lane makes it load bearing enough for its own entry."
---

# CORE-MATH (correctly rounded binary libm, the contemporary program)

The active correctly rounded elementary function project (ARITH 2022
best paper). Everything it ships is binary32/binary64; its
methodology (worst case informed fixed budgets per function) is the
CRlibm road ADR-0032 declined for an embedded decimal target.
ADR-0059 cites it for the field position: correct rounding is
becoming table stakes for binary, and decimal128 remains open.
