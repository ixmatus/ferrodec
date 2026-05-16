# fd-oaa: Decimal128 FMA correctness defect — Phase 1 findings

> Phase 1 deliverable. Read-only triage plus a pinned reproducer and
> an empirical scope gate. No production source changed in Phase 1.
> Focused single-defect slice (user-locked scope), not a six-agent
> review; the defect family is already mapped by ADR-0018 (decimal64)
> and ADR-0019 (decimal32).

## Origin

`fd-oaa` surfaced during the decimal32 correctness slice: a
`cargo test --workspace` run randomly persisted a shrunk
`tests/property_fma_oracle.proptest-regressions` counterexample. It
was scoped out of decimal32 (the parent `ferrodec` crate was
untouched; the seed was absent on `a6845a3`) and filed for its own
triage. The shrunk operands, decoded from the Decimal128 bit
patterns:

* `a` = biased_exp 6209, coef 1            → `1 × 10^33`
* `b` = biased_exp 6176, coef 1            → `1`
* `c` = biased_exp 6161, coef 3000000000000000 → `3.0`

## The question (user, Phase 1)

Genuine Decimal128 FMA kernel bug at large biased exponents, or the
astro-float ULP envelope too tight in that regime. Decide with
evidence before proposing a fix.

## Finding: GENUINE KERNEL BUG (envelope is not at fault)

Exact value: `a × b + c = 10^33 + 3 =
1000000000000000000000000000000003`. That is exactly 34 significant
digits; Decimal128 precision is 34. The correctly rounded `fma`
result is that value with **no rounding** and status `OK` (no
`INEXACT`).

The released kernel returns `1000000000000000000000000000000000`
(the `+ 3` dropped) **and** raises a spurious `INEXACT`. Error is 3
ULP on an exactly representable result plus a false flag. No
tolerance argument rescues that, so the 2-ULP property envelope in
`tests/property_fma_oracle.rs` is **not** the cause. Confirmed by
the deterministic reproducer `tests/regression_fd_oaa.rs`
(`fd_oaa_fma_shrunk_cohort`): got
`Decimal128 { biased_exp 6176, coefficient
1000000000000000000000000000000000 }`, want `…003`.

## Root cause

`src/ops/fma.rs:343-344` triggers the sub-ULP path on a **static**
raw-shift threshold:

```rust
let ab_too_wide = shift_ab > SHIFT_LIMIT || cab_grown_digits > 110;
let c_too_wide  = shift_c  > SHIFT_LIMIT.saturating_add(35) || cc_grown_digits > 110;
```

Trace for the reproducer: `qab = 33`, `qc = -15`, `cab = 1`,
`target = min(33,-15) = -15`, `shift_ab = 48`. `48 > SHIFT_LIMIT
(47)` makes `ab_too_wide` true and diverts into `fma_sub_ulp` →
`sub_ulp_round(cab=1, qab=33, …)`, which pads `cab` to 34 digits and
collapses `c` to a sticky bit, discarding its value.

The static `shift_ab > SHIFT_LIMIT` clause is the wrong proxy for "c
is sub-ULP". The shift is inflated only because `c`'s cohort sits at
`exp -15` (`3.000000000000000`), which drags `target = min(qab, qc)`
down. The product coefficient here is a single digit (`cab = 1`), so
the aligned exact sum is just 49 digits — far inside the U384
buffer's ~115-digit capacity. The dynamic clause `cab_grown_digits >
110` is already correct and is **not** met (`1 + 48 = 49 ≤ 110`):
the buffer can hold this case exactly. Only the vestigial static
disjunct misfires. This is the parent-crate analogue of the
static-alignment-window anti-pattern fixed in decimal64 (ADR-0018)
and decimal32 (ADR-0019).

The corrected sibling crates confirm the principled fix. The
decimal64 FMA admission rule, with its own regression test
`fma_far_exponent_with_small_product_does_not_drop_c`
(`ferrodec-decimal64/src/ops/fma.rs:524`), is exactly:
`digit_count(ab_coef) + shift_ab ≤ 38` (38 = its u128 capacity), the
dynamic grown-digit-count bound with no static-shift disjunct. The
parent's analogue is the existing `*_grown_digits > 110` U384
capacity bound; the fix is to drop the static-shift disjuncts.

## Scope gate (empirical)

`tests/regression_fd_oaa.rs`, run on `main`-equivalent code:

| probe | result | reading |
|---|---|---|
| `fd_oaa_fma_shrunk_cohort` (`c` = coef 3e15, exp -15) | **FAIL** | FMA drops `+3` |
| `fd_oaa_fma_plain_cohort` (`c` = coef 3, exp 0) | pass | cohort-triggered: `qc=0` keeps `shift_ab=33 ≤ 47` |
| `fd_oaa_scope_gate_add` (`add(1e33, 3.0)`) | pass | parent Decimal128 `add` is sound |
| `fd_oaa_scope_gate_mul_then_add` | pass | parent `mul`/`add` sound |

**Scope is FMA-only.** The parent Decimal128 add/mul paths are not
independently broken; no `src/ops/addsub.rs` commit is needed. The
defect is cohort-triggered: a `c` written at a deeper quantum drags
`target` down and inflates `shift_ab` past the static limit, while a
minimal-cohort `c` stays under it.

## Work order (Phase 2..N)

1. **Phase 2 (fix).** In `src/ops/fma.rs`, drop the static
   `shift_ab > SHIFT_LIMIT` / `shift_c > SHIFT_LIMIT + 35`
   disjuncts; the sub-ULP trigger becomes the dynamic U384-capacity
   bound (`cab_grown_digits > 110` / `cc_grown_digits > 110`) alone,
   mirroring the corrected decimal64/decimal32 `digit_count + shift ≤
   capacity` admission. Remove the now-unused `SHIFT_LIMIT` const and
   re-derive the `fma_sub_ulp` helper precondition comments in terms
   of the capacity bound rather than the deleted constant. Un-ignore
   `fd_oaa_fma_shrunk_cohort`.
2. **Phase 3 (breadth).** Boundary probes around the old
   `SHIFT_LIMIT`, narrow-coefficient and far-quantum-zero `c`
   (mirror decimal64 `fma_zero_c_at_far_exponent_does_not_drop_product`),
   effective-subtract and overflow sub-ULP paths unregressed.
   `property_fma_oracle` must go green (no longer a tolerated
   exception).
3. **Phase 4 (release).** New ADR; `ferrodec` 1.15.0 → 1.15.1;
   CHANGELOG `[1.15.1]`; KNOWN_ISSUES accounting; archive plan.
4. **fd-fq6.** Drop redundant `dep:libm` from
   `ferrodec-decimal32`'s `num-traits` feature (final separate
   commit, decimal32 analogue of decimal64 fd-17).
