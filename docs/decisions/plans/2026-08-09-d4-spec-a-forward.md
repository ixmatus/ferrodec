# D4 spec A (binding): sinPi, cosPi, tanPi — `sincospi.rs`

Authority: ADR-0061 and the design brief
(`2026-08-09-d4-pi-scaled-design-brief.md`). The seams this spec
builds on are landed on the branch: `exact_pi.rs` (classifiers, the
residue layer, `deliver_pi_exact`), `ladder::SINPI`/`TANPI` budgets,
the `trig-pi` feature, `ExtNum::inv_pi`. Deviating from this spec
without a supervisor checkpoint is a defect.

## Files this slice owns (no other file may be edited)

- `ferrodec-transcend/src/sincospi.rs` (new; self-contained — do NOT
  import from `sincos.rs` or `argred.rs`: the standalone gate is the
  point).
- `src/math/trig_pi.rs`, `ferrodec-decimal64/src/math/trig_pi.rs`,
  `ferrodec-decimal32/src/math/trig_pi.rs` (new; ONLY the three
  forward wrappers — agent B appends the inverse four at
  integration, so keep the file structured one function per block).
- `tests/transcend_sinpi.rs`, `tests/transcend_cospi.rs`,
  `tests/transcend_tanpi.rs` and the d64/d32 mirrors (new).
- Local (worktree-only) registrations in `lib.rs` / `math/mod.rs`
  to compile and test; the supervisor re-does registrations at
  integration, so keep them to one `mod`/`pub use` line each.

## Kernel structure (one shared body, three entries)

`sincospi_kernel_body<F, E>(ex, x, want) -> Option<(F, Status)>`
with `want ∈ {Sin, Cos, Tan}` (tan divides the two components,
sharing the reduction exactly as `sincos.rs` shares its own):

1. §9.2.1 specials, transcribed rows (NaN propagation crate-wide
   rules; sNaN → quieted + `INVALID`; `±∞` → NaN + `INVALID` for all
   three; `sinPi(±0) = ±0`, `cosPi(±0) = 1`, `tanPi(±0) = ±0`).
2. Classifier: `exact_pi::{sinpi,cospi,tanpi}_exact` on the decoded
   parts, delivered through `exact_pi::deliver_pi_exact`. This owns
   every integer, half-integer, and (tan) quarter-integer input, so
   the paths below never see one.
3. Exact reduction at working precision, every step provably exact
   (state the proof at the site): decode `x` (`from_format`, exact);
   `x/2` by `×5, exponent −1` (exact always); truncate to `m`
   (`|x| < 10^P` here, since larger magnitudes are integers the
   classifier consumed); `r = x − 2m` (exact subtract, `r ∈ (−2, 2)`);
   fold `r` by exact quarter subtractions into `δ ∈ [−1/4, 1/4]`
   plus `(branch, sign)` per the standard octant bookkeeping,
   `branch ∈ {Sin, Cos}` selecting which series computes the
   magnitude. Document why every fold step is exact decimal
   arithmetic (subtraction of exact halves/quarters of like scale).
4. `cosPi` anchor arm (ADR-0061's closed list), AFTER the classifier
   and BEFORE the ladder: when the cos branch would run with
   `adj(δ) ≤ −⌈(P + 3)/2⌉`, the true magnitude hugs 1 from below by
   `≤ (πδ)²/2 ≤ 5·10^(−P−2)`, one decade inside the first boundary
   below 1 (`5·10^(−P−1)`), so deliver
   `ex.one().to_format_with_residual::<F>(false, eff_rm)` with the
   parity sign applied after (side theorem: `cos(πδ) < 1` strictly
   for `δ ≠ 0`, and `δ = 0` is classifier territory). Derive and
   state the margin table per format (P = 34/16/7) at the site.
5. `tanPi` near-`±1` anchor arm: when the tan path's reduced
   position is a quarter-integer neighborhood with
   `adj(δ) ≤ −(P + 4)`, the value hugs `±1` linearly
   (`|value ∓ 1| ≤ 6.6·|δ| ≤ 6.6·10^(−P−3)`, margin ≥ ×75 inside
   `5·10^(−P−1)`); deliver the residual at the 1 anchor with
   `magnitude_grows = sign(δ)` composed with the quadrant, per the
   four-case table you derive and pin at the site (tan increasing;
   `tanPi(1/4 + δ) − 1 ~ +2πδ`, `tanPi(3/4 + δ) + 1 ~ +2πδ`).
6. Series: own Taylor loops for `sin(πδ)`/`cos(πδ)` at
   `|πδ| ≤ π/4`, caps from `ex.sin_cos_series_terms()` (the caps
   are valid: same convergence class as `sincos.rs`). `πδ` via
   `ex.pi()` multiply. Tan: quotient with `div::<F>`.
7. Delivery: `ladder::round_guarded::<F, E>(value, eff_rm, budget)`
   with `&ladder::SINPI` (sin/cos) or `&ladder::TANPI`; signs and
   `eff_rm` reflection per the standard negation rule (`for_negation`
   when the applied sign is negative). NO adjudicator (ADR-0061:
   route closed). `tanPi` pole neighborhoods take the plain ladder
   and CANNOT overflow — state the `δ ≥ 10^(adj − P + 1)` cap proof
   at the site instead of writing a gate.

## Wrappers (each of the three format crates)

`sin_pi`, `cos_pi`, `tan_pi` on the format type, `#[must_use]`,
`#[doc(alias = "sinPi")]` AND `#[doc(alias = "sinpi")]` (both
spellings), rustdoc with: the §9.2.1 rows, the exact tables
(integers/half/quarter), an Accuracy block stating the ADR-0059 tier
with ADR-0061's no-reduction-caveat language and the no-ties fact,
and the revolutions framing (`sin_pi(0.5) == 1` exactly). Mirror the
D3 wrapper prose discipline (`src/math/hypot.rs` is the model).

## Tests (per op, per format; the D3 files are the pattern)

- §9.2.1 rows, every rounding mode.
- The exact tables: integers (both parities, both signs, huge
  magnitudes where quantum ≥ 1), half integers, quarter integers
  (tan), including cohort variants (`2.50` vs `2.5`).
- The tanPi pole table: sign alternation and `DIV_BY_ZERO`, plus
  the no-overflow fact at the closest representable neighbors of a
  pole.
- Anchor-arm bands: `cosPi(n + 10^-k)` and `tanPi(1/4 + 10^-k)`
  across the gate boundary (both sides), directed modes asserting
  the side theorems.
- Small-argument sanity: `sin_pi(tiny) ≈ π·tiny` INEXACT in every
  mode (slope π: assert it does NOT equal the input, the non-anchor
  fact).
- Parity/oddness metamorphic checks: `sin_pi(−x) = −sin_pi(x)`
  bitwise, `cos_pi(−x) = cos_pi(x)`, `tan_pi(x + 1) = tan_pi(x)`
  where both representable.

## Verification bar (before reporting done)

`cargo test -p ferrodec-transcend --features trig-pi`, all three
format crates `--features trig-pi` AND `--features transcendentals`,
`cargo fmt`, workspace clippy `-D warnings`, rustdoc `-D warnings`.
Report the exact commands and totals. Do not touch corpus files,
CHANGELOGs, README, or any file outside the inventory above.
