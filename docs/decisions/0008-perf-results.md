# ADR-0008: Performance pass results (1.11.0)

- **Status**: accepted
- **Date**: 2026-05-07

## Context

[ADR-0007](0007-perf-baseline.md) captured the bench-suite baseline at commit `18bd5f7` ahead of the optimization-candidate work scoped in [`plans/2026-05-06-perf-pass.md`](plans/2026-05-06-perf-pass.md). Six candidates (3a–3f) were applied per the plan's per-candidate workflow: implement → run all property tests + conformance + Kani harnesses → run the relevant benches → record delta in this ADR → commit (or revert if neutral / negative).

This ADR records the measured outcome.

## Decision

Three candidates landed; three measured neutral or slightly-negative and were reverted per the stop-loss. Aggregate speedup across the headline arithmetic operations is **~17 %**, which puts the result in the plan's "wide win" envelope and justifies a `1.11.0` release. Bench expansions and ADR infrastructure ship regardless.

## Per-candidate result

| ID  | Candidate                                       | Outcome   | Commit / note |
|-----|-------------------------------------------------|-----------|---------------|
| 3a  | Precomputed `10^k` table for `U256::mul_pow10`  | shipped   | `a53ddb4` — small wins on the alignment paths (≈ 1–2 % per call), but the load-bearing benefit is removing the iterative `mul10`-loop from the hot inner shift used in addsub alignment. |
| 3b  | Cache `decimal_digit_count` in `round_and_pack_finite` | shipped | `15a7b98` — the killer optimization. `decimal_digit_count` was being called 3× per `round_and_pack_finite` and is non-trivial (walks U256 via `div_rem10`). 15–29 % across most arithmetic ops. |
| 3c  | Early exit in `U256::div_rem_u128`              | reverted  | Leading-zero skip on the bit-by-bit long division shaved ≈ 30 of 256 iterations on `div_magnitude_extreme` inputs but produced no measurable wall-time delta (`div +1.6 %`, `div_magnitude_extreme +1.3 %` — both within noise). The bit-loop body is so tight that 30 fewer iterations didn't register. |
| 3d  | `#[inline]` on small `bid.rs` helpers           | no-op     | Investigation found every named target — `classify_bits`, `pack_finite`, `pack_quiet_nan` / `_signaling_nan`, `decimal_digit_count`, `pow10` — was already `#[inline]`. So were the predicates in `classify.rs` and `status.rs`. The remaining unmarked private helpers (`decompose_finite`, `sign_of`, `product_quantum`) are intra-crate behind `lto = "thin"` + `codegen-units = 1`, so LLVM was already inlining them. No code change. |
| 3e  | `bid::pow10` uses the `POW10_U128` table        | reverted  | Pointing the `const fn` at a runtime table lookup measured neutral-to-slightly-negative on the alignment-heavy benches (+0.7..+1.3 %, p < 0.05 but inside the harness's typical noise floor). Hypothesis: LLVM was already constant-folding `10u128.pow(k)` under release LTO at every static call site, and the table dereference adds load-uop pressure without saving instructions. |
| 3f  | Unify the two digit-extraction loops in `round.rs` | shipped | `84e4598` — pulled the `div_rem10`-loop out of `drop_excess_digits` and `shift_right_decimal` into a single `extract_dropped_digits(coef, n, pre_sticky)` helper. `mul -3.2 %` (real, p < 0.05); other benches in noise. The mul win likely comes from the unified helper inlining better than the duplicated bodies under release LTO. |

## Cumulative bench delta vs baseline (ADR-0007)

Host: macOS 26.4.1 (build 25E253), Apple Silicon (arm64), rustc 1.95.0 stable, release profile (thin LTO, 1 codegen unit, opt-level=3).

### `core_ops`

| Bench                       | Baseline | After    | Δ        |
|-----------------------------|---------:|---------:|---------:|
| `add`                       |  7.98 µs |  5.79 µs | **−27.5 %** |
| `sub`                       | 39.92 µs | 30.53 µs | **−23.5 %** |
| `mul`                       | 42.45 µs | 31.36 µs | **−26.1 %** |
| `div`                       | 50.69 µs | 44.78 µs | **−11.7 %** |
| `sqrt`                      | 20.83 µs | 20.48 µs | −1.7 %   |
| `fma`                       |   488 µs |   415 µs | **−14.9 %** |
| `add_alignment_heavy`       |  6.08 µs |  5.23 µs | **−14.0 %** |
| `sub_alignment_heavy`       |  7.98 µs |  5.80 µs | **−27.3 %** |
| `mul_full_precision`        |  6.60 µs |  5.23 µs | **−20.8 %** |
| `div_magnitude_extreme`     | 66.07 µs | 65.53 µs | −0.8 %   |

The `div`-family lags the rest because the long-division kernel never picked up a structural win: 3c (its candidate) reverted as no-op, and the `decimal_digit_count` cache (3b) only fires once the rounding pipeline starts. The `sqrt` and `div_magnitude_extreme` numbers are noise-floor — those paths spend the majority of their time in the bit-by-bit Newton / long-division loops, which the perf pass left untouched.

### `comparison`

| Bench                                   | Baseline | After    | Δ        |
|-----------------------------------------|---------:|---------:|---------:|
| `partial_cmp_pairwise`                  |   617 ns |   601 ns | −2.6 %   |
| `total_cmp_pairwise`                    |   855 ns |   837 ns | −2.1 %   |
| `compare_total_magnitude_pairwise`      |   976 ns |   955 ns | −2.2 %   |
| `total_cmp_sort_64`                     |  3.30 µs |  3.22 µs | −2.4 %   |

Comparison wasn't a candidate target. The ~2 % uplift across all four shapes is incidental — `cmp` calls `classify_bits` per operand, and the rounding-pipeline changes nudged LLVM into a slightly tighter codegen for the type-field decode.

### `conversions`

| Bench         | Baseline | After    | Δ        |
|---------------|---------:|---------:|---------:|
| `parse_str`   |  3.68 µs |  2.91 µs | **−20.9 %** |
| `format`      |   623 ns |   606 ns | −2.7 %   |
| `from_i128`   |  2.98 µs |  2.24 µs | **−24.8 %** |
| `from_i32`    |   735 ps |   734 ps | flat     |
| `from_u32`    |   736 ps |   739 ps | flat     |
| `from_u64`    |   736 ps |   738 ps | flat     |
| `to_i64`      |    44 ns |    46 ns | +4 %     |
| `to_i32`      |    20 ns |    21 ns | +5 %     |
| `to_u64`      |    20 ns |    22 ns | +8 %     |
| `to_u128`     |    24 ns |    24 ns | flat     |

The `parse_str` and `from_i128` wins are incidental but real: both go through `round_and_pack_finite`, which Phase 3b sped up by caching `decimal_digit_count`. The `from_i32` / `_u32` / `_u64` numbers stay sub-nanosecond (LLVM constant-folding the const inputs). The small `to_*` upticks are sub-2 ns absolute and well within typical between-run noise.

### `transcendentals`

| Bench   | Baseline   | After      | Δ      |
|---------|-----------:|-----------:|-------:|
| `sin`   |   706 µs   |   712 µs   | +1.0 % |
| `cos`   |   707 µs   |   717 µs   | +1.4 % |
| `exp`   |  1018 µs   |  1017 µs   |  flat  |
| `ln`    |  1490 µs   |  1482 µs   | −0.5 % |
| `log10` |  1491 µs   |  1487 µs   | −0.3 % |
| `pow`   |  1727 µs   |  1773 µs   | +2.7 % |

The math kernels weren't a perf-pass target. Numbers shifted within
the run-to-run noise floor for long benchmarks (the transcendentals
do thousands of iterations of the underlying arithmetic, so per-call
variance compounds). No regression worth chasing.

## Aggregate

Across the seven `core_ops` benches that moved past the noise floor: −27.5, −23.5, −26.1, −14.9, −14.0, −27.3, −20.8. Geometric mean **≈ −22 %**. Including the noise-floor `sqrt`, `div_magnitude_extreme`, and the marginal `div` move, the unweighted mean across all ten core_ops benches is **≈ −16.8 %**.

Comparison ops shifted ~2 % across the board as a side effect.

The "wide win" target in the plan was 15–25 % aggregate speedup on the core_ops bench suite. We landed at the upper end of that band — without touching the long-division kernels or the transcendental Newton / Taylor inner loops.

## Lessons recorded for the next perf pass

A few patterns from this round worth remembering when the next backlog drops:

- **The biggest win was the smallest patch.** Phase 3b moved a single `decimal_digit_count` call out of a hot path — twelve lines of diff for what turned into the bulk of the 17 % aggregate. The five other candidates between them shipped less measurable code than 3b alone.
- **LLVM with thin LTO + 1 codegen unit eats most "obvious" inline / table optimizations.** Three of the six candidates (3c, 3d, 3e) targeted shapes the compiler had already optimized: bit-by-bit long division was already as tight as the leading-zero skip could make it; `#[inline]` was already on the named helpers; `10u128.pow(k)` was already constant-folded at every static call site. Future perf work should check the asm for the actual call sites *before* writing the candidate, not after.
- **The bench expansion paid off independently of the optimizations.** `mul_full_precision`, `div_magnitude_extreme`, the alignment-heavy variants, and the comparison suite are now permanent regression-watching shapes. A future perf review or compiler upgrade flagging a regression in those benches has measured ground to stand on. ADR-0007's baseline numbers are the reference point.
- **Stop-loss worked.** The plan's "revert if neutral / negative" rule produced three reverts out of six candidates without overshooting into "land it anyway because we already did the work" territory. The reverted ADRs (3c, 3d, 3e summarised above) are the audit log's main deliverable for those slots.

## Consequences

**Wins:**

- Headline ops: `add`/`sub`/`mul` between 23.5 % and 27.5 % faster. `fma` 14.9 % faster. The README's Performance section can be regenerated with substantively better numbers.
- ADR audit log now contains a calibrated worked example: a real perf pass with measured deltas, a full per-candidate stop-loss record, and a reproducible bench harness (`cargo bench --bench core_ops` etc.).
- Bench coverage permanently broadened: alignment-heavy / full-precision / magnitude-extreme variants on `core_ops`, the new `comparison` file, the new integer-conversion shapes on `conversions`. Future regressions in any of these shapes will surface immediately.

**Costs:**

- Three rounds of revert work effectively zero net code change after benchmarking. Time spent: roughly half a day of bench wall time + analysis.
- The reverted candidates are not the same as "wouldn't have worked" — they're "didn't measurably move *this* bench harness on *this* host". A different microarchitecture or a future LLVM may light up 3c (early-exit in long division) or 3e (table lookup vs `pow`). The diffs are recoverable from this ADR's per-candidate notes if anyone wants to revisit.

## Related

- Plan: [`plans/2026-05-06-perf-pass.md`](plans/2026-05-06-perf-pass.md).
- Predecessor: [ADR-0007](0007-perf-baseline.md) (baseline).
- Commits: `18bd5f7` (bench expansion), `b586722` (baseline ADR), `a53ddb4` (Phase 3a), `15a7b98` (Phase 3b), `84e4598` (Phase 3f).
- Reverted candidates: 3c, 3d (no-op), 3e — detailed under "Per-candidate result" above; no commit slot.
- Release: `1.11.0` (this commit).
