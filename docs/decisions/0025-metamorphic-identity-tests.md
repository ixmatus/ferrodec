# ADR-0025: Metamorphic identity tests with condition-number-derived bounds

- **Status**: accepted
- **Date**: 2026-05-17

## Context

The transcendental surface is verified against an astro-float oracle
under the faithful-rounding contract (ADR-0021, ADR-0024). The oracle has
a sound magnitude domain: past a bounded argument magnitude a
fixed-precision astro-float reference loses the digits needed to bracket
the format result, so the property suites skip out of domain (fd-3cd,
fd-dfs; the `coef.ilog10()+exp > 15` guard in `property_sincos.rs`, the
magnitude-scaled precision in `property_sincos_large.rs`). In those
skipped regions exp, ln, hyperbolic, and inverse-trig have no correctness
backstop at all. Metamorphic identities (algebraic relations that hold
for the exact functions regardless of magnitude) are the natural backstop
because they need no oracle.

Two design hazards had to be resolved before writing the suite.

**Hazard 1: tautological identities.** The shared kernel deliberately
derives functions from one another. An identity whose two sides both
route through the same kernel helper cannot fail when that helper is
wrong: the error cancels. A kernel audit (ferrodec-transcend/src) found:

- `log10_kernel` / `log2_kernel` are literally `ln(x) · const`, so
  `log_b(x)·ln(b) ≈ ln(x)` is an identity on `ln` against itself.
- `tanh_kernel` is `sinh_ext / cosh_ext`, so `tanh ≈ sinh/cosh` is
  trivially true by construction.
- `exp2_kernel` is `exp(x·ln2)` via `exp_from_extended`, and the general
  `pow(2,x)` path is the same `exp_from_extended(x·ln2)`, so
  `exp2 == pow(2,x)` compares a computation with itself.
- `asinh_kernel` / `atanh_kernel` implement exactly their ln-forms, so a
  format-level reconstruction of the same formula is the same algorithm,
  not an independent oracle. `acosh_kernel` near `x = 1` uses a distinct
  `log1p` path, so the naive `ln(x + sqrt(x²−1))` reconstruction *is*
  independent there.

**Hazard 2: a flat ULP budget is unsound.** An earlier draft bounded an
N-operation identity by `N + 2` ULP. That holds only for well-conditioned
compositions. `exp(ln(x))` round-trips through `ln` (faithful to one ULP
of `ln(x)`, an absolute error `≈ |ln x|·10⁻³⁴`) and `exp` (which turns
absolute argument error into one-for-one relative output error), so the
round-trip relative error is `≈ |ln x|·10⁻³⁴`: at `x = 1e300` that is
`≈ 700` ULP, not 4. Catastrophic-cancellation identities
(`cosh²−sinh²=1` at non-small `|x|`) are worse. A flat budget either
fails spuriously or, widened to pass, proves nothing.

## Decision

Ship a metamorphic identity suite (`tests/property_metamorphic.rs` in
each of the three decimal crates) under three categories, with the
tautological identities removed and per-identity bounds derived from the
analytic condition number.

**Shared mechanism.** `ferrodec_test_support::transcend_oracle::within_n_ulp_band`
checks `|got − want| ≤ n_ulps · ulp(want)`, where `ulp(want)` is the
larger of the two adjacent representable gaps (conservative across a
power-of-ten cohort boundary) and `want` is the exact representable
right-hand side (so the comparison carries no oracle noise; the
`cmp_approx` dead-band is deliberately not on this path). It is O(1) in
`n_ulps`: the gap is computed once and scaled, never walked, because a
condition-amplified `n_ulps` can reach `~10⁵`. The caller derives
`n_ulps` per identity from the condition factor evaluated in-format at
the test point; a constant `C = 4` absorbs higher-order terms and the
identity's own residual rounding.

**Category A — independent cross-computation, well-conditioned, tight
band (`n_ulps = 4`).** Two mutually independent kernels computing the
same magnitude. High signal, real teeth at any magnitude including the
oracle skip regions.

- `pow(x,2) == x*x`: `pow` (= `exp(2·ln x)`) vs the BID multiplier.
- `pow(x,0.5) == sqrt(x)`: `pow` vs Newton `sqrt`.
- `ln(exp(x)) ≈ x` for `exp(x)` finite: `exp` then `ln`; well-conditioned
  (the `ln∘exp` direction contracts error, unlike `exp∘ln`).
- `atan2(sin x, cos x) ≈ x` on `(−π, π]`: the sincos kernel vs the
  independent atan2 kernel.

**Category B — independent inverse round-trip, condition-amplified,
derived bound.** The kernels are independent (not tautological); the
bound is the analytic condition number times `C`.

- `exp(ln(x)) ≈ x`, `x > 0`: `n_ulps = ceil(C·(|ln x| + 1))`. The primary
  large-magnitude backstop.
- `pow(2, log2 x) ≈ x` and `pow(10, log10 x) ≈ x`, `x > 0`:
  `n_ulps = ceil(C·(|ln x| + 1))`. Replaces the dropped
  `log_b·ln(b)≈ln` tautology; `log_b` routes through `ln`, `pow` through
  the independent `exp`, so the round-trip is genuine.
- `asin(sin x) ≈ x` on `[−π/2, π/2]`: `n_ulps = ceil(C·(|tan x|/|x| + 1))`.
- `acos(cos x) ≈ x` on `(0, π)`: `n_ulps = ceil(C·(|cot x|/|x| + 1))`.
- `atan(tan x) ≈ x` on `(−π/2, π/2)` away from the poles:
  `n_ulps = ceil(C·(|sin x·cos x|/|x| + 1))`, `≤ ~2` (well-conditioned).

  The condition number is expressed in **ULP-of-x units**, which carries
  a `1/|x|`-type magnitude-ratio term a naive `1 + |cot x|` misses: an
  intermediate at magnitude `O(1)` (`cos x`) has absolute error
  `≈ |cos x|·u`, but one ULP *of x* is `|x|·u`, so for small `x` the
  error is many ULP of `x`. A first draft that omitted this underbanded
  `acos(cos 0.05)` (true condition `≈ 400`, not `≈ 20`); the corrected
  form is what ships.
- `acosh(x) ≈ ln(x + sqrt(x²−1))` for `x ≥ 1 + δ`: kernel uses the
  independent `log1p` path; `n_ulps = ceil(C·(1 + 1/sqrt(x−1)))`.

**Category C — cancellation, weak, small `|x|` only, documented as
weak.** `cosh²(x) − sinh²(x) ≈ 1` for `|x| ≤ 1`. Shares the `exp` kernel,
so this is a sanity check on `eˣ·e⁻ˣ` consistency, not an independent
oracle; bounded small and labelled weak so it is not mistaken for a
strong claim.

**Dropped as tautological (recorded so they are not re-added):**
`log2·ln2≈ln`, `log10·ln10≈ln`, `tanh≈sinh/cosh`, `exp2==pow(2,x)`,
`asinh≈ln-form`, `atanh≈ln-form`.

Sweeps reach the oracle skip regions deliberately: each suite uses
table-driven probes at hand-chosen decade points appropriate to the
format's exponent range (decimal32 `±90`, decimal64 `±369`, Decimal128
`±6144`), plus a magnitude-biased proptest. Each probe documents the
oracle-skip predicate it penetrates.

## Consequences

- The skip regions gain a backstop. Category A keeps full teeth at any
  magnitude; category B keeps teeth scaled honestly by the conditioning;
  category C is explicitly weak.
- The identity set is materially smaller and sharper than a naive
  catalogue. Removing the tautological identities removes checks that
  would have looked like coverage while proving nothing, which is the
  anti-frugal failure this ADR exists to prevent.
- The derived bounds cost a per-identity condition analysis (recorded
  above and in the suite's module docs) rather than a single constant.
  That analysis is the deliverable: it is what a future maintainer needs
  to extend or tighten the suite without rediscovering the conditioning.
- The bound is not correctly-rounded-tight, so a sub-condition-number
  systematic bias can still hide. Metamorphic tests corroborate the
  faithful oracle in the regions it cannot reach; they do not replace it
  where it can.

## Related

- Plan: `plans/2026-05-17-testing-surface-extension.md`
- Other ADRs: builds on ADR-0021 (faithful contract, exact oracle) and
  ADR-0024 (shared Extended kernel); does not supersede either.
