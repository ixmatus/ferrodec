# ADR-0043: DecBig and transcendental performance baseline

- **Status**: accepted
- **Date**: 2026-06-03

## Context

ADR-0038 sets four gates between `ferrodec-decimal` 0.x and 1.0: arbitrary
precision transcendentals (delivered, ADR-0040), full general decTest
conformance (delivered, ADR-0039), a settled public API, and a performance
pass. The spec surface is complete as of 0.3.0; this ADR opens the performance
pass.

The backend is correct but uses the simplest algorithms. `DecBig::mul` is
schoolbook, quadratic in limb count. `DecBig::div_rem` is Knuth Algorithm D,
also quadratic. The transcendental kernels evaluate their series term by term
on a variable precision `Work` float, so the `ln` / `exp` cost is roughly cubic
in the working precision: order `wp` series terms, each a `DecBig` multiply and
divide that are order `(wp/9)^2` in limbs. ADR-0040 names this directly ("the
high precision `ln` path is not yet optimised") and lists the levers: Newton
reciprocal division, Karatsuba multiplication, and a Brent-McMillan
binary-splitting series.

Until now there was no benchmark for `DecBig` or `ferrodec-decimal`; the four
existing benches measure only the fixed-width `Decimal128`. Under the project's
perf discipline (ADR-0006/0007: profile before patching, revert on a neutral
measurement) the pass cannot start without a baseline. This ADR captures that
baseline, following ADR-0007's stance that the bench numbers *are* the profile
data.

## Decision

Adopt the numbers below as the baseline for the `ferrodec-decimal` performance
pass. Two new bench harnesses, local to the crates they measure (each library
stands alone):

- `ferrodec-multiword/benches/decbig_ops.rs` — `mul`, `div_rem`, `isqrt`,
  `mul_pow10`, `div_rem_pow10` swept across digit widths from a single limb (9)
  into the high-precision tail (4000), plus a magnitude-extreme `div_rem` guard.
  Behind the `alloc` feature.
- `ferrodec-decimal/benches/decimal_ops.rs` — the core arithmetic (`add`,
  `multiply`, `divide`, `sqrt`) with operands sized to the precision, and the
  four transcendentals (`exp`, `ln`, `log10`, `power`) with small fixed inputs,
  swept across context precision (16, 34, 50, 100, 500).

Each optimization candidate reports its delta against these numbers in
ADR-0044 and is reverted on a neutral or negative measurement. The swept widths
double as the evidence for *which* candidates the data justifies and for the
crossover thresholds that keep the small-precision path untouched.

## Baseline numbers

**Host**:

- Apple M2 Max (arm64), 12 cores; macOS 26.4.1 (build 25E253).
- rustc 1.95.0 stable (59807616e 2026-04-14).
- `ferrodec-decimal` code at commit `b38d45c` (release/bench profile: thin LTO,
  1 codegen unit, opt-level=3). The bench harnesses themselves are the new
  work; the measured library code is unchanged from `b38d45c`.

**Method**: `cargo bench -p <crate> [--features alloc] --bench <name> --
--warm-up-time 1 --measurement-time 3`. Numbers are the criterion median
(middle of the reported [low, mid, high] range). The reduced warm-up and
measurement windows trade a little stability for a tractable full-suite run on
the recorded host; the IQR stays within a few percent of the median.

### `decbig_ops` (median per call, by operand digit width)

`div_rem` divides a `2n`-digit dividend by an `n`-digit divisor; `isqrt` takes
an `n`-digit input; the `pow10` ops shift by `n` digits.

| digits | `mul` | `div_rem` | `isqrt` | `mul_pow10` | `div_rem_pow10` |
|-------:|------:|----------:|--------:|------------:|----------------:|
|      9 | 15.8 ns | 30.0 ns | 456 ns | 17.0 ns | 31.0 ns |
|     18 | 21.6 ns | 96.5 ns | 482 ns | 18.7 ns | 29.2 ns |
|     34 | 41.8 ns | 195 ns | 1.02 µs | 40.9 ns | 114 ns |
|     50 | 73.9 ns | 261 ns | 1.51 µs | 45.2 ns | 125 ns |
|    100 | 267 ns | 641 ns | 3.01 µs | 62.2 ns | 152 ns |
|    200 | 944 ns | 1.91 µs | 7.45 µs | 102 ns | 216 ns |
|    500 | 7.10 µs | 9.86 µs | 33.9 µs | 244 ns | 445 ns |
|   1000 | 31.1 µs | 36.9 µs | 130 µs | 458 ns | 864 ns |
|   4000 | 521 µs | 560 µs | 2.16 ms | 1.68 µs | 3.13 µs |

`div_magnitude_extreme` (4000-digit dividend, 7-digit divisor): 2.04 µs. Cheap,
because a single-limb divisor takes the linear `div_rem_small` fast path.

### `decimal_ops` (median per call, by context precision)

| precision | `add` | `multiply` | `divide` | `sqrt` | `ln` | `log10` | `exp` | `power` |
|----------:|------:|-----------:|---------:|-------:|-----:|--------:|------:|--------:|
|        16 | 242 ns | 270 ns | 292 ns | 1.15 µs | 23.1 µs | 87.3 µs | 87.3 µs | 162 µs |
|        34 | 280 ns | 314 ns | 383 ns | 2.55 µs | 39.7 µs | 127 µs | 117 µs | 210 µs |
|        50 | 296 ns | 406 ns | 468 ns | 4.05 µs | 56.4 µs | 168 µs | 143 µs | 269 µs |
|       100 | 399 ns | 506 ns | 853 ns | 7.57 µs | 138 µs | 318 µs | 246 µs | 494 µs |
|       500 | 992 ns | 8.05 µs | 11.1 µs | 129 µs | 5.16 ms | 4.66 ms | 3.90 ms | 9.83 ms |

## Observations the baseline highlights

The bench is the profile data (ADR-0007's stance). The swept widths fix the
scaling laws, and combined with the static call trace of the kernels they pin
the hot paths without a separate sampling run.

- **The primitives are clean quadratic.** `mul` and `div_rem` both grow ~16×
  for a 4× width step (1000→4000 digits), an exponent of ~2.0. `div_rem` is
  only ~1.1× `mul` at 4000 digits, never a standout: division is *not*
  disproportionately expensive relative to multiplication anywhere in the
  sweep.
- **The transcendentals dominate by orders of magnitude.** At precision 500,
  `ln` is 5.16 ms and `power` is 9.83 ms, versus ≤ 11 µs for every core op. The
  performance pass is a transcendental story; the core arithmetic is already
  cheap.
- **The high-precision tail is the cubic regime.** `ln` grows from 138 µs at
  precision 100 to 5.16 ms at 500: 37× for a 5× precision step (exponent
  ~2.25, heading toward 3 as the per-term `DecBig` quadratic cost overtakes the
  linear term count). This is the O(wp³) path ADR-0040 named. Binary splitting
  (fewer, larger `DecBig` products plus one final divide) and Karatsuba (a
  cheaper large product) attack exactly this.
- **A large fixed floor at typical precision comes from the constants.** At
  precision 16, `log10` and `exp` are 87 µs each and `power` is 162 µs, while
  `ln` is only 23 µs. The gap is constant computation: `ln(2)` takes the
  near-one path and needs no stored constant, but `log10` / `exp` / `power`
  recompute `ln2`, `ln10`, and `inv_ln10` from scratch each call via the same
  `atanh` series (at `wp + 12` digits), and `power` runs two kernels. Binary
  splitting the constant series in `consts.rs` cuts this directly, so the
  Slice 1 candidate helps the typical-precision floor as well as the
  high-precision tail.

### What the data justifies for ADR-0044

- **Binary-splitting the series (Slice 1): confirmed top priority.** It is the
  only candidate that helps both regimes (the value series and the constant
  series) at every precision, and it is independent of the `DecBig` internals.
- **Karatsuba multiply (Slice 2): confirmed, scoped to the high-precision
  tail.** Schoolbook is fine through ~100–200 digits; the quadratic penalty
  only bites at 500+ digits (7.1 µs) and 4000 (521 µs). The crossover threshold
  (to be tuned in Slice 2) keeps the ≤ ~50-digit common path on schoolbook,
  untouched.
- **Newton reciprocal division (Slice 3): deferred, not justified by this
  baseline.** Division is comparable to multiplication across the sweep, not a
  bottleneck, and it is the riskiest refactor. Re-measure after Slice 2: once
  Karatsuba makes large multiplies sub-quadratic, division may become the
  relative bottleneck and re-open the case. Until then it stays deferred.

## Consequences

**Wins:**

- The performance pass has a calibrated reference. Every candidate in ADR-0044
  attaches a delta against these numbers, and the swept widths show where a
  sub-quadratic multiply, Newton division, or binary-splitting series actually
  crosses over, so thresholds can be set from data rather than guessed.
- `DecBig` gains its first benchmark coverage, a permanent regression-watching
  fixture for the otherwise property-test-only backend.

**Costs:**

- Numbers are host-specific; reproduce on the target machine rather than
  comparing absolute values across hosts.
- The full bench run takes a few minutes; PR-time CI runs the unit and
  conformance tests, not the benches.

## Related

- Plan: the DecBig performance pass (`reactive-twirling-toucan`).
- Builds on: [ADR-0038](0038-arbitrary-precision-decimal.md) (the 1.0 gates),
  [ADR-0040](0040-arbitrary-precision-transcendentals.md) (names the perf
  levers), [ADR-0006](0006-defer-perf-pass.md) / [ADR-0007](0007-perf-baseline.md)
  (the fixed-format perf-pass discipline this mirrors).
- Successor: ADR-0044 (records the per-candidate deltas).
