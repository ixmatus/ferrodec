//! Truncated remainder for [`Decimal64`].
//!
//! `rem(a, b) = a − trunc(a / b) × b`. Result has sign of dividend
//! and magnitude < |b|. Result quantum = `min(Q(a), Q(b))` per
//! IEEE 754-2019 §5.3.1. Returns `(NaN, INVALID)` when the integer
//! quotient would exceed `PRECISION` (= 16) digits or when an operand
//! makes the operation undefined.

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS, COEFFICIENT_LIMIT};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

const POW10_U128: [u128; 39] = {
    let mut t = [0u128; 39];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 39 {
        t[i] = v;
        if i < 38 {
            v *= 10;
        }
        i += 1;
    }
    t
};

/// Per-side alignment cap: `coef × 10^shift` must fit in `u128`. The
/// maximum decimal digit count of a `u128` is 38 (`10^38 < 2^128 <
/// 10^39`), so a coefficient with `d` digits can shift up by at most
/// `U128_DIGIT_CAP - d` decimal positions.
const U128_DIGIT_CAP: u32 = 38;

// Compile-time invariant: the largest reachable index is
// `U128_DIGIT_CAP = 38`. The table needs ≥ 39 entries.
const _: () = assert!(POW10_U128.len() > U128_DIGIT_CAP as usize);

impl Decimal64 {
    /// Truncated-quotient remainder: `rem(a, b) = a − trunc(a / b) × b`.
    ///
    /// This is the GDA / C99 `fmod` remainder: the integer quotient
    /// rounds toward zero, so the result has the sign of `a` and
    /// magnitude `< |b|`. It is the decTest `remainder` op, the
    /// truncated variant. The IEEE 754-2019 §5.3.1 nearest-even
    /// remainder (`|r| ≤ |b|/2`, decTest `remaindernear`) is a
    /// *different* operation, exposed separately as
    /// [`Decimal64::rem_near`]; `Decimal128::rem` is the nearest-even
    /// one while this sibling `rem` is the truncated one, a deliberate
    /// per-format choice and a documented API hazard (ADR-0027). For
    /// rule-stable code prefer the explicit `rem_near` / the truncated
    /// `rem`; ADR-0027 records the 2.0 plan to retire the bare,
    /// ambiguous spelling.
    ///
    /// `rm` is unused: a truncated remainder is exact (its magnitude
    /// is strictly below `|b|` and it shares the dividend's quantum
    /// floor), so no rounding ever occurs. The parameter is retained
    /// only so the signature matches the other binary operations.
    #[must_use]
    pub fn rem(self, other: Self, _rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        if coef_a == 0 {
            // target_q = min(exp_a, exp_b) is within the classify_bits
            // range; biased conversion is in range.
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        // H5 fix (Phase 1 Agent 2 M2): per-side alignment bound is
        // dynamic, not the static `MAX_SAFE_SHIFT = 22` the pre-1.4.0
        // code used. The static bound conflated "aligning into u128
        // overflows" with "quotient digit count exceeds PRECISION";
        // the spec test (IEEE 754-2019 §5.4.2 "Division_impossible")
        // is the latter. With dynamic bounds, cases like
        // `rem(1E+25, 9999999999999999)` — quotient ≈ 1E+9, well
        // inside PRECISION digits, but `shift_a = 25` — now succeed.
        //
        // The alignment-overflow case `shift_a > ab_safe_shift`
        // implies `D_a + shift_a > U128_DIGIT_CAP`, i.e.
        // `D_a + shift_a > 38`. With `D_b ≤ 16`, this gives
        // `D_a + shift_a − D_b ≥ 22 > PRECISION`, so the quotient
        // digit count exceeds PRECISION and `INVALID` is spec-correct.
        // The exact digit-count check for in-range alignments is
        // already handled by the `quotient >= COEFFICIENT_LIMIT`
        // test below.
        let d_a = decimal_digit_count(coef_a);
        let d_b = decimal_digit_count(coef_b);
        let ab_safe_shift = U128_DIGIT_CAP - d_a;
        let bb_safe_shift = U128_DIGIT_CAP - d_b;

        if shift_a > ab_safe_shift {
            return (Decimal64::NAN, Status::INVALID);
        }

        if shift_b > bb_safe_shift {
            // |b| at target_q exceeds u128, which (by the same
            // bound argument) means |b| >> |a| at target_q. The
            // truncated quotient is 0 and rem(a, b) = a.
            let biased_exp =
                crate::bid::BiasedExp::try_from_unbiased(exp_a).expect("exp_a from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coef_a)
                .expect("coef_a < COEFFICIENT_LIMIT from classify_bits");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
                Status::OK,
            );
        }

        let aligned_a = u128::from(coef_a) * POW10_U128[shift_a as usize];
        let aligned_b = u128::from(coef_b) * POW10_U128[shift_b as usize];
        debug_assert!(aligned_b > 0);

        let quotient = aligned_a / aligned_b;
        if quotient >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal64::NAN, Status::INVALID);
        }
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        if residue >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal64::NAN, Status::INVALID);
        }

        // target_q is bounded by min of two classify_bits-derived exponents,
        // residue < COEFFICIENT_LIMIT checked above.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
            .expect("target_q from classify_bits-derived exponents");
        let coefficient = crate::bid::Coefficient::try_new(residue as u64)
            .expect("residue < COEFFICIENT_LIMIT checked above");
        (
            Decimal64::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.1 nearest-even remainder:
    /// `rem_near(a, b) = a − n·b` where `n` is `a / b` rounded to the
    /// nearest integer, ties to even.
    ///
    /// Result magnitude `≤ |b| / 2`; always exact when defined; never
    /// raises `INEXACT`. This mirrors `Decimal128::rem` (the parent's
    /// `rem` is this nearest-even op), and is distinct from this
    /// crate's truncated [`Decimal64::rem`]. The decTest spelling is
    /// `remaindernear`. ADR-0027 records why both spellings exist and
    /// the 2.0 plan to make the explicit name the only one.
    ///
    /// Special cases follow IEEE 754-2019 §5.3.1 exactly as the
    /// truncated `rem` does (they share `handle_specials`): NaN
    /// propagation, sNaN → `INVALID`, `a / 0` and `±∞ / b` →
    /// `NaN + INVALID`, `a / ±∞` → `a`, `±0` → `±0` with the sign of
    /// `a`. `Division_impossible` (the truncated integer quotient
    /// exceeds `PRECISION` digits) → `NaN + INVALID`.
    #[must_use]
    pub fn rem_near(self, other: Self) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        if coef_a == 0 {
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        // Same dynamic per-side alignment bounds as the truncated
        // `rem` (the H5 fix): an overflowing `shift_a` means the
        // integer quotient already exceeds `PRECISION` digits, so
        // `Division_impossible` / `INVALID` is spec-correct.
        let d_a = decimal_digit_count(coef_a);
        let d_b = decimal_digit_count(coef_b);
        let ab_safe_shift = U128_DIGIT_CAP - d_a;
        let bb_safe_shift = U128_DIGIT_CAP - d_b;

        if shift_a > ab_safe_shift {
            return (Decimal64::NAN, Status::INVALID);
        }

        if shift_b > bb_safe_shift {
            // |b| at target_q overflows u128, so |b| ≫ |a|, so
            // |a / b| ≪ 1/2: the nearest integer is 0 and the
            // remainder is exactly `a` (same answer as truncated).
            let biased_exp =
                crate::bid::BiasedExp::try_from_unbiased(exp_a).expect("exp_a from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coef_a)
                .expect("coef_a < COEFFICIENT_LIMIT from classify_bits");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
                Status::OK,
            );
        }

        let aligned_a = u128::from(coef_a) * POW10_U128[shift_a as usize];
        let aligned_b = u128::from(coef_b) * POW10_U128[shift_b as usize];
        debug_assert!(aligned_b > 0);

        let quotient = aligned_a / aligned_b;
        // `Division_impossible` keys off the *truncated* integer
        // quotient digit count (GDA / the exact oracle), so the test
        // is the same as the truncated `rem`'s.
        if quotient >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal64::NAN, Status::INVALID);
        }
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        // Adjust the truncated quotient to nearest-even: round the
        // integer quotient up by one when the residue is past
        // `aligned_b / 2`, ties to even on the quotient's low bit.
        let round_up = compare_remainder_to_half(residue, aligned_b, (quotient & 1) as u32);
        let (mut result_mag, sign_flip) = if round_up {
            (aligned_b - residue, true)
        } else {
            (residue, false)
        };

        if result_mag == 0 {
            // Exact multiple: ±0 with the sign of `a`, at target_q.
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        // The IEEE remainder is exactly representable when the
        // operands are, so any coefficient above `PRECISION` digits is
        // made of trailing zeros: shift them out, raising the quantum
        // (mirrors `Decimal128::rem`'s re-encode normalisation).
        let result_sign = sign_a ^ sign_flip;
        let mut q_unbiased = target_q;
        while result_mag >= u128::from(COEFFICIENT_LIMIT) {
            debug_assert!(
                result_mag % 10 == 0,
                "rem_near result not exactly representable"
            );
            result_mag /= 10;
            q_unbiased += 1;
        }
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_unbiased)
            .expect("rem_near quantum stays in range (|r| ≤ |b|/2)");
        let coefficient = crate::bid::Coefficient::try_new(result_mag as u64)
            .expect("result_mag < COEFFICIENT_LIMIT after trailing-zero normalisation");
        (
            Decimal64::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                coefficient,
            )),
            Status::OK,
        )
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the finite-finite quotient pipeline. Mirrors
    /// decimal128's `rem_special_only_for_kani` (ADR-0016).
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn rem_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        handle_specials(classify_bits(self.0), classify_bits(rhs.0))
    }
}

/// `residue` versus `aligned_b / 2`, ties broken by the integer
/// quotient's low bit (round half to even). Returns `true` when the
/// truncated quotient should be rounded up by one. Mirrors the
/// `Decimal128` `rem` half-even comparison.
fn compare_remainder_to_half(residue: u128, aligned_b: u128, quotient_lsb: u32) -> bool {
    if residue == 0 {
        return false;
    }
    let (two_r, overflow) = residue.overflowing_mul(2);
    if overflow {
        // 2·residue > u128::MAX ≥ aligned_b, so residue > aligned_b / 2.
        return true;
    }
    match two_r.cmp(&aligned_b) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => quotient_lsb == 1,
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal64, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if matches!(a, Infinity { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(b, Zero { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(b, Infinity { .. }) {
        if let Finite {
            sign,
            biased_exp,
            coefficient,
        } = a
        {
            let biased_exp = crate::bid::BiasedExp::try_from_biased(biased_exp)
                .expect("biased_exp from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coefficient)
                .expect("coefficient from classify_bits");
            return Some((
                Decimal64::from_bits(crate::bid::pack_finite(sign, biased_exp, coefficient)),
                Status::OK,
            ));
        }
        if let Zero { sign, biased_exp } = a {
            let biased_exp = crate::bid::BiasedExp::try_from_biased(biased_exp)
                .expect("biased_exp from classify_bits");
            return Some((
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn rem_basic() {
        let (r, _) = from_int(10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        let (r, _) = from_int(10, 0).rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(-10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());
    }

    #[test]
    fn rem_zero_dividend() {
        let (r, _) = Decimal64::ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn rem_near_nearest_even_quotient() {
        // 7.5 rem_near 2.0: round_half_even(3.75) = 4, 7.5 − 4·2 = -0.5
        // (the documented Decimal128::rem example; the sibling now
        // matches it).
        let (r, s) = from_int(75, -1).rem_near(from_int(20, -1));
        assert_eq!(r.to_bits(), from_int(-5, -1).to_bits());
        assert!(s.is_ok());

        // 10 rem_near 3 = 1 (10/3 ≈ 3.33, n = 3).
        let (r, _) = from_int(10, 0).rem_near(from_int(3, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        // -10 rem_near 3 = -1.
        let (r, _) = from_int(-10, 0).rem_near(from_int(3, 0));
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());

        // |b| ≫ |a|: 1 rem_near 10 = 1 (n = 0).
        let (r, _) = from_int(1, 0).rem_near(from_int(10, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());
    }

    #[test]
    fn rem_near_half_tie_rounds_to_even_quotient() {
        // Exact half quotient ties to the even integer quotient, so
        // the parity of trunc(a/b) flips the sign of the result.
        // 10/4 = 2.5  → n = 2 (even)  → 10 − 8  =  2
        let (r, _) = from_int(10, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
        // 30/4 = 7.5  → n = 8 (even)  → 30 − 32 = -2
        let (r, _) = from_int(30, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(-2, 0).to_bits());
        // 50/4 = 12.5 → n = 12 (even) → 50 − 48 =  2
        let (r, _) = from_int(50, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
    }

    #[test]
    fn rem_near_exact_multiple_keeps_dividend_sign() {
        // -8 rem_near 4 = -0 (exact multiple; sign of the dividend).
        let (r, s) = from_int(-8, 0).rem_near(from_int(4, 0));
        assert!(r.is_zero() && r.is_sign_negative());
        assert!(s.is_ok());

        // 0 rem_near 5 = +0.
        let (r, _) = Decimal64::ZERO.rem_near(from_int(5, 0));
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn rem_near_special_cases_match_truncated() {
        // Shares handle_specials with `rem`, so the special lattice is
        // identical: x / 0 and ∞ / y → NaN+INVALID, x / ∞ → x.
        let (r, s) = from_int(1, 0).rem_near(Decimal64::ZERO);
        assert!(r.is_quiet_nan() && s.invalid());
        let (r, s) = Decimal64::INFINITY.rem_near(from_int(1, 0));
        assert!(r.is_quiet_nan() && s.invalid());
        let (r, s) = from_int(7, 0).rem_near(Decimal64::INFINITY);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
        assert!(s.is_ok());
        let (r, s) = Decimal64::SIGNALING_NAN.rem_near(from_int(1, 0));
        assert!(r.is_quiet_nan() && s.invalid());
    }

    #[test]
    fn rem_h5_large_exponent_gap_quotient_in_precision() {
        // H5 regression (Phase 1 Agent 2 M2): `rem(1E+25, 10^16 - 1)`
        // has `shift_a = 25 > old MAX_SAFE_SHIFT = 22`, but the
        // truncated quotient (≈ 10^9, 10 digits) fits well inside
        // PRECISION = 16. The pre-1.4.0 code returned `(NaN, INVALID)`;
        // the spec answer is the truncated remainder `1E+9`.
        //
        // Quotient: floor(10^25 / (10^16 - 1)) = 10^9 (with tiny
        // residue from the `(1 + 10^-16)` factor).
        // Remainder: 10^25 − 10^9 × (10^16 − 1) = 10^9.
        let a = Decimal64::try_new(1, 25).unwrap();
        let b = Decimal64::try_new(9_999_999_999_999_999, 0).unwrap();
        let (r, status) = a.rem(b, RoundingMode::NearestEven);
        let expected = Decimal64::try_new(1_000_000_000, 0).unwrap();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "rem(1E+25, 10^16 - 1) should equal 1E+9, got {r:?}"
        );
        assert!(
            status.is_ok(),
            "rem is exact, expected no flags, got {status:?}"
        );

        // Genuinely-impossible quotient (10^25 / 1 = 10^25, 26 digits)
        // still raises INVALID as expected.
        let one = Decimal64::try_new(1, 0).unwrap();
        let (r, status) = a.rem(one, RoundingMode::NearestEven);
        assert!(
            r.is_quiet_nan(),
            "rem(1E+25, 1) quotient has 26 digits, expected NaN"
        );
        assert!(status.invalid());
    }

    #[test]
    fn rem_by_zero_invalid() {
        let (r, s) = from_int(5, 0).rem(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_infinity() {
        let (r, s) = Decimal64::INFINITY.rem(from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, _) = from_int(7, 0).rem(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
    }

    #[test]
    fn rem_too_large_quotient_invalid() {
        let (r, s) = Decimal64::MAX.rem(Decimal64::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_dividend_smaller_than_divisor() {
        let (r, _) = from_int(3, 0).rem(from_int(10, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());
    }

    #[test]
    fn rem_nan_propagation() {
        let (r, s) = Decimal64::NAN.rem(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.rem(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
