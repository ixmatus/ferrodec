---
slug: knuth-taocp-v2
category: algorithm
citation: "Knuth, D. E. The Art of Computer Programming, Volume 2: Seminumerical Algorithms, 3rd edition. Addison-Wesley, 1997."
canonical: "ISBN 978-0-201-89684-8"
doi: none
archived: "none (printed book; no archivable free copy)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Copyright Addison-Wesley. No paper copy owned; pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - ferrodec-multiword/src/decbig.rs
  - ferrodec-multiword/src/lib.rs
  - docs/decisions/0038-arbitrary-precision-decimal.md
  - docs/decisions/0043-decbig-perf-baseline.md
verification:
  - ferrodec-multiword/tests/decbig.rs
notes: "Source of Algorithm D (§4.3.1), the long division DecBig::div_rem derives at radix 10^9, and the consulted text for Karatsuba multiplication (§4.3.3; the karatsuba-1962 entry records the original). Derived, not transcribed: the published algorithm is the source, the radix 10^9 limb implementation and its add-back witness test are the project's own."
---

# Knuth TAOCP Volume 2 (Seminumerical Algorithms)

The textbook source for the two classical kernels in
`ferrodec-multiword`: Algorithm D long division (with the rare
add-back correction step the fd-aqs.14 witness test pins) and the
Karatsuba threshold treatment behind `DecBig::mul`. The book is the
canonical stable-publisher citation; nothing about it needs
archiving, and the registry records it for the derivation chain
rather than rot protection.
