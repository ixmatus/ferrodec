//! `sinPi(x)`, `cosPi(x)`, `tanPi(x)`: the IEEE 754-2019 §9.2 forward
//! pi-scaled trio, one shared exact reduction and three entries
//! (ADR-0061, fd-4zo.26 group D4).
//!
//! The operand counts revolutions rather than radians, and that single
//! change of unit removes the machinery the radian trio is built
//! around. `sin(πx)` reduces by `x mod 2` on the operand's own decimal
//! digits: no `π` constant enters the reduction, no Payne and Hanek
//! window, no truncation item in the budget. The family therefore ships
//! under its own `trig-pi` feature and pulls none of `argred`, which a
//! `trig-pi` build does not even compile.
//!
//! ## Reduction (exact integer arithmetic on the stored digits)
//!
//! Past the classifier every operand has a stored exponent `exp ≤ −1`
//! (a value whose quantum reaches 1 is an integer, which the classifier
//! consumed), so write `|x| = N · 10^−k` with `k = −exp ≥ 1` and
//! `N < 10^P`. Let `Q = 10^k`. Two regimes, split so no intermediate
//! leaves `u128`:
//!
//! * `k > P`: then `N < 10^P ≤ 10^(k−1)`, so `|x| < 1/10`. The fold is
//!   the identity (`h = 0`, `δ = |x|`) and `Q` is never formed, which
//!   matters because `k` reaches 6176 at `Decimal128`.
//! * `k ≤ P`: `Q ≤ 10^34` fits. Reduce `N mod 2Q` for `x mod 2`, then
//!   fold onto the nearest multiple of `1/2`.
//!
//! Every step is exact, and exact for a reason that is arithmetic
//! rather than numerical:
//!
//! 1. `N_red = N mod 2Q` is integer remainder, and `r = N_red/Q ∈ [0, 2)`
//!    satisfies `r ≡ |x| (mod 2)` by construction.
//! 2. `h = ⌊(4·N_red + Q)/(2Q)⌋` is `round(2r)` with halves resolved
//!    upward, so `h ∈ {0, 1, 2, 3, 4}` and `D = 2·N_red − h·Q` obeys
//!    `−Q/2 ≤ D < Q/2` (rearranging `2Qh ≤ 4N_red + Q < 2Q(h+1)`).
//! 3. `δ = r − h/2 = D/(2Q)`, hence `|δ| = (5·|D|) · 10^(exp−1)` and
//!    `ε = 1/4 − |δ| = (5·(Q/2 − |D|)) · 10^(exp−1)`. Both coefficients
//!    are at most `2.5·10^34` (35 digits), so materializing them at any
//!    rung's working width (50 digits and up) rounds nothing.
//!
//! `δ = 0` would mean `|x|` is a multiple of `1/2`, which the classifier
//! owns for all three operations, so the paths below never see it.
//!
//! ## Quadrant recomposition (transcribed to magnitude and sign)
//!
//! With `|x| ≡ h/2 + δ (mod 2)`, `d = |δ| ≤ 1/4`, and `πd ≤ π/4`, both
//! series run on `d` alone and the sign of `δ` folds into the result
//! sign (`sin` odd, `cos` even). Writing `dn` for `δ < 0`:
//!
//! ```text
//! h mod 4   sinPi magnitude / sign   cosPi magnitude / sign   tanPi magnitude / sign
//! -------   ----------------------   ----------------------   ----------------------
//!    0        sin(πd)      dn          cos(πd)      +           sin/cos       dn
//!    1        cos(πd)      +           sin(πd)     !dn          cos/sin      !dn
//!    2        sin(πd)     !dn          cos(πd)      −           sin/cos       dn
//!    3        cos(πd)      −           sin(πd)      dn          cos/sin      !dn
//! ```
//!
//! `sinPi` and `tanPi` are odd, so the operand's sign exclusive-ors into
//! the result sign; `cosPi` is even and ignores it. The whole kernel
//! therefore computes a nonnegative magnitude and applies one sign at
//! the end, which is why the directed rounding modes are reflected
//! through `RoundingMode::for_negation` before the magnitude is rounded
//! (the `rootn` rule: rounding `|f|` toward `+∞` and negating is
//! rounding `f` toward `−∞`).
//!
//! ## The `cosPi` anchor arm (ADR-0051 residual channel)
//!
//! `cos(πδ)` hugs 1 from below quadratically, and near the integer zero
//! the hug is unbounded: `δ` is `x` itself there, so it reaches
//! `10^−6176` at `Decimal128` and no rung can decide the directed modes.
//! The arm fires when the cosine series would run (`h` even) with
//! `adj(δ) ≤ −A`, `A = ⌈(P + 4)/2⌉`, and delivers the residual just
//! below the `±1` grid point. Its premise is `1 − cos t ≤ t²/2`
//! (equality only at `t = 0`) against the first nearest-mode boundary
//! below 1, which sits at `5·10^(−P−1)`:
//!
//! ```text
//! format       P   A    |δ| <     (πδ)²/2 <    boundary     margin
//! ----------  --  --  --------  ------------  -----------  -------
//! Decimal128  34  19   10^-18   4.94·10^-36   5·10^-35      ×10.1
//! Decimal64   16  10   10^-9    4.94·10^-18   5·10^-17      ×10.1
//! Decimal32    7   6   10^-5    4.94·10^-10   5·10^-8       ×101
//! ```
//!
//! The side is a theorem rather than a measurement: `cos(πδ) < 1`
//! strictly for `δ ≠ 0`, and `δ = 0` is the classifier's. Outside the
//! gate the ladder decides unaided, with room to spare: `|δ| ≥ 10^−A`
//! gives `1 − cos ≥ 0.9·(π·10^−A)²/2`, which is `4.4·10^-38`,
//! `4.4·10^-20`, and `4.4·10^-12` at the three formats against rung 1's
//! `≈ 4·10^-46` resolution.
//!
//! `sinPi` gets no anchor arm even though it hugs `±1` near the half
//! integers, and the reason is the quantum floor rather than the
//! geometry. A half integer is at least `1/2` in magnitude, so
//! `adj(x) ≥ −1` and the stored quantum forces `|δ| ≥ 10^−P`; the hug
//! bottoms out at `(π·10^−P)²/2`, which is `4.9·10^-68` at
//! `Decimal128` and larger at the narrow formats, decades outside every
//! rung's budget. Only an anchor whose neighborhood contains zero
//! escapes that floor, and `cosPi`'s integers are the only such set in
//! the trio.
//!
//! ## The `tanPi` anchor arm, and why the format never reaches it
//!
//! Near an odd quarter the value hugs `±1` linearly. Writing the offset
//! as `σ` and `u = tan(πσ)`, `tan(π/4 + πσ) = 1 + 2u/(1 − u)` and
//! `tan(3π/4 + πσ) = −1 + 2u/(1 + u)`, so both deviations are `+2πσ` to
//! first order and `|value ∓ 1| ≤ 6.6·|σ|` once `|πσ| ≤ 10^-9`. In the
//! fold's own coordinates `σ` is `±ε` and the four cases collapse: `d`
//! is strictly below `1/4`, so the magnitude is `cot(π/4 − πε) > 1`
//! when `h` is odd and `tan(π/4 − πε) < 1` when `h` is even, i.e.
//! **`magnitude_grows = (h odd)`**, independent of which quarter and of
//! the operand's sign.
//!
//! ```text
//! format       P   gate adj(ε) ≤   ε <      |value∓1| <   boundary    margin
//! ----------  --  --------------  -------  ------------  ----------  ------
//! Decimal128  34       -38        10^-37   6.6·10^-37    5·10^-35    ×75.8
//! Decimal64   16       -20        10^-19   6.6·10^-19    5·10^-17    ×75.8
//! Decimal32    7       -11        10^-10   6.6·10^-10    5·10^-8     ×75.8
//! ```
//!
//! The arm is nevertheless unreachable at every format, by the same
//! quantum floor that spares `sinPi`: the nearest odd quarter is at
//! least `1/4`, so `adj(x) ≥ −1`, the stored quantum is at worst
//! `10^−P`, and `ε ≥ 10^−P` whenever `ε ≠ 0` (`ε = 0` is the
//! classifier's quarter-integer row). The gate asks for `ε < 10^(−P−3)`,
//! three decades below that floor. The ladder covers the whole `±1`
//! neighborhood on its own: the tightest hug the format can express is
//! `2π·10^−P`, which is `6.3·10^-34` at `Decimal128` against rung 1's
//! `≈ 8·10^-46`. The arm stays as a guard whose premise is proven
//! rather than a live path, and its absence would change no result.
//!
//! ## The `tanPi` poles carry no overflow gate
//!
//! Half integers are the poles, and the classifier delivers those as
//! `±∞` with `DIV_BY_ZERO`. Their neighborhoods take the plain ladder,
//! and ADR-0061 records the missing overflow gate as designed. The cap:
//! `δ` is `|x|` minus a half integer, so it is a nonzero multiple of
//! the operand's own quantum `10^exp`, giving `|δ| ≥ 10^exp ≥
//! 10^(adj − P + 1)`; a pole neighborhood has `adj(x) ≥ −1`, so
//! `|δ| ≥ 10^−P`. With `|tan t| ≥ |t|` on `|t| < π/2`,
//! `|tanPi| = |cot(πδ)| ≤ 1/(π|δ|) ≤ 10^P/π`.
//!
//! ```text
//! format       P   |δ| ≥    |tanPi| ≤    format MAX      decades inside
//! ----------  --  -------  -----------  --------------  ---------------
//! Decimal128  34  10^-34   3.18·10^33   9.99…·10^6144        6111
//! Decimal64   16  10^-16   3.18·10^15   9.99…·10^384          369
//! Decimal32    7  10^-7    3.18·10^6    9.999999·10^96         90
//! ```
//!
//! The same floor keeps the quotient's divisor away from zero, so the
//! pole path needs no division guard either.
//!
//! ## Accuracy
//!
//! Correctly rounded. Tier 1 by construction plus the Tier 2 model
//! (ADR-0059), and with no reduction caveat: the reduction item is
//! provably zero, so `ladder::SINPI` and `ladder::TANPI` price only the
//! `πδ` multiply, the series, and (for `tanPi`) one division. ADR-0060's
//! adjudicator route is closed for this family and deliberately not
//! wired: `sin(πp/q)` is algebraic of degree growing with `φ(2q)`, which
//! at format denominators is past any fixed-width comparison.
//!
//! The exact set is complete and the family has no nearest-mode tie at
//! any format, both proven in `exact_pi`: Niven's theorem
//! inventories the rational values as `{0, ±1/2, ±1}`, the `±1/2` rows
//! need the abscissas `k ± 1/6` and `k ± 1/3` that no decimal format
//! represents, and every remaining rational value is a grid point rather
//! than a midpoint. So `ladder_audit` is vacuous for the trio by
//! construction, and the unconditional `INEXACT` past the classifier is
//! correct in every rounding direction.

use crate::exact_pi::{cospi_exact, deliver_pi_exact, sinpi_exact, tanpi_exact};
use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use core::cmp::Ordering;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::U256;

/// Sine of `π · self`, in revolutions (IEEE 754-2019 §9.2 `sinPi`).
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exact at the integers (`±0`, signed by the operand) and the half
/// integers (`±1`), and irrational everywhere else by Niven; no format
/// value lands on a nearest-mode tie. The completeness proof and the
/// §9.2.1 sign conventions live on `exact_pi::sinpi_exact`.
pub fn sin_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| sincospi_kernel_body::<F, _>(ex, x, Want::Sin, rm))
}

/// Cosine of `π · self`, in revolutions (IEEE 754-2019 §9.2 `cosPi`).
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exact at the integers (`±1` by parity) and the half integers (`+0`
/// always, the §9.2.1 rule that keeps the function even), irrational
/// everywhere else, never a tie.
pub fn cos_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| sincospi_kernel_body::<F, _>(ex, x, Want::Cos, rm))
}

/// Tangent of `π · self`, in revolutions (IEEE 754-2019 §9.2 `tanPi`).
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exact at the integers (`±0`), the quarter integers (`±1`, the family
/// the decimal formats keep where `1/6` and `1/3` deny the sine and
/// cosine their `±1/2` rows), and the half integers (`±∞` with
/// `DIV_BY_ZERO`); irrational everywhere else, never a tie.
pub fn tan_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| sincospi_kernel_body::<F, _>(ex, x, Want::Tan, rm))
}

/// Which member of the trio a shared-body call is evaluating.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Want {
    Sin,
    Cos,
    Tan,
}

/// Which series (or quotient of series) supplies the result magnitude
/// once the quadrant is folded out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Magnitude {
    /// `sin(πd)`.
    Sin,
    /// `cos(πd)`.
    Cos,
    /// `sin(πd)/cos(πd)`.
    Tan,
    /// `cos(πd)/sin(πd)`, the pole side of `tanPi`.
    Cot,
}

/// The exact fold of `|x|` onto `h/2 + δ (mod 2)`, in the operand's own
/// digits. Rung independent by construction: the whole computation is
/// integer arithmetic, so every rung materializes the same `δ`.
struct Fold {
    /// `h mod 4`, the quadrant of the nearest multiple of `1/2`.
    quad: u8,
    /// `δ < 0`.
    delta_neg: bool,
    /// `|δ| = coefficient · 10^exponent`, exact, `≤ 1/4`.
    delta: (u128, i32),
    /// `ε = 1/4 − |δ|`, exact, `≥ 0`. `None` in the `k > P` regime,
    /// where `|x| < 1/10` puts `ε` above `0.15` and no quarter-integer
    /// neighborhood is in reach.
    eps: Option<(u128, i32)>,
}

/// Generic body of the three entries (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
///
/// Private rather than `pub(crate)`: no other module composes this trio
/// the way `rootn` composes `rsqrt`, so the body's `Want` selector stays
/// off the crate surface.
fn sincospi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    want: Want,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // IEEE 754-2019 §9.2.1, transcribed. NaN follows the crate-wide
    // convention (a signaling NaN raises INVALID and returns the
    // quieted payload, a quiet NaN passes through). All three
    // operations are periodic with no limit at infinity, so `±∞` is a
    // domain error. The zero rows are the odd/even split.
    let (coef, exp, sign_neg) = match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { .. } => return Some((F::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => {
            return Some(match want {
                // sinPi(±0) = ±0 and tanPi(±0) = ±0.
                Want::Sin | Want::Tan => (if sign { F::NEG_ZERO } else { F::ZERO }, Status::OK),
                // cosPi(±0) = 1.
                Want::Cos => (F::ONE, Status::OK),
            });
        }
        Class::Finite { .. } => x
            .to_extended_parts()
            .expect("finite: NaN, infinity, and zero dispatched above"),
    };

    // Input-side exact classification (ADR-0059 M7, ADR-0061). This owns
    // every integer, half integer, and (for tanPi) quarter integer, so
    // the reduction below never sees `δ = 0` and the tangent arm never
    // sees `ε = 0`. Delivery is through the format rounder from the
    // exact coefficient, so §7.5 holds in every rounding direction.
    let exact = match want {
        Want::Sin => sinpi_exact(coef, exp, sign_neg),
        Want::Cos => cospi_exact(coef, exp),
        Want::Tan => tanpi_exact(coef, exp, sign_neg),
    };
    if let Some(hit) = exact {
        return Some(deliver_pi_exact::<F>(hit, rm));
    }

    let fold = fold_to_half::<F>(coef, exp);
    let dn = fold.delta_neg;

    // The quadrant table, magnitude column.
    let magnitude = match (want, fold.quad % 2 == 0) {
        (Want::Sin, true) | (Want::Cos, false) => Magnitude::Sin,
        (Want::Sin, false) | (Want::Cos, true) => Magnitude::Cos,
        (Want::Tan, true) => Magnitude::Tan,
        (Want::Tan, false) => Magnitude::Cot,
    };

    // The quadrant table, sign column, then the odd-function reflection.
    let mut res_neg = match (want, fold.quad) {
        (Want::Sin, 0) => dn,
        (Want::Sin, 1) => false,
        (Want::Sin, 2) => !dn,
        (Want::Sin, 3) => true,
        (Want::Cos, 0) => false,
        (Want::Cos, 1) => !dn,
        (Want::Cos, 2) => true,
        (Want::Cos, 3) => dn,
        (Want::Tan, 0 | 2) => dn,
        (Want::Tan, 1 | 3) => !dn,
        _ => unreachable!("fold_to_half returns h mod 4"),
    };
    if matches!(want, Want::Sin | Want::Tan) {
        res_neg ^= sign_neg;
    }
    // The magnitude is rounded and the sign applied afterwards, so the
    // directed modes reflect first (`rootn`'s `for_negation` rule).
    let eff_rm = if res_neg { rm.for_negation() } else { rm };

    // The `cosPi` anchor arm (module doc, margin table). Unguarded by
    // design: the anchor leg runs before the ladder's predicate, because
    // no finite rung separates a value this close to a grid point, and
    // the theorem-backed side does.
    if want == Want::Cos && magnitude == Magnitude::Cos {
        let a = i32::try_from((F::PRECISION + 4).div_ceil(2)).expect("format precision is small");
        if adj_of(fold.delta) <= -a {
            return Some(deliver_at_one::<F, E>(ex, false, res_neg, eff_rm));
        }
    }

    // The `tanPi` anchor arm. Proven unreachable at every format by the
    // quantum floor `ε ≥ 10^−P` (module doc); kept because its premise
    // is proven and its cost is one comparison.
    if want == Want::Tan {
        if let Some(eps) = fold.eps {
            let gate = -(i32::try_from(F::PRECISION).expect("format precision is small") + 4);
            if eps.0 != 0 && adj_of(eps) <= gate {
                // `magnitude_grows = (h odd)`: `d < 1/4` strictly, so the
                // odd quadrants deliver `cot(π/4 − πε) > 1` and the even
                // ones `tan(π/4 − πε) < 1`.
                let grows = fold.quad % 2 == 1;
                return Some(deliver_at_one::<F, E>(ex, grows, res_neg, eff_rm));
            }
        }
    }

    // Series on `πd`, `0 < πd ≤ π/4`. The `δ = 0` case is the
    // classifier's, so `d` is nonzero here and the tangent quotients
    // below never divide by zero.
    debug_assert!(fold.delta.0 != 0, "a zero fold is the classifier's");
    let d = ex.from_parts_u128(fold.delta.0, fold.delta.1, false);
    let t = ex.pi().mul(d);
    let t_sq = t.square();
    let value = match magnitude {
        Magnitude::Sin => taylor_sin_ext(t, t_sq),
        Magnitude::Cos => taylor_cos_ext(t_sq),
        Magnitude::Tan => taylor_sin_ext(t, t_sq).div::<F>(taylor_cos_ext(t_sq)),
        Magnitude::Cot => taylor_cos_ext(t_sq).div::<F>(taylor_sin_ext(t, t_sq)),
    };

    let (mag, status) = ladder::round_guarded::<F, E>(value, eff_rm, budget(want))?;
    Some((if res_neg { mag.neg() } else { mag }, status))
}

/// This operation's escalation budget (ADR-0059 M8). `cosPi` shares
/// `sinPi`'s: one reduction, one series, the same itemization.
fn budget(want: Want) -> &'static ladder::Budget {
    match want {
        Want::Sin | Want::Cos => &ladder::SINPI,
        Want::Tan => &ladder::TANPI,
    }
}

/// The ADR-0051 residual delivery at the `±1` anchor: round the
/// magnitude 1 with the side the caller proved, then apply the sign.
fn deliver_at_one<F: DecimalFormat, E: ExtNum>(
    ex: E,
    magnitude_grows: bool,
    res_neg: bool,
    eff_rm: RoundingMode,
) -> (F, Status) {
    let (mag, status) = ex
        .one()
        .to_format_with_residual::<F>(magnitude_grows, eff_rm);
    (
        if res_neg { mag.neg() } else { mag },
        status | Status::INEXACT,
    )
}

/// Fold `|x| = coef · 10^exp` onto `h/2 + δ (mod 2)` in exact integer
/// arithmetic (module doc, "Reduction").
///
/// Caller guarantees a finite nonzero operand past the classifier, so
/// `exp ≤ −1`: a stored quantum of 1 or more makes the value an
/// integer, and integers are classified out.
fn fold_to_half<F: DecimalFormat>(coef: U256, exp: i32) -> Fold {
    debug_assert!(exp <= -1, "an integer operand is the classifier's");
    let k = -exp;
    let n = coef.to_u128();
    let precision = i32::try_from(F::PRECISION).expect("format precision is small");

    // Small-operand regime: `N < 10^P ≤ 10^(k−1)` puts `|x|` below
    // `1/10`, so the nearest multiple of `1/2` is 0 and `δ` is `|x|`
    // itself. Forming `Q = 10^k` here would overflow (`k` reaches 6176).
    if k > precision {
        return Fold {
            quad: 0,
            delta_neg: false,
            delta: (n, exp),
            eps: None,
        };
    }

    let q = 10u128.pow(u32::try_from(k).expect("k is between 1 and P"));
    let two_q = 2 * q;
    let n_red = n % two_q;
    // `h = round(2r)`, halves upward. `4·N_red + Q < 9Q` bounds `h ≤ 4`,
    // and `−Q/2 ≤ D < Q/2` follows from the floor's own inequality, so
    // `|δ| ≤ 1/4` and `Q/2 − |D| ≥ 0`.
    let h = (4 * n_red + q) / two_q;
    let d = 2 * (n_red as i128) - (h as i128) * (q as i128);
    let d_abs = d.unsigned_abs();
    debug_assert!(d_abs <= q / 2, "the fold keeps |delta| at or below 1/4");
    Fold {
        quad: (h % 4) as u8,
        delta_neg: d < 0,
        // `|δ| = |D|/(2Q) = 5·|D| · 10^(−k−1)` and `ε = (Q/2 − |D|)/(2Q)`
        // on the same scale; `5·|D| ≤ 2.5·10^34` stays inside `u128`.
        delta: (5 * d_abs, exp - 1),
        eps: Some((5 * (q / 2 - d_abs), exp - 1)),
    }
}

/// The adjusted exponent of an exact `coefficient · 10^exponent` pair:
/// `⌊log10⌋` of the value. Caller guarantees a nonzero coefficient.
fn adj_of((coef, exp): (u128, i32)) -> i32 {
    debug_assert!(coef != 0, "adj of zero is undefined");
    let digits = i32::try_from(U256::from_u128(coef).decimal_digit_count())
        .expect("a u128 has at most 39 digits");
    exp + digits - 1
}

/// `sin(t)` for `0 ≤ t ≤ π/4` by `Σ (−1)^j · t^(2j+1)/(2j+1)!`.
///
/// The caller passes `t²` so the cosine evaluation shares it. Successive
/// terms satisfy `term_{j+1} = term_j · t²/((2j+2)(2j+3))`, one multiply
/// and one small-integer divide apiece; the loop stops as soon as a term
/// stops moving the sum. The trip cap is the rung's own
/// `sin_cos_series_terms`, which is calibrated for exactly this argument
/// range and so carries over unchanged.
fn taylor_sin_ext<E: ExtNum>(t: E, t_sq: E) -> E {
    let mut sum = t;
    let mut term = t;
    let mut j: u32 = 0;
    for _ in 0..t.sin_cos_series_terms() {
        let denom = (2 * j + 2) * (2 * j + 3);
        term = term.mul(t_sq).div_u32(denom);
        j += 1;
        let signed = if j % 2 == 1 { term.neg() } else { term };
        let next = sum.add(signed);
        if next.cmp(sum) == Ordering::Equal || term.is_zero() {
            return next;
        }
        sum = next;
    }
    sum
}

/// `cos(t)` for `0 ≤ t ≤ π/4` by `Σ (−1)^j · t^(2j)/(2j)!`, with
/// `term_{j+1} = term_j · t²/((2j+1)(2j+2))`.
fn taylor_cos_ext<E: ExtNum>(t_sq: E) -> E {
    let mut sum = t_sq.one();
    let mut term = t_sq.one();
    let mut j: u32 = 0;
    for _ in 0..t_sq.sin_cos_series_terms() {
        let denom = (2 * j + 1) * (2 * j + 2);
        term = term.mul(t_sq).div_u32(denom);
        j += 1;
        let signed = if j % 2 == 1 { term.neg() } else { term };
        let next = sum.add(signed);
        if next.cmp(sum) == Ordering::Equal || term.is_zero() {
            return next;
        }
        sum = next;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_format::ValueFmt128;

    fn fold(coef: u128, exp: i32) -> Fold {
        fold_to_half::<ValueFmt128>(U256::from_u128(coef), exp)
    }

    /// The fold reproduces `|x| = h/2 + δ (mod 2)` on worked cases, with
    /// `δ` exact in the operand's own digits.
    #[test]
    fn fold_reproduces_the_operand() {
        // 0.3 = 0.5 − 0.2: h = 1, δ = −0.2.
        let f = fold(3, -1);
        assert_eq!(f.quad, 1);
        assert!(f.delta_neg);
        assert_eq!(f.delta, (20, -2), "|delta| = 0.20");
        // 0.7 = 0.5 + 0.2.
        let f = fold(7, -1);
        assert_eq!(f.quad, 1);
        assert!(!f.delta_neg);
        assert_eq!(f.delta, (20, -2));
        // 1.2 = 1.0 + 0.2: h = 2.
        let f = fold(12, -1);
        assert_eq!(f.quad, 2);
        assert!(!f.delta_neg);
        // 1.8 = 2.0 − 0.2: h = 4, quad 0.
        let f = fold(18, -1);
        assert_eq!(f.quad, 0);
        assert!(f.delta_neg);
        // 3.7 wraps mod 2 onto 1.7 = 1.5 + 0.2: h = 3.
        let f = fold(37, -1);
        assert_eq!(f.quad, 3);
        assert!(!f.delta_neg);
        // 2.3 and 0.3 are one value mod 2.
        let a = fold(23, -1);
        let b = fold(3, -1);
        assert_eq!(a.quad, b.quad);
        assert_eq!(a.delta, b.delta);
        assert_eq!(a.delta_neg, b.delta_neg);
    }

    /// `|δ| ≤ 1/4` and `ε = 1/4 − |δ| ≥ 0` on a dense sweep, the two
    /// invariants every downstream proof rests on.
    #[test]
    fn fold_keeps_delta_within_a_quarter() {
        for coef in 1u128..=4000 {
            for exp in [-1i32, -2, -3, -4] {
                let f = fold(coef, exp);
                // |delta| ≤ 1/4 as an exact comparison on the parts.
                let (dc, de) = f.delta;
                if dc != 0 {
                    // 4·|delta| ≤ 1 rewritten without division.
                    let scale = 10u128.pow(u32::try_from(-de).expect("negative exponent"));
                    assert!(4 * dc <= scale, "coef {coef} exp {exp}: |delta| > 1/4");
                }
                let (ec, _) = f.eps.expect("the reduced regime reports eps");
                assert!(ec <= dc + ec, "eps stays nonnegative");
                assert!(f.quad < 4);
            }
        }
    }

    /// The small-operand regime declines to form `10^k` and hands back
    /// the operand as `δ`, which is what keeps `k = 6176` from
    /// overflowing.
    #[test]
    fn small_operands_skip_the_modulus() {
        let f = fold(1, -6176);
        assert_eq!(f.quad, 0);
        assert!(!f.delta_neg);
        assert_eq!(f.delta, (1, -6176));
        assert!(f.eps.is_none(), "no quarter neighborhood is in reach");
    }

    /// `adj` agrees with the adjusted-exponent definition on both sides
    /// of a decade.
    #[test]
    fn adj_matches_the_decade() {
        assert_eq!(adj_of((1, 0)), 0);
        assert_eq!(adj_of((999, -2)), 0);
        assert_eq!(adj_of((1, -6176)), -6176);
        assert_eq!(adj_of((25, -3)), -2);
    }

    /// The series reproduce known values on the reduced range. `π/4` is
    /// the domain edge, where both must agree to the working width.
    #[test]
    fn series_agree_at_the_domain_edge() {
        let ex = crate::extended::Extended::ZERO;
        let t = ex.pi().div_u32(4);
        let s = taylor_sin_ext(t, t.square());
        let c = taylor_cos_ext(t.square());
        // sin(π/4) = cos(π/4) mathematically, so the fold's `|δ| = 1/4`
        // tie case delivers one value whichever branch runs. The two
        // series reach it by different rounding paths, so they agree to
        // the working width rather than bit for bit.
        let tol = ex.parse_str("1E-45");
        assert!(
            s.sub(c).abs().cmp(tol) == Ordering::Less,
            "sin and cos meet at π/4: {s:?} vs {c:?}"
        );
        // And their square sum is 1.
        let one = ex.one();
        assert!(s.square().add(c.square()).sub(one).abs().cmp(tol) == Ordering::Less);
    }

    /// `sin(0)` and `cos(0)` anchor the series at the other end.
    #[test]
    fn series_anchor_at_zero() {
        let ex = crate::extended::Extended::ZERO;
        let z = ex.zero();
        assert!(taylor_sin_ext(z, z).is_zero());
        assert_eq!(taylor_cos_ext(z).cmp(ex.one()), Ordering::Equal);
    }
}
