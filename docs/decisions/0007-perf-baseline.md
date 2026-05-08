# ADR-0007: Performance baseline (1.10.1 + bench expansion)

- **Status**: accepted
- **Date**: 2026-05-06

## Context

ADR-0006 deferred wholesale perf optimization until profile data existed. The expanded bench suite landed in commit `18bd5f7` (alignment-heavy, full-precision, magnitude-extreme variants in `core_ops.rs`; `compare`/`total_cmp`/`compare_total_magnitude` in a new `comparison.rs`; integer-conversion shapes in `conversions.rs`). This ADR captures the baseline numbers from that suite at commit `18bd5f7`, ahead of the perf-pass optimization candidates landing in subsequent commits.

The baseline supersedes ADR-0006's "wait for profile data" stance: this *is* the profile data.

## Decision

Adopt the bench-suite numbers below as the baseline. Each subsequent perf-pass commit (Phase 3a–3f in `docs/decisions/plans/2026-05-06-perf-pass.md`) reports its delta against these numbers in the corresponding optimization ADR. Commits that don't produce a measurable delta in the targeted bench get reverted.

## Baseline numbers

**Host**:

- macOS 26.4.1 (build 25E253), Apple Silicon (arm64).
- rustc 1.95.0 stable (59807616e 2026-04-14).
- ferrodec commit `18bd5f7` (release profile: thin LTO, 1 codegen unit, opt-level=3).

**Method**: `cargo bench --bench <name>` per file. Numbers below are the criterion median (middle of the [low, mid, high] range it reports). The 25-75 IQR is generally <1 % of the median; outliers noted where present.

### `core_ops` (six original + four new shapes)

| Bench                       | Calls / iter | Median time / iter | ≈ time / call |
|-----------------------------|-------------:|-------------------:|--------------:|
| `add`                       |            6 |             7.98 µs |       1.33 µs |
| `sub`                       |           36 |            39.92 µs |       1.11 µs |
| `mul`                       |           36 |            42.45 µs |       1.18 µs |
| `div`                       |           36 |            50.69 µs |       1.41 µs |
| `sqrt`                      |            5 |            20.83 µs |       4.16 µs |
| `fma`                       |          216 |              488 µs |       2.26 µs |
| `add_alignment_heavy`       |            6 |             6.08 µs |       1.01 µs |
| `sub_alignment_heavy`       |            6 |             7.98 µs |       1.33 µs |
| `mul_full_precision`        |            3 |             6.60 µs |       2.20 µs |
| `div_magnitude_extreme`     |            4 |            66.07 µs |       16.5 µs |

### `comparison` (new file)

| Bench                                   | Calls / iter | Median time / iter |
|-----------------------------------------|-------------:|-------------------:|
| `partial_cmp_pairwise`                  |        144 |             617 ns |
| `total_cmp_pairwise`                    |        144 |             855 ns |
| `compare_total_magnitude_pairwise`      |        144 |             976 ns |
| `total_cmp_sort_64`                     |   ~6 × 64 = 384 |          3.30 µs |

### `conversions` (six existing + new integer shapes)

| Bench         | Calls / iter | Median time / iter | ≈ time / call |
|---------------|-------------:|-------------------:|--------------:|
| `parse_str`   |            6 |             3.68 µs |       613 ns |
| `format`      |            5 |             623 ns |       125 ns |
| `from_i128`   |            6 |             2.98 µs |       497 ns |
| `from_i32`    |            5 |             735 ps |       147 ps |
| `from_u32`    |            5 |             736 ps |       147 ps |
| `from_u64`    |            5 |             736 ps |       147 ps |
| `to_i64`      |            5 |              44 ns |       8.8 ns |
| `to_i32`      |            5 |              20 ns |       4.0 ns |
| `to_u64`      |            5 |              20 ns |       4.0 ns |
| `to_u128`     |            5 |              24 ns |       4.8 ns |

The `from_i32` / `from_u32` / `from_u64` numbers — 147 ps per call — are essentially "compiler eliminated the call" territory. These conversions are `pub const fn` and the bench inputs are constant, so LLVM constant-folds them at the iteration boundary. They're listed for completeness; the perf pass shouldn't expect to move them.

### `transcendentals` (existing benches, captured separately)

Captured at the same commit; included here for ADR completeness. See `/tmp/bench-baseline-transcendentals.txt` (or re-run the bench) for the full numbers. Transcendentals aren't in the perf-pass scope — the candidates target the arithmetic kernels and rounding pipeline, both of which the transcendentals sit on top of, so any uplift there will reflect indirectly. Re-bench at Phase 4 confirms no regression.

## Observations the baseline highlights

A few patterns worth noting before optimization:

- **`div_magnitude_extreme` at 16.5 µs/call** is ~12× the median `div` cost. The long-division path on widely-separated magnitudes is the clear outlier; candidate 3c (early exit in `div_rem_u128`) targets exactly this.
- **`add_alignment_heavy` is *faster* than `add`**: the bench is 6 calls vs `add`'s 6-call inner loop, similar per-call work. The "alignment-heavy" pairs hit the sub-ULP path which is structurally simpler than full-precision alignment — but candidate 3a (`mul_pow10` table) should still help by removing the iterative `mul10`-loop on shifts that *do* pad coefficients.
- **`mul_full_precision` at 2.20 µs/call** is ~2× median `mul` — full-precision pairs use the entire U256 product width, whereas the average mul mix sees small-coefficient short-circuits.
- **`fma` at 488 µs/iter (216 calls)** averages 2.26 µs/call — closer to mul than to add+mul. The U384 alignment buffer + single-rounding pipeline pays off.

## Consequences

**Wins:**

- Future perf claims have a calibrated reference. Every optimization-candidate commit attaches a delta; ADR-0008 will record the cumulative result.
- The README's Performance section can be regenerated from this data after the perf pass lands, replacing the 2024-vintage 1.3.0 numbers.
- Coverage gaps catalogued during planning (alignment-heavy, full-precision, comparison, integer conversions) are now closed; future regression-watching has wider visibility.

**Costs:**

- Bench runs take ~15 minutes on the recorded host across all four bench files (criterion's default 100-sample × 5s warmup × 5s measurement). Acceptable for the perf-pass cycle but slow for casual re-runs; PR-time CI runs the unit tests, not the benches.
- Numbers are host-specific. Anyone reproducing on a different machine should generate a fresh baseline rather than compare directly to these absolute values.

## Related

- Plan: [`plans/2026-05-06-perf-pass.md`](plans/2026-05-06-perf-pass.md).
- Supersedes: [ADR-0006](0006-defer-perf-pass.md).
- Successor: ADR-0008 (will record the post-pass deltas).
- Commit: `18bd5f7` (bench expansion).
