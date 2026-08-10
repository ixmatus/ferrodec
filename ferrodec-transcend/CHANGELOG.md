# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The pi-scaled trigonometric machinery (ADR-0061, fd-4zo.26) under
  a standalone `trig-pi` feature: `exact_pi.rs` (the Niven residue
  classifier over decoded coefficient and exponent, the `PiExact`
  disposition type, and `deliver_pi_exact`, all pure decision code
  shared by every format), `sincospi.rs` (the forward kernel: exact
  decimal `x mod 2` reduction with a provably exact quarter fold,
  own Taylor loops at `|πδ| ≤ π/4`, the `cosPi` 1-anchor and `tanPi`
  ±1-anchor residual arms, delivery through `round_guarded`), and
  `inverse_trig_pi.rs` (the inverse four as extended cores times
  `ExtNum::inv_pi`, with the `atanPi`/`acosPi`/`atan2Pi` anchor arms
  from ADR-0061's closed list). `ladder.rs` gains the six budget
  entries (`SINPI`, `TANPI`, `ASINPI`, `ACOSPI`, `ATANPI`,
  `ATAN2PI`) and `consts.rs`/`bigconst.rs` gain `1/π` at all three
  rungs. The pi family takes no adjudicator by design: ADR-0061
  closes that route (no bounded-degree minimal polynomial exists at
  format denominators) and records the no-ties theorem that makes
  one unnecessary.

### Changed

- `inverse_trig.rs`'s four kernels are each split into an
  extended-precision core returning the pre-delivery value plus the
  existing delivery tail (the `exp_prepared` precedent), so the pi
  variants can reuse the cores; delivered results are byte
  identical (full corpus and trig suites as the proof). The module
  gate widens from `trig` to any transcendental trig feature, with
  the radian kernels still `trig`-gated inside.

### Added

- The ADR-0060 exact integer adjudicator (fd-jxk): the rung 2
  escalation predicate returns the candidate boundary's identity
  (`candidate_boundary`, the bool predicate its adapter), and the
  five algebraic kernels (`rsqrt`, `hypot`, `powi`'s powering arm,
  `rootn`, `compound`) deliver through `ladder::round_adjudicated`,
  which on a rung 2 ambiguity decides the true value's side of the
  one candidate boundary in exact integer arithmetic
  (`adjudicate::<op>_side`, widths up to `U1024`) and delivers
  through the ADR-0051 residual channel anchored at the boundary.
  Adjudication is rung 2 semantics in every build; `ladder_audit`
  for these operations panics only when the adjudicator declined.
  The `force_adjudicate` battery lane (with `force_escalate`) routes
  the whole corpus through the adjudicator with the pins as the byte
  identity reference.

### Fixed

- `exact::pack_value` carries each operation's §9.2.2 preferred
  exponent instead of a hardcoded 0 (fd-5g6): per-op helpers beside
  `compound`'s precedent compute `floor(y × Q(x))` for `pow`/`powr`,
  `floor(n × Q(x))` for `pown`, `−floor(Q(x)/2)` for `rSqrt`, and
  `floor(Q(x)/n)` for `rootn` and `cbrt`, all on the stored quantum.
  Cohort only; values and flags unchanged.

### Added

- The §9.2 algebraic group (ADR-0059 Track D D3, under ADR-0060's
  phase gate): `powi_kernel` (pown), `powr_kernel`, `rootn_kernel`,
  `compound_kernel`, `rsqrt_kernel`, and `hypot_kernel` on the
  generic `ExtNum` bodies. Kernel architecture follows the ADR's
  Liouville floors: `rsqrt` is a direct Newton composition with one
  division-free polish step (the `exp(−½·ln x)` route's budget cannot
  clear the proven `4.9e-105` floor), `powi` carries a working
  precision binary powering arm for `|n| ≤ 6`, `rootn` delegates
  `n ∈ {1, −1, 2, −2}` to the identity, division, the format's
  square root, and the `rsqrt` kernel, and `compound` builds its base
  through `logp1_extended_core` so `1 ⊕ x` never loses digits at the
  destination width. Input-side classifiers: `powi_exact_input` (the
  Lauter–Lefèvre criterion collapsed at `b = 1`, whole-range
  power-of-ten family included), `rootn_exact_input` (the criterion
  at `y = 1/n`; positive orders provably tie-free),
  `rsqrt_exact_parts` (terminating reciprocal criterion; the `5^d`
  midpoint families), `compound_exact_input` (exact rational
  `(1 + x)^n`, the nines whole-range family, §9.2.2 preferred
  quantum on exact deliveries), `hypot_exact_or_tie` (aligned
  `S = A² + B²` perfect-square test inside `U256::isqrt`'s envelope
  after stripping), and `compound_huge_x_anchor` (the second
  whole-range on-grid family, a live directed-mode misround repaired
  before first release). Anchor arms: `rootn`'s and `compound`'s
  hug-at-1 residual deliveries and `hypot`'s anchor band at
  `δ₀ = ⌈(P+2)/2⌉`. Budgets `POWI_INT`, `POWI`, `POWR`, `ROOTN`,
  `COMPOUND`, `RSQRT`, `HYPOT`; catalog arrays at 34. `powr` carries
  the ADR-0060 negative-result tier honesty paragraph (its claim
  cannot be upgraded by minimal-polynomial bounds).

### Fixed

- `pow_exact_input` handed the format rounder exponents up to
  `i32::MAX`, wrapping the rounder's own `i32` arithmetic
  (`pow(10, 2147483647)`: a debug panic, a wrong §7.4 disposition in
  release). The classifier now declines past `EXACT_EXPONENT_WINDOW`
  (99,999), where the value is provably past every format's `exp`
  gates and the saturation proxy answers it (fd-clc, found by the D3
  powi lane).

### Added

- The §9.2 `expm1` family and `exp10` (ADR-0059 Track D D2):
  `expm1_kernel`, `exp2m1_kernel`, `exp10_kernel`, `exp10m1_kernel`
  on the generic bodies, sharing `expm1_special_cases`
  (`f(−∞) = −1` exact), `expm1_gates` (overflow proxy; the `−1`
  band via the ADR-0051 residual channel), and `expm1_ext` (direct
  series in the reduction's k = 0 window; exp pipeline plus the
  closing subtraction outside it). Input-side classifiers:
  `exp2m1_exact_or_tie` (including the six enumerable ties, resolved
  by the format rounder's own tie rule), `exp10_integer` (every
  integer in and beyond range through one `pack_value` call), and
  `exp10m1_integer` (the all-nines proxy with total digit
  knowledge). Anchor seams: `expm1`'s x anchor (`e^x − 1 > x`) and
  the family's `−1` collapse. Budgets `EXPM1`, `EXP2M1`, `EXP10`,
  `EXP10M1`; catalog arrays at 27.

### Added

- The §9.2 `logp1` family (ADR-0059 Track D D1): `logp1_kernel`,
  `log2p1_kernel`, `log10p1_kernel` on the generic `ExtNum` bodies,
  sharing `logp1_special_cases` (§9.2.1 dispositions: sign-preserved
  zeros, the `−1` pole with `DIV_BY_ZERO`, below-domain `INVALID`)
  and `logp1_extended_core` (direct `log1p` series band below half,
  `1 ⊕ x` into the `ln` core at or above it). Input-side classifiers
  `log2p1_exact` and `log10p1_exact` with completeness proofs at
  every bail site; `logp1`'s anchor seam on `ln(1+x) < x`;
  `log10p1_power_of_ten_exponent` delivering the integer-anchor
  family (`x = 10^n`, `n ≥ 36`) through the ADR-0051 residual
  channel (past the rung width the wide band's `1 ⊕ x` absorption
  lands the working value exactly on the grid point `n`, which no
  fixed rung can move off — found by the D1 review's `ladder_audit`
  lane and repaired before first release). Budgets `LOGP1`,
  `LOG2P1`, `LOG10P1` (LN's shape; itemizations in `ladder.rs`).

## [0.3.0] - 2026-08-02

### Added

- The ADR-0059 two-rung escalation ladder (M8). Every kernel delivery that
  is not an exact/tie classification, an ADR-0051 anchor residual, a
  saturation proxy, or an offline-certified constant now runs the M2
  boundary predicate against a per-function error budget (rederived from
  op counts, itemized in `ladder.rs` rustdoc, padded ×10); a near-boundary
  rung 1 result re-runs the identical kernel at the 110-digit `Extended2`
  rung, whose trig reduction (`reduce_wide`) replaces rung 1's empirically
  discharged 38-digit `π/2` truncation with an analytic `< 10^-114` bound.
  Escalation is a deterministic, mode-independent function of the input.
  Two test-lane cfgs land with it: `--cfg force_escalate` routes every
  guarded delivery through rung 2 (the anti-rot byte-identity
  differential — the full root/d64/d32 suites pass under it), and
  `--cfg ladder_audit` panics on top-rung residual ambiguity (clean over
  every suite and the S1 witness corpus). Non-escalating latency cost of
  the guard: ~1.5% on trig, ~6% on the exp/ln family (criterion, vs the
  pre-M8 baseline).

- The `unbounded-ladder` feature (ADR-0059 M8b): a third, unbounded rung
  above the fixed ladder. `ExtendedDyn` is a `Copy` handle into a
  per-attempt arena (coefficients on `ferrodec-multiword`'s growable
  `DecBig`; the receiver carries the arena and the precision through the
  M8b exemplar seam), mirroring `extended2.rs` clause for clause at a
  runtime width. Its constants come from `ferrodec-multiword`'s
  `bigconst` generators at call time and its trig reduction
  (`argred::reduce_dyn`) computes the `2/π` window at depth `q + p + 70`
  per call, because a stored table caps the precision a rung can reach.
  The Ziv driver (`ladder::run3`) doubles the working precision from 220
  digits until the boundary predicate clears at that width's
  `budget.dynamic(p)` — the fixed catalog's itemizations re-evaluated at
  `p`, pinned within a factor of five of the rung 2 constants at
  `p = 110`. With the feature on, rung 2 escalates on its own budget
  instead of delivering unconditionally: such builds have no exception
  set (`ladder_audit` is vacuous by construction there); the crate doc
  states the final three-tier claim and ADR-0059 §Outcome records the
  measured costs. Pulls in
  `ferrodec-multiword/alloc`; off by default; default, no-alloc, and
  thumbv6m builds are unchanged. A third test-lane cfg lands with it:
  `--cfg force_rung3` routes every guarded delivery through the dynamic
  rung (full root/d64/d32 suites and the S1 witness replay pass under
  it, byte-identical to the pinned expectations).

### Fixed

- `sinh` / `cosh` saturation escalation waste and the `ladder_audit`
  panic it implied (an M8 defect, surfaced by the M8b unbounded rung):
  the overflow saturation proxy fed the guarded delivery instead of the
  format rounder directly, and a proxy's one-digit coefficient sits
  exactly on a working grid point — a distance no rung can grow. Every
  saturating `sinh` / `cosh` call silently paid a full rung 2 re-run, a
  `--cfg ladder_audit` build panicked on any saturating Decimal64 /
  Decimal32 input (their overflow regions start at `|x| > ~885` and
  `~222`, squarely inside random samplers, where Decimal128's starts
  at `|x| > 14150` — the audit lane had only ever run on Decimal128,
  which is the blind spot that kept this invisible), and the unbounded
  rung turned the waste into an unbounded widening loop. The gates now
  sit in the kernel bodies and deliver the proxy directly, mirroring
  `exp`'s; format results and status are byte-identical. The audit
  lane runs on all three formats now, and the minimal failing inputs
  are committed as proptest regression seeds.

- High-decade `Decimal128` trig misrounds (ADR-0059 S1): the 1 819
  Arb-certified witness rows (sin 643, cos 570, tan 606) that falsified
  the shipped correctly-rounded claim all round correctly under the
  ladder, and replay as a pinned regression gate
  (`tests/transcend_campaign_s1.rs`). The witnesses sit inside rung 1's
  honest trig budget (the `π/2` truncation item), so the predicate
  escalates them and rung 2 resolves the side.

- `pow` exactness and ties are now decided from the inputs alone (ADR-0059
  M7), by the decimal analog of the Lauter–Lefèvre criterion: with
  `|x| = 2^α · 5^β · t` (`gcd(t, 10) = 1`) and `|y| = a/b` in lowest terms,
  `x^y` is an exact rational iff `b | α`, `b | β`, and `t = s^b` — decidable
  in bounded integer arithmetic without factoring — and then equals
  `s^a · 2^(αa/b) · 5^(βa/b)` exactly, delivered through the format rounder.
  The ADR-0047 post-hoc proof this replaces was circular and failed in
  production: `pow(4, 0.5)` at `TowardZero` / `TowardNegative` returned
  `1.999…9` with a spurious `INEXACT` instead of the exact `2` (all
  formats), and `pow(-1, y)` with `y` too wide for the rational reduction
  (e.g. `1E+40`) carried a spurious `INEXACT` in every mode. `pow`'s
  nearest-mode ties — `PRECISION + 1`-digit exact values ending in 5, e.g.
  `pow(5, 49)` / `pow(2, -49)` at 34 digits — previously misrounded at
  `NearestAway` and are now resolved by the rounder's own tie rule, under
  the negation-reflected mode for odd powers of negative bases. Exact
  results carry the input-derived cohort. Every bail to the kernel is now
  documented as provably neither exact nor a tie (classification
  completeness, the ladder's standing assumption).

- `cbrt` of a perfect cube is now decided from the input alone (ADR-0059
  M7): stripped `x = c · 10^e` is an exact cube iff `c = t³` and `3 | e`,
  and the exact root is delivered before any approximation runs — every
  rounding direction, status `OK`. The ADR-0047 post-hoc proof this
  replaces was circular (it could only recognise an exact root the kernel
  had already delivered exactly) and failed in production: `cbrt(0.027)`
  at `TowardZero` / `TowardNegative` returned `0.2999…9` with a spurious
  `INEXACT` instead of the exact `0.3`. `cbrt` provably has no nearest-mode
  ties (midpoint cubes exceed every format's width or range), so the
  kernel's unconditional `INEXACT` is correct on every remaining input.
  Exact results now carry the input-derived cohort (`cbrt(0.027)` is `0.3`,
  quantum −1) where the post-hoc era's cohort was kernel noise (`0.3000…0`
  at quantum −34 here, bare `2` for `cbrt(8)`).

- `exp2` now resolves nearest-mode ties exactly (ADR-0059 M7). An integer
  input `n` whose `2^n` is expressible in at most `PRECISION + 1` digits is
  delivered from the exact coefficient through the format rounder instead of
  the approximation kernel, whose error lands on an arbitrary side of a true
  value that is itself a rounding boundary (`5^n` ends in 5, so a
  `PRECISION + 1`-digit `5^n` makes `exp2(-n)` an exact midpoint). Changed
  values, all at ties: `exp2(-49)` at `NearestAway` and `exp2(-50)` at
  `NearestEven` for a 34-digit format; `exp2(-23)` and `exp2(-24)` at
  `NearestAway` for 16 digits; `exp2(-11)` at `NearestAway` for 7 digits.
  Every other mode and input is unchanged (the non-tie `PRECISION + 1` cases
  were already correct and are now pinned).

## [0.2.0] - 2026-07-03

### Changed

- **Breaking:** `DecimalFormat::to_extended_parts` now returns
  `Option<(U256, i32, bool)>` instead of `(U256, i32, bool)`, returning
  `None` for NaN and infinity rather than panicking. Implementors and callers
  of the public `DecimalFormat` trait must handle the `Option` (fd-aqs.13).

### Fixed

- The `cbrt` and `pow` kernels no longer raise `INEXACT` on an exactly
  representable result. A new `exact` module, which allocates nothing, proves
  a perfect cube root (`cbrt(8) = 2`) or an exact integer or rational power
  (`pow(10, 300) = 1E+300`, `pow(4, 0.5) = 2`) in fixed width `U256` / `U384`
  integer arithmetic, and the kernel clears the flag only on that proof.
  `exp`, `ln`, and the trigonometric and hyperbolic families are unchanged
  (their results are irrational for every input that reaches the rounding
  step). The value is unchanged; the fix matches IEEE 754-2019 §7.5. See
  ADR-0047 (fd-92w.8).

## [0.1.0] - 2026-05-17

Initial release. The shared faithful Extended-precision transcendental
kernel (`exp` / `ln` / `exp2` / `log2` / `log10` / `cbrt` / `sin` /
`cos` / `tan` / `asin` / `acos` / `atan` / `atan2` / the hyperbolic
family / `pow`, the Payne-Hanek argument reduction, and the
`Extended` 50-digit intermediate with its constants) was extracted
from `ferrodec`'s private `math` module into this standalone `no_std`
crate (fd-r0l P0a.2, commits `d9106b0`..`756d336`), generic over the
`DecimalFormat` seam so every decimal sibling reuses one verified
implementation instead of a per-precision copy.

Behaviour-neutral for the formally-verified `Decimal128` parent: its
instantiation is byte-identical to the pre-extraction kernel, proven
by the unchanged property, conformance, and per-kernel suites. The
faithful-rounding contract is ADR-0021; the family-wide decision is
recorded in ADR-0024. Depends on `ferrodec-ieee` 0.1.4 (the decoded
`IeeeDecodedClass`) and `ferrodec-multiword` 0.1.0 (wide-integer
primitives).
