# ADR-0046: ferrodec-decimal performance follow-ups (post-1.0)

- **Status**: accepted
- **Date**: 2026-06-03

## Context

The 1.0 performance pass (ADR-0043 baseline, ADR-0044 results) shipped a
rectangular-split logarithm series and a Karatsuba multiply, and filed three
candidates it deliberately did not take: a Newton reciprocal division, splitting
`exp`'s Taylor series, and binary-splitting the `ln 2` / `ln 10` constant series.
They are performance only, not API, so they land after 1.0 as patch work. This
ADR records their per-candidate results under the same discipline as ADR-0044
(ship on a reproducible win, revert on neutral, every outcome recorded; measured
against the 1.0 release on the same host, Apple M2 Max, rustc 1.95.0).

The throttling lesson from ADR-0044 recurred and was applied: the constant-using
operations were re-measured cold and isolated when a combined run showed
late-bench swings with no causal path to the change.

## Per-candidate results

| Candidate | Outcome | Note |
|-----------|---------|------|
| Binary-split the `ln 2` / `ln 10` constant series | shipped | `log10` −47 %, `exp` −21 %, `power` −13 % at precision 500; large wins at typical precision too |
| Speed `exp` by argument halving | shipped | `exp` −45 %, `power` −34 % at precision 500 (chosen over factorial-rectangular splitting) |
| Newton reciprocal division | pending | |

## Binary-split the constant series

**Technique.** `ln 2 = 2*atanh(1/3)` and `ln 10 = 3*ln 2 + 2*atanh(1/9)`, and
`atanh(1/m) = (1/m) * sum_{k>=0} 1/((2k+1) m^(2k))`. With `M = m^2` an integer,
that sum is a small-rational hypergeometric series, the case binary splitting is
built for: the partial sum is an exact ratio of big integers `(Q + T) / Q`
(Haible-Papanikolaou), so `atanh(1/m) = (Q + T) / (m * Q)` is a balanced product
tree of `DecBig` integer multiplies (themselves now Karatsuba) plus one final
divide, `O(M(D) log D)` against the term-by-term loop's `O(D^2)`. The previous
constant loop was kept below a term threshold, where its small-integer divides
beat the tree's recursion overhead (`consts.rs`).

**Why this was bigger than expected.** ADR-0044 left the constants on the
term-by-term loop, judging them a small slice. The cold measurement showed the
opposite: the constants dominate `log10` (which also forms `1/ln 10`) and weigh
heavily on `exp` (range reduction needs `ln 10`) and `power` (both). Splitting
them is the single largest typical-precision win of the whole performance effort.

**Result (median per call, 1.0 release → here).**

| op | p=16 | p=100 | p=500 |
|----|-----:|------:|------:|
| `log10` | 87.6 → 51.3 µs (−41 %) | 301.8 → 166.7 µs (−45 %) | 3.01 → 1.60 ms (**−47 %**) |
| `exp`   | 86.3 → 60.2 µs (−30 %) | 242.6 → 167.6 µs (−31 %) | 3.39 → 2.67 ms (−21 %) |
| `power` | 154.1 → 128.4 µs (−17 %) | 400.2 → 326.9 µs (−18 %) | 4.82 → 4.20 ms (−13 %) |

`ln` is unchanged: `ln(2)` takes the near-one path, which needs no constant.

**Correctness.** A unit test cross-checks the split against the term-by-term loop
for `m = 3` and `m = 9` at high precision; the `ln 2` / `ln 10` / `1/ln 10`
reference-value tests now route through the split; the decTest conformance stays
0-fail and the libmpdec differential cohort-exact (the constants back every
`log10` / `exp` / `power` case).

## Speed `exp` by argument halving

**Technique chosen over the filed one.** The filed candidate was rectangular
(Paterson-Stockmeyer) splitting of the `exp` Taylor series, the analogue of the
logarithm's Slice 1. Its factorial coefficients (`1/n!`) make the splitting
materially fiddlier and riskier than the logarithm's `1/(2k+1)`. Argument halving
reaches the same `O(sqrt(ip))` full-multiply count far more simply:
`e^r = (e^(r / 2^j))^(2^j)`. Reducing `r` by a power of two (exact: multiply the
coefficient by `5^j`, lower the exponent) shrinks the Taylor term count from
about `ip` to about `ip/j`; squaring the result back `j` times costs `j`
multiplies, so `j ~ sqrt(ip)` balances the two. The `j` squarings amplify the
error by `2^j` (about `0.3j` digits), absorbed by `j` guard digits on the
internal precision (`exp.rs`).

**Result (post-constants state → here, median per call).**

| op | p=100 | p=500 |
|----|------:|------:|
| `exp`   | 167.6 → 156.6 µs (−7 %) | 2.67 → 1.46 ms (**−45 %**) |
| `power` | 326.9 → 281.6 µs (−14 %) | 4.20 → 2.77 ms (**−34 %**) |

`power` benefits because its general path evaluates `exp(y * ln x)`. Low precision
sees a small win and no regression (the halving is cheap when the term count is
already small). Cumulatively against the ADR-0043 baseline, `exp` is 2.7x faster
at precision 500 and `power` 3.5x.

**Correctness.** A unit test checks `exp(1) = e` to 48 digits through the halving
and squaring path; the decTest conformance stays 0-fail and the libmpdec
differential cohort-exact.

## Consequences

The patch is a strict improvement on `log10` / `exp` / `power` at every
precision, with no regression and the small-rational constants finally on the
algorithm built for them. It also reframes the remaining candidates: with the
constant cost removed, `exp`'s residual high-precision cost is its Taylor series
(the next candidate), and the `exp` improvement here came from constants rather
than from the series.

## Related

- The pass this continues: [ADR-0043](0043-decbig-perf-baseline.md),
  [ADR-0044](0044-decbig-perf-pass-results.md).
- Method: Haible and Papanikolaou, "Fast multiprecision evaluation of series of
  rational numbers" (1998); Brent and Zimmermann, *Modern Computer Arithmetic*,
  4.9.
