---
slug: haible-papanikolaou-1998
category: algorithm
citation: "Haible, B., Papanikolaou, T. Fast multiprecision evaluation of series of rational numbers. Technical report TI-7/97, Darmstadt University of Technology; also ANTS-III, LNCS 1423, 1998."
canonical: "https://www.ginac.de/CLN/binsplit.pdf"
doi: "10.1007/BFb0054873"
archived: "https://web.archive.org/web/20260221053736/https://www.ginac.de/CLN/binsplit.pdf"
archive-date: "2026-02-21"
retrieved: "2026-06-11"
sha256: n/a
license: "Authors' technical report hosted on the CLN project site; Springer holds the ANTS-III version. Pointer and archive."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: secondary
consumers:
  - ferrodec-decimal/src/transc/consts.rs
  - docs/decisions/0046-decimal-perf-followups.md
verification:
  - ferrodec-decimal/tests/conformance.rs
notes: "The binary splitting paper: evaluating a rational term series by recursive pair merging keeps every intermediate exact and turns constant evaluation (ln 2, ln 10 via atanh(1/m) series) from quadratic small-step accumulation into balanced big-integer products. Cited directly by the consts module; brent-zimmermann-mca carries the textbook treatment. The CLN hosted PDF is the free copy; the rot class follows the project page hosting."
---

# Haible and Papanikolaou 1998 (binary splitting)

The primary source for the ADR-0046 constants speedup: ferrodec's
`ln` constants evaluate `atanh(1/m)` series by binary splitting above
a term-count threshold, exactly the paper's recursive
numerator/denominator merge. The technique's win shows up in the
1.0.1 bench results (constants dominate the ln profile at high
precision; the binary split constants carried much of the 5x
cumulative `ln` improvement recorded in ADR-0046).
