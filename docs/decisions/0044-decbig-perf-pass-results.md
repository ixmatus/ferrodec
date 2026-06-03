# ADR-0044: DecBig performance pass results

- **Status**: accepted
- **Date**: 2026-06-03

## Context

ADR-0043 captured the `ferrodec-decimal` performance baseline and identified the
candidates the data justified. This ADR records the per-candidate results of the
pass, each measured against the ADR-0043 baseline on the same host (Apple M2
Max, rustc 1.95.0). A candidate ships only on a reproducible win and is reverted
on a neutral or negative measurement, with the outcome recorded here either way
(the ADR-0006/0007/0008 discipline).

Correctness is gated independently of speed: every candidate keeps the default
decTest conformance at 0 fail with the per-file pins intact (it pins exact
`exp` / `ln` / `log10` / `power` outputs to precision 400) and keeps the
libmpdec differential cohort-exact, so a faster kernel that drifted would fail
the build before it could be benched for a win.

## Per-candidate results

| Slice | Candidate | Outcome | Note |
|-------|-----------|---------|------|
| 1 | Rectangular splitting of the logarithm value series | shipped | `ln` up to 4.5x faster; no regressions |
| 1 | Rectangular splitting of the constant series | reverted | regressed the constants; the small-divide loop is already linear |
| 2 | Karatsuba multiply (threshold-gated) | pending | high-precision tail |
| 3 | Newton reciprocal division | deferred | division is not a bottleneck (ADR-0043); re-measure after Slice 2 |

## Slice 1: rectangular splitting of the logarithm series

**Technique.** The logarithm reduces to `atanh(w) = w * sum_{k>=0} (w^2)^k /
(2k+1)`. Summed term by term this advances `(w^2)^k` with one full-width `Work`
multiply per term, so a kernel evaluation is roughly cubic in the working
precision: the dominant high-precision cost the baseline showed. Paterson-
Stockmeyer rectangular splitting (`transc/series.rs`) evaluates the same
polynomial with about `2*sqrt(N)` full multiplies instead of `N`: precompute the
block powers once, accumulate each block from them with only divide-by-small-
integer steps, recombine by a Horner recurrence in `z^s`. Below a tuned term
threshold the term-by-term loop is kept, so low precision is unchanged.

**Why rectangular and not binary splitting.** ADR-0040 named "Brent-McMillan /
binary-splitting `ln`" as the lever, and the plan carried that name. Binary
splitting needs the term ratio to be a ratio of small integers, which holds only
for the rational constants (`z` a power of `1/m`), not for the value series,
whose argument `z = w^2` is an arbitrary full-precision decimal. Rectangular
splitting is the correct accelerator for a power series in a full-precision
argument and is what shipped. This is a deliberate refinement of the plan's named
technique, made when the series shapes were examined closely.

**The reverted half (recorded, not hidden).** The first attempt also routed the
constant series (`ln 2` / `ln 10`, `atanh(1/m)` with `z = 1/m^2`) through the
same evaluator. That regressed the constants by 20-35 % at typical precision and
by ~22 % even at precision 500 for `exp`. The cause: the constant loop never did
a full multiply per term; it divided by the small integer `m^2`, already linear
in the digit count, so it was not the `O(N)`-full-multiplies shape rectangular
splitting targets, and the split only added full multiplies. The constants were
reverted to their small-divide loop. Binary splitting (which the constants' small
integer ratio admits) would accelerate them and is a possible follow-up; it is
not in this slice.

**Result (median per call, ADR-0043 baseline → Slice 1).**

| op | p=16 | p=100 | p=500 |
|----|-----:|------:|------:|
| `ln` | 23.1 → 23.3 µs (flat) | 138 → 84.6 µs (−39 %) | 5.16 → 1.14 ms (**−78 %, 4.5×**) |
| `log10` | 87.3 → 86.5 µs (flat) | 318 → 303 µs (−5 %) | 4.66 → 3.12 ms (**−33 %**) |
| `power` | 162 → 153 µs (−6 %) | 494 → 410 µs (−17 %) | 9.83 → 5.36 ms (**−45 %**) |
| `exp` | 87.3 → 86.4 µs (flat) | 246 → 244 µs (flat) | 3.90 → 3.85 ms (flat) |

The win concentrates where the value series dominates: `ln` directly, `log10`
and `power` through the `ln` they call. `exp` is flat: its Taylor series is
untouched in this slice and dominates its cost; it benefits only through the
(reverted-to-baseline) constants, so it neither wins nor regresses. The core
arithmetic (`add` / `multiply` / `divide` / `sqrt`) is unchanged within noise, as
expected: Slice 1 touches only the transcendental series. No regressions.

**Follow-ups this slice surfaces** (each its own measured candidate):

- `exp`'s Taylor series. Rectangular splitting applies but its factorial
  coefficients make it materially fiddlier; deferred and to be measured
  separately. Until then `exp` (and the `exp` half of `power`) keeps its cubic
  high-precision tail.
- Binary splitting the constant series, to cut the typical-precision floor
  (`log10` / `exp` / `power` recompute `ln 2` / `ln 10` per call). The baseline
  flagged this floor; it is a small-rational binary-splitting candidate.

## Consequences

- The headline high-precision cost ADR-0040 named (the cubic `ln` path) is cut
  by ~4.5× at precision 500, with the common low-precision path unchanged and no
  regression anywhere. The 1.0 performance gate is materially advanced; Slice 2
  (Karatsuba) and the surfaced follow-ups remain.
- A reusable rectangular-splitting series evaluator now exists
  (`transc/series.rs`), available to any future full-precision power series.
- The pass keeps the ADR-0008 audit-trail discipline: the reverted constant
  attempt is recorded with its measured regression and its cause, not dropped.

## Related

- Baseline and candidate justification: [ADR-0043](0043-decbig-perf-baseline.md).
- Named the levers and the cubic `ln` path: [ADR-0040](0040-arbitrary-precision-transcendentals.md).
- Discipline mirrored: [ADR-0007](0007-perf-baseline.md), [ADR-0008](0008-perf-results.md).
- Plan: the DecBig performance pass (`reactive-twirling-toucan`).
