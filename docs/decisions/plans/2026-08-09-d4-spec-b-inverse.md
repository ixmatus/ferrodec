# D4 spec B (binding): asinPi, acosPi, atanPi, atan2Pi — `inverse_trig_pi.rs`

Authority: ADR-0061 and the design brief. Landed seams: `exact_pi.rs`
(`asinpi_exact`, `acospi_exact`, `atanpi_exact`, `atan2pi_exact`,
`deliver_pi_exact`), `ladder::{ASINPI, ACOSPI, ATANPI, ATAN2PI}`,
`ExtNum::inv_pi` at all three rungs, the `trig-pi` feature.
Deviating without a supervisor checkpoint is a defect.

## Files this slice owns

- `ferrodec-transcend/src/inverse_trig.rs` — ONE refactor, first
  commit, byte-identity obligation: split each of the four kernel
  bodies into an extended-precision core
  (`atan_extended_core<F, E>(ex, x) -> E` and siblings, the
  pre-delivery value) plus the existing delivery tail, exactly the
  `exp.rs` `exp_prepared` precedent; regate the MODULE to
  `#[cfg(any(feature = "trig", feature = "trig-pi"))]` with the
  public kernels and their tests staying `trig`-gated inside. No
  delivered result may change: the full corpus and the trig suites
  are the proof, and the refactor commit carries them green before
  the pi kernels build on it.
- `ferrodec-transcend/src/inverse_trig_pi.rs` (new).
- The inverse four appended to the three `math/trig_pi.rs` wrapper
  files (agent A owns the file creation; append-only sections, one
  function per block — integration merges by concatenation).
- `tests/transcend_asinpi.rs`, `tests/transcend_acospi.rs`,
  `tests/transcend_atanpi.rs`, `tests/transcend_atan2pi.rs` and the
  d64/d32 mirrors (new).
- Worktree-local registrations only, as in spec A.

## Kernel structure

Each kernel: §9.2.1 specials → `exact_pi` classifier → anchor arms →
core ÷ π → `round_guarded` with its own budget. NO adjudicator.

§9.2.1 tables (transcribed; re-verify against the MPFR 4.2.2 texi
and C23 Annex F rows during implementation and cite both at the
site):

- `asinPi`: NaN rules crate-wide; `asinPi(±0) = ±0`; `|x| > 1` and
  `±∞` → NaN + `INVALID`. Classifier owns `±1 → ±1/2`.
- `acosPi`: `acosPi(±0) = 1/2` exactly (deliver `5·10^−1` through
  the rounder, `OK`); `|x| > 1`, `±∞` → NaN + `INVALID`. Classifier
  owns `+1 → +0`, `−1 → 1`.
- `atanPi`: `atanPi(±0) = ±0`; `atanPi(±∞) = ±1/2` exactly, `OK`.
  Classifier owns `±1 → ±1/4`.
- `atan2Pi`: transcribe `atan2`'s §9.2.1 table from
  `inverse_trig.rs` with every result scaled by `1/π` — the scaled
  results are all exact multiples of `1/4` and `1/2`, delivered
  exactly with clean flags (state that the scaling turns atan2's
  inexact `±π`-family rows into EXACT rows here, a real behavioral
  difference to test). Classifier owns the finite diagonals.

Anchor arms (ADR-0061's closed list; derive each margin table at the
site, ×10 discipline):

- `atanPi`, large magnitude: for `adj(x) ≥ P + 2`, the value hugs
  `±1/2` from inside by `≤ 1.01/(π|x|) ≤ 3.3·10^(−P−3)` (margin
  ≥ ×150 inside `5·10^(−P−1)`); residual at the `1/2` anchor,
  `magnitude_grows = false`, sign applied after (side theorem
  `|atanPi| < 1/2` strictly for finite `x`).
- `acosPi`, tiny magnitude: for `adj(x) ≤ −(P + 3)`, the value hugs
  `1/2` by `≤ 1.01·|x|/π` on the side opposite `x`'s sign
  (`acosPi(δ) = 1/2 − δ/π + O(δ³)`): residual at `1/2`,
  `magnitude_grows = (x < 0)`.
- `atan2Pi`: two hug families per the brief — `|y/x|` huge → `±1/2`
  (the `atanPi` arm, quadrant-signed) and `|y/x|` tiny with `x < 0`
  → `±1` (`atan2Pi ≈ ±(1 − |y/(πx)|)`, hugging from inside,
  `magnitude_grows = false` at the 1 anchor). Gate both on the
  adjusted-exponent gap `adj(y) − adj(x)` (`≥ P + 2` resp.
  `≤ −(P + 3)`), margins derived at the site. `x > 0` with tiny
  ratio is NOT an anchor (slope `1/π`); say so where the arm is
  absent.
- `asinPi` has NO anchor arm (slope `1/π` at 0; near `±1` the
  distance to `±1/2` is `~sqrt(2δ)/π`, decades above ulp scale —
  state both facts in the kernel doc).

Cores: `asinPi = asin_extended_core(x) · inv_pi()`, likewise acos /
atan / atan2, then `round_guarded` under `ASINPI` / `ACOSPI` /
`ATANPI` / `ATAN2PI`. Sign and `eff_rm` handling mirrors the parent
kernels.

## Wrappers and tests

As spec A (dual doc aliases, Accuracy blocks, §9.2.1 row tests every
mode, exact tables with cohort variants, anchor bands across both
sides of each gate with directed-mode side assertions, and the
metamorphic rows: `asin_pi(x) + acos_pi(x) = 1/2` value-checked at
loose tolerance as a smoke property, `atan2_pi(y, x)` quadrant
symmetries). Plus the non-terminating rows: `asin_pi(±0.5)` is
INEXACT in every mode and rounds to the correctly rounded `1/6`
neighborhood (pin the 16- and 34-digit strings you compute from the
kernel and cross-check against `1/6` by hand arithmetic in the test
comment), the `acos_pi(±0.5)` twins likewise.

## Verification bar

As spec A, PLUS: the refactor commit alone must replay the full
corpus and trig suites byte-identical BEFORE any pi kernel lands
(two commits minimum: refactor, then the family). Report commands
and totals. No corpus, CHANGELOG, or README edits.
