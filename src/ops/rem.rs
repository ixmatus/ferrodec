//! IEEE 754 remainder for [`Decimal128`], plus the truncating-quotient
//! variant.
//!
//! Two flavors:
//!
//! * [`Decimal128::rem`] — IEEE 754 §5.3.1 `remainder`:
//!   `r = x − n × y` where `n` is the nearest-even integer to `x / y`.
//!   Result magnitude `≤ |y| / 2`. Always exact when defined; never
//!   raises `INEXACT`.
//! * [`Decimal128::rem_trunc`] — truncating-quotient remainder:
//!   `r = x − trunc(x / y) × y` (the integer quotient rounds toward
//!   zero). Result has sign of dividend, magnitude `< |y|`. Matches
//!   C99 `fmod` and decTest's `remainder` op (distinct from
//!   decTest's `remaindernear`, which is the round-half-to-even
//!   variant `rem` implements). Always exact when defined.
//!
//! Special cases (IEEE 754-2019 §5.3.1):
//!
//! * NaN propagation; sNaN → `INVALID`.
//! * `x / 0` (finite `x`) → NaN + `INVALID`.
//! * `±∞ / y` → NaN + `INVALID`.
//! * `x / ±∞` (finite `x`) → `x` (the result equals `x` exactly).
//! * `x = ±0` → `±0` with the sign of `x`.
//!
//! ## Implementation
//!
//! The finite-finite kernel aligns both operands to a common quantum
//! `q_min = min(q_x, q_y)`. If the aligned numerator and denominator
//! both fit in the working envelope (numerator in U256, denominator in
//! `u128`), we do an exact `div_rem_u128` and round the quotient
//! to nearest-even, returning `x − n·y` packed at `q_min`.
//!
//! Two boundary cases short-circuit:
//!
//! 1. **Aligned divisor overflows `u128`** (`cy_digits > 38`). This
//!    only happens when `q_y > q_x`, so the aligned dividend has
//!    `cx_digits ≤ 34` while the aligned divisor exceeds `10^38`.
//!    Therefore `|y_scaled| > |x_scaled|`, the integer quotient `n`
//!    is zero, and the remainder is exactly `x`. We return `self`.
//! 2. **Aligned numerator overflows `U256`** (`cx_digits > 75`). The
//!    bound `cy_digits ≤ 38` (already enforced by the previous check)
//!    forces `n_digits ≥ cx_digits − cy_digits ≥ 76 − 38 = 38`, which
//!    always exceeds `PRECISION = 34`. The dec-spec
//!    [`Division_impossible`] condition therefore applies in every
//!    case that hits this branch, so we return `NaN + INVALID`. This
//!    is the same answer the in-band `q.decimal_digit_count() >
//!    PRECISION` check produces when the operation does fit U256 —
//!    the early-return is purely a working-buffer-size guard, not a
//!    semantic limitation.
//!
//! [`Division_impossible`]: https://speleotrove.com/decimal/daops.html#refrema

use crate::bid::{classify_bits, decimal_digit_count, pack_finite, Class, BIAS, BIASED_EXP_MAX};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::propagate_nan2;
use crate::status::Status;

impl Decimal128 {
    /// IEEE 754 §5.3.1 `remainder(self, rhs)`.
    ///
    /// Returns `r = self − n · rhs` where `n` is the integer nearest to
    /// `self / rhs` with ties rounding to even. Result magnitude
    /// `≤ |rhs| / 2`. Always exact when defined; never raises
    /// `INEXACT`.
    ///
    /// Distinct from [`Decimal128::rem_trunc`], which uses a
    /// truncating-quotient (C99 `fmod`-style) rule.
    #[must_use]
    pub fn rem(self, rhs: Self) -> (Self, Status) {
        if let Some(early) = rem_special_cases(self, rhs) {
            return early;
        }
        rem_finite(self, rhs, RemRounding::HalfEven)
    }

    /// Truncating-quotient remainder: `r = self − trunc(self / rhs) · rhs`.
    ///
    /// The integer quotient rounds toward zero, so the result has the
    /// sign of `self` and magnitude `< |rhs|`. Matches C99 `fmod` and
    /// the dec-spec / decTest `remainder` op. Always exact when
    /// defined; never raises `INEXACT`.
    ///
    /// Distinct from [`Decimal128::rem`], which uses the IEEE 754
    /// round-half-to-even quotient rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // 7.5 mod 2.0:
    /// //   trunc(7.5 / 2.0) = trunc(3.75) = 3
    /// //   7.5 − 3 × 2.0 = 1.5
    /// let x = Decimal128::try_new(75, -1).unwrap();
    /// let y = Decimal128::try_new(20, -1).unwrap();
    /// let (r, _) = x.rem_trunc(y);
    /// assert_eq!(r.to_bits(), Decimal128::try_new(15, -1).unwrap().to_bits());
    ///
    /// // Compare with `rem` (round-half-to-even quotient):
    /// //   round-half-even(3.75) = 4
    /// //   7.5 − 4 × 2.0 = -0.5
    /// let (r_ieee, _) = x.rem(y);
    /// assert_eq!(r_ieee.to_bits(), Decimal128::try_new(-5, -1).unwrap().to_bits());
    /// ```
    #[must_use]
    pub fn rem_trunc(self, rhs: Self) -> (Self, Status) {
        if let Some(early) = rem_special_cases(self, rhs) {
            return early;
        }
        rem_finite(self, rhs, RemRounding::TowardZero)
    }

    /// Kani-only entry point for the special-case path.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn rem_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        rem_special_cases(self, rhs)
    }
}

/// Quotient-rounding rule for the shared `rem_finite` kernel.
#[derive(Clone, Copy)]
enum RemRounding {
    /// IEEE 754 §5.3.1: nearest integer, ties to even.
    HalfEven,
    /// C99 `fmod`: integer quotient toward zero (drop fractional part).
    TowardZero,
}

#[inline]
fn rem_special_cases(a: Decimal128, b: Decimal128) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan =
        matches!(cls_a, Class::SignalingNaN { .. }) || matches!(cls_b, Class::SignalingNaN { .. });
    let status = if snan { Status::INVALID } else { Status::OK };

    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((propagate_nan2(a, b), status));
    }

    // x / 0 — invalid.
    if matches!(cls_b, Class::Zero { .. }) {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // ±∞ / y — invalid.
    if matches!(cls_a, Class::Infinity { .. }) {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // x / ±∞ = x (preserve cohort).
    if matches!(cls_b, Class::Infinity { .. }) {
        return Some((a, status));
    }
    // ±0 / y_finite_nonzero = ±0 with sign of x. Preferred quantum
    // per dec spec is `min(qx, qy)`.
    if let Class::Zero { sign, biased_exp } = cls_a {
        let qy = match cls_b {
            Class::Zero { biased_exp, .. } | Class::Finite { biased_exp, .. } => biased_exp,
            _ => biased_exp,
        };
        let q = biased_exp.min(qy);
        return Some((Decimal128::from_bits(pack_finite(sign, q, 0)), status));
    }

    None
}

fn rem_finite(a: Decimal128, b: Decimal128, rounding: RemRounding) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sx, qxb, cx) = decompose_finite(cls_a);
    let (_sy, qyb, cy) = decompose_finite(cls_b);
    debug_assert!(cx != 0 && cy != 0);

    let qx = qxb as i32 - BIAS as i32;
    let qy = qyb as i32 - BIAS as i32;

    let q_min = qx.min(qy);
    let dq_x = (qx - q_min) as u32;
    let dq_y = (qy - q_min) as u32;

    let cx_digits = decimal_digit_count(cx) + dq_x;
    let cy_digits = decimal_digit_count(cy) + dq_y;

    // Case 1: |y| ≫ |x| — divisor too wide for u128. Exact answer is `x`.
    if cy_digits > 38 {
        return (a, Status::OK);
    }
    // Case 2: aligned numerator overflows U256. With cy_digits ≤ 38
    // (enforced above), n_digits ≥ cx_digits − cy_digits ≥ 38, which
    // always trips dec-spec Division_impossible. NaN + INVALID is the
    // same answer the in-band check below produces; this is just a
    // working-buffer-size guard. See module docs.
    if cx_digits > 75 {
        return (Decimal128::NAN, Status::INVALID);
    }

    let y_scaled: u128 = cy * 10u128.pow(dq_y);
    let x_scaled = U256::from_u128(cx).mul_pow10(dq_x);

    let (q, r) = x_scaled.div_rem_u128(y_scaled);

    // dec-spec "Division_impossible": if the integer quotient would
    // exceed PRECISION digits, the remainder operation is undefined
    // and we return NaN+INVALID. This matters for cases like
    // `remaindernear (10^33) 0.1` where the integer quotient is 10^34.
    if q.decimal_digit_count() > crate::bid::PRECISION {
        return (Decimal128::NAN, Status::INVALID);
    }

    // The integer quotient `q` returned by `div_rem_u128` is already
    // truncated toward zero (it's unsigned integer division). For
    // `RemRounding::TowardZero` we keep it as-is — the remainder `r`
    // is the answer. For `RemRounding::HalfEven` we adjust by ±1 if
    // the fractional part is past `0.5 · y_scaled`.
    let round_up = match rounding {
        RemRounding::HalfEven => {
            let n_lsb = (q.lo & 1) as u32;
            compare_remainder_to_half(r, y_scaled, n_lsb)
        }
        RemRounding::TowardZero => false,
    };

    let (result_mag, sign_flip) = if round_up {
        (y_scaled - r, true)
    } else {
        (r, false)
    };

    if result_mag == 0 {
        // Exact zero remainder; sign is sign(x).
        let biased = clamp_biased(q_min);
        return (
            Decimal128::from_bits(pack_finite(sx, biased, 0)),
            Status::OK,
        );
    }

    let result_sign = sx ^ sign_flip;

    // Re-encode at quantum q_min, normalising trailing zeros if the
    // magnitude has more than `PRECISION` digits. The IEEE remainder is
    // always exactly representable when the operands are, so any
    // overflow above 34 digits must be made up of trailing zeros — we
    // shift right (dividing by 10) and increment the quantum until the
    // coefficient fits.
    let (mut coef, mut q_unbiased) = (result_mag, q_min);
    while decimal_digit_count(coef) > 34 {
        debug_assert!(coef % 10 == 0, "rem result not exactly representable");
        coef /= 10;
        q_unbiased += 1;
    }
    let biased = clamp_biased(q_unbiased);
    (
        Decimal128::from_bits(pack_finite(result_sign, biased, coef)),
        Status::OK,
    )
}

/// `r` vs `y_scaled / 2`, with tie-breaking by `n_lsb` (round-to-even).
/// Returns `true` if we should round the integer quotient up.
fn compare_remainder_to_half(r: u128, y_scaled: u128, n_lsb: u32) -> bool {
    if r == 0 {
        return false;
    }
    let (two_r, overflow) = r.overflowing_mul(2);
    if overflow {
        // 2r > u128::MAX ≥ y_scaled, so r > y_scaled / 2.
        return true;
    }
    match two_r.cmp(&y_scaled) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => n_lsb == 1,
    }
}

fn clamp_biased(unbiased: i32) -> u32 {
    let biased = unbiased + BIAS as i32;
    biased.clamp(0, BIASED_EXP_MAX as i32) as u32
}

fn decompose_finite(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => {
            debug_assert!(false, "decompose_finite on non-finite Class");
            (false, BIAS, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn d_finite(s: bool, exp: u32, coef: u128) -> Decimal128 {
        Decimal128::from_bits(pack_finite(s, exp, coef))
    }

    fn d_int(c: i128) -> Decimal128 {
        if c == 0 {
            return Decimal128::ZERO;
        }
        let sign = c < 0;
        let coef = c.unsigned_abs();
        d_finite(sign, BIAS, coef)
    }

    #[test]
    fn nan_propagates() {
        let (r, _) = Decimal128::ONE.rem(Decimal128::NAN);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.rem(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn x_over_zero_is_invalid_nan() {
        let (r, s) = Decimal128::ONE.rem(Decimal128::ZERO);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_over_y_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.rem(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn x_over_inf_is_x() {
        let (r, _) = d_int(7).rem(Decimal128::INFINITY);
        assert_eq!(r.to_bits(), d_int(7).to_bits());
        let (r, _) = Decimal128::NEG_ZERO.rem(Decimal128::INFINITY);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    /// `rem(x, ±∞)` short-circuits via `return Some((a, status))`
    /// (operand verbatim), distinct from the other special arms which
    /// re-canonicalise via `pack_finite`. The 2026-05-10 review (M-2
    /// in the core-arithmetic finding) flagged this asymmetry. In
    /// practice the asymmetry is benign because `classify_bits`
    /// canonicalises non-canonical Form A (coefficient ≥ 10^34) to
    /// `Class::Zero` at decode, so by the time `rem` reaches the
    /// Infinity-divisor case the dividend has already been classified
    /// as either Zero or Finite (with coefficient < 10^34). Pin the
    /// invariant explicitly so a future refactor that touches either
    /// `classify_bits`'s canonicalisation rule or `rem_special_cases`'s
    /// short-circuit surfaces the implicit dependency.
    #[test]
    fn rem_with_inf_divisor_preserves_non_canonical_form_a_safely() {
        // Construct a non-canonical Form A bit pattern: coefficient
        // 10^34 + 5 (just past COEFFICIENT_LIMIT), biased_exp BIAS.
        // `classify_bits` decodes this as Class::Zero per §3.5.2.
        let bias = crate::bid::BIAS;
        let non_canonical_coef = crate::bid::COEFFICIENT_LIMIT + 5;
        let bits = crate::bid::pack_finite(false, bias, non_canonical_coef);
        let a = Decimal128::from_bits(bits);
        // The dividend's class is Zero, not Finite — proving the
        // classify_bits canonicalisation that makes the rem
        // short-circuit safe.
        assert!(
            a.is_zero(),
            "non-canonical Form A must canonicalise to Zero on decode"
        );

        // rem(non_canonical_form_a, +Inf) returns the operand
        // verbatim per rem_special_cases line 157. Because the
        // operand is canonicalised, the verbatim return is safe.
        let (r, _) = a.rem(Decimal128::INFINITY);
        assert_eq!(r.to_bits(), a.to_bits());
        assert!(r.is_zero());
    }

    #[test]
    fn rem_trunc_basic_in_range() {
        // 7 fmod 3: trunc(7/3) = 2, 7 − 2·3 = 1 (sign of dividend).
        let (r, s) = d_int(7).rem_trunc(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        assert!(s.is_ok());

        // 8 fmod 3: trunc(8/3) = 2, 8 − 2·3 = 2. (Differs from rem,
        // which gives -1 because round-half-to-even of 8/3≈2.67 is 3.)
        let (r, _) = d_int(8).rem_trunc(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(2));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // -7 fmod 3: trunc(-7/3) = -2, -7 − (-2)·3 = -1.
        let (r, _) = d_int(-7).rem_trunc(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 7 fmod -3: trunc(7/-3) = -2, 7 − (-2)·(-3) = 1.
        let (r, _) = d_int(7).rem_trunc(d_int(-3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_trunc_diverges_from_rem_at_half_boundary() {
        // 5 fmod 2: trunc(2.5) = 2, result 1.
        // (vs rem: round-half-to-even(2.5) = 2 → result 1. Same here.)
        let (r_t, _) = d_int(5).rem_trunc(d_int(2));
        let (r_e, _) = d_int(5).rem(d_int(2));
        let (cmp, _) = r_t.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        let (cmp, _) = r_e.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 7 fmod 2: trunc(3.5) = 3, result 1.
        // rem: round-half-to-even(3.5) = 4, result -1. Different.
        let (r_t, _) = d_int(7).rem_trunc(d_int(2));
        let (r_e, _) = d_int(7).rem(d_int(2));
        let (cmp, _) = r_t.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        let (cmp, _) = r_e.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_trunc_special_cases() {
        // NaN propagation matches rem.
        let (r, _) = Decimal128::ONE.rem_trunc(Decimal128::NAN);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.rem_trunc(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());

        // x / 0 → NaN+INVALID.
        let (r, s) = Decimal128::ONE.rem_trunc(Decimal128::ZERO);
        assert!(r.is_nan());
        assert!(s.invalid());

        // ±∞ / y → NaN+INVALID.
        let (r, s) = Decimal128::INFINITY.rem_trunc(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());

        // x / ±∞ → x.
        let (r, _) = d_int(7).rem_trunc(Decimal128::INFINITY);
        assert_eq!(r.to_bits(), d_int(7).to_bits());
    }

    #[test]
    fn rem_basic_in_range() {
        // 7 mod 3: 7 = 2*3 + 1, |1| < 1.5, n=2, result = 1
        let (r, s) = d_int(7).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        assert!(s.is_ok());

        // 8 mod 3: 8 = 2*3 + 2, |2| > 1.5, n=3, result = -1
        let (r, _) = d_int(8).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // -7 mod 3: result = -1
        let (r, _) = d_int(-7).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // -8 mod 3: result = +1
        let (r, _) = d_int(-8).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_round_to_even_tie() {
        // 5 mod 2: q=2 r=1, 2r=2=y_scaled tie. q parity = even, round down.
        // result = 1.
        let (r, _) = d_int(5).rem(d_int(2));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 7 mod 2: q=3 r=1, tie. q parity = odd, round up.
        // result = -1.
        let (r, _) = d_int(7).rem(d_int(2));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_zero_when_divisible() {
        let (r, _) = d_int(12).rem(d_int(4));
        assert!(r.is_zero());
        let (r, _) = d_int(-12).rem(d_int(4));
        assert!(r.is_zero());
    }

    #[test]
    fn rem_y_much_larger_returns_x() {
        // x = 1, y = 1 × 10^100 (way larger). |x|/|y| → 0. n = 0.
        // Result = x.
        let huge_y = d_finite(false, BIAS + 100, 1);
        let (r, _) = Decimal128::ONE.rem(huge_y);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[cfg(feature = "fmt")]
    #[test]
    fn rem_division_impossible_at_buffer_boundary() {
        // Mirrors dqRemainderNear vectors dqrmn1051..1054. The aligned
        // numerator for `1e+277 rem 1e-311` would need ≈ 589 digits,
        // far beyond U256. The integer quotient is 10^588, which is
        // well over PRECISION=34 digits, so dec-spec
        // Division_impossible applies and the early-return is the
        // semantically correct answer.
        let big = Decimal128::parse_str("1E+277", crate::RoundingMode::NearestEven)
            .expect("parse")
            .0;
        let tiny = Decimal128::parse_str("1E-311", crate::RoundingMode::NearestEven)
            .expect("parse")
            .0;
        let (r, s) = big.rem(tiny);
        assert!(r.is_nan(), "expected NaN, got {r}");
        assert!(s.invalid(), "expected INVALID, got {s:?}");
    }

    #[cfg(feature = "fmt")]
    #[test]
    fn rem_division_impossible_in_band() {
        // The in-band check (integer quotient too wide despite fitting
        // U256) covers the same dec-spec condition for less-extreme
        // ratios. Mirrors dqrmn772.
        let x = Decimal128::parse_str(
            "1234500000000000000000067890123456",
            crate::RoundingMode::NearestEven,
        )
        .expect("parse")
        .0;
        let y = Decimal128::parse_str("0.1", crate::RoundingMode::NearestEven)
            .expect("parse")
            .0;
        let (r, s) = x.rem(y);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_zero_dividend_preserves_sign() {
        let (r, _) = Decimal128::ZERO.rem(d_int(7));
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.rem(d_int(7));
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }
}
