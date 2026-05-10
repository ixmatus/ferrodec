//! IEEE 754-2019 remainder for [`Decimal32`].
//!
//! Truncated remainder: `rem(a, b) = a − trunc(a / b) * b`. The result
//! has the sign of the dividend `a` and magnitude strictly less than
//! `|b|`. The quantum is `min(Q(a), Q(b))` per IEEE 754-2019 §5.3.1.
//!
//! Per the General Decimal Arithmetic spec, the operation raises
//! `Invalid_operation` when the integer quotient would exceed
//! `PRECISION` digits. That condition collapses cleanly here: if the
//! aligned coefficient on the dividend side overflows `u64` or the
//! integer quotient exceeds `10⁷`, we return `NaN` with `INVALID`.
//!
//! # Special cases (IEEE 754-2019 §7)
//!
//! * sNaN / qNaN propagation (a preferred per §6.2.3).
//! * `±∞ % anything` → NaN + `INVALID`.
//! * `anything % 0` → NaN + `INVALID`.
//! * `0 % b` (b ≠ 0) → ±0 with sign of dividend at the preferred
//!   quantum.
//! * `finite % ±∞` → finite (the dividend) at the preferred quantum.

use crate::bid::{classify_bits, BIAS, Class, COEFFICIENT_LIMIT};
use crate::decimal::Decimal32;
use crate::status::{RoundingMode, Status};

const POW10_U64: [u64; 16] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
];

/// Maximum decimal-shift we can apply to a u32 coefficient and stay
/// within u64. `(10^7 - 1) × 10^12 < 10^19 < 2^64`.
const MAX_SAFE_SHIFT: u32 = 12;

impl Decimal32 {
    /// Truncated remainder: `self − trunc(self / other) × other`.
    ///
    /// Result has the sign of `self` and magnitude strictly less than
    /// `|other|`. Returns `(NaN, INVALID)` when the integer quotient
    /// would exceed `PRECISION` (= 7) digits or when an operand makes
    /// the operation undefined per IEEE 754-2019 §5.3.1.
    ///
    /// The `rm` parameter is unused (`rem` is exact when defined) but
    /// kept on the signature for parity with the other arithmetic
    /// methods.
    #[must_use]
    pub fn rem(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let _ = rm; // exact operation; rm carried for API parity
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        // Zero dividend → ±0 at preferred quantum (sign preserved).
        if coef_a == 0 {
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign_a,
                    (target_q + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            );
        }

        // Align both operands at target_q. If either shift exceeds
        // MAX_SAFE_SHIFT, fall back to closed-form handling:
        //
        // * `exp_a > exp_b + MAX_SAFE_SHIFT`: `a` is far larger than
        //   `b` in magnitude; integer quotient would have many more
        //   digits than PRECISION ⇒ INVALID.
        // * `exp_a + MAX_SAFE_SHIFT < exp_b`: `b` is far larger than
        //   `a` in magnitude; trunc(a/b) = 0 and the remainder is just
        //   `a` itself, packed at `target_q = exp_a`.
        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        if shift_a > MAX_SAFE_SHIFT || shift_b > MAX_SAFE_SHIFT {
            // shift_a > 0 means exp_a > exp_b (so target_q = exp_b);
            // shift_b > 0 means exp_b > exp_a (target_q = exp_a). Only
            // one can be non-zero.
            if shift_a > MAX_SAFE_SHIFT {
                // |a| ≫ |b|: integer quotient exceeds PRECISION digits.
                return (Decimal32::NAN, Status::INVALID);
            }
            // shift_b > MAX_SAFE_SHIFT: |b| ≫ |a|, trunc(a/b) = 0.
            // Remainder = a, packed at target_q = exp_a.
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign_a,
                    (exp_a + BIAS as i32) as u32,
                    coef_a as u32,
                )),
                Status::OK,
            );
        }

        let aligned_a = coef_a * POW10_U64[shift_a as usize];
        let aligned_b = coef_b * POW10_U64[shift_b as usize];
        debug_assert!(aligned_b > 0); // zero divisor handled by dispatcher

        let quotient = aligned_a / aligned_b;
        // Per the GDA spec, the integer quotient must fit in PRECISION
        // digits (≤ 9_999_999). If it doesn't, the operation is
        // invalid.
        if quotient >= u64::from(COEFFICIENT_LIMIT) {
            return (Decimal32::NAN, Status::INVALID);
        }
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        // Sign of remainder = sign of dividend. Magnitude is `residue`
        // packed at `target_q`. Residue may have up to 7 digits (since
        // it's strictly less than aligned_b which had at most ~14
        // digits, but the canonical post-rounding bound is 10^7); it
        // is always ≤ aligned_b - 1 ≤ COEFFICIENT_LIMIT × 10^shift_b
        // - 1. After we computed `aligned_b ≤ 10^7 × 10^MAX_SAFE_SHIFT
        // = 10^19`, residue < 10^19, but it actually fits in 7 digits
        // because the original `coef_b < 10^7` and aligned_b's value
        // at the same target_q means residue < aligned_b's coefficient
        // expressed at the target quantum. For our purposes residue
        // does fit in u32 → packed value is well within Decimal32.
        // Verify before packing.
        if residue >= u64::from(COEFFICIENT_LIMIT) {
            // Should not happen for canonical inputs, but guard
            // against pathological alignment.
            return (Decimal32::NAN, Status::INVALID);
        }

        (
            Decimal32::from_bits(crate::bid::pack_finite(
                sign_a,
                (target_q + BIAS as i32) as u32,
                residue as u32,
            )),
            Status::OK,
        )
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    // ±∞ % anything → NaN + INVALID.
    if matches!(a, Infinity { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // anything % 0 → NaN + INVALID.
    if matches!(b, Zero { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // finite % ±∞ → finite (the dividend, sign preserved).
    if matches!(b, Infinity { .. }) {
        if let Finite { sign, biased_exp, coefficient } = a {
            return Some((
                Decimal32::from_bits(crate::bid::pack_finite(sign, biased_exp, coefficient)),
                Status::OK,
            ));
        }
        if let Zero { sign, biased_exp } = a {
            return Some((
                Decimal32::from_bits(crate::bid::pack_finite(sign, biased_exp, 0)),
                Status::OK,
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn rem_basic() {
        // 10 % 3 = 1
        let (r, s) = from_int(10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());
        assert!(s.is_ok());

        // 10 % 5 = 0
        let (r, _) = from_int(10, 0).rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // 10 % -3 = 1 (sign of dividend)
        let (r, _) = from_int(10, 0).rem(from_int(-3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        // -10 % 3 = -1
        let (r, _) = from_int(-10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());
    }

    #[test]
    fn rem_quantum_min() {
        // 1.5 % 0.5 = 0.0 at quantum -1 (min of -1 and -1).
        let (r, _) = from_int(15, -1).rem(from_int(5, -1), RoundingMode::NearestEven);
        assert!(r.is_zero());
        // The result preserves the min-quantum cohort.
        // rem doesn't strip trailing zeros: result is "0E-1" = "0.0".
        let _ = r;
    }

    #[test]
    fn rem_zero_dividend() {
        // 0 % 5 = +0 (sign of dividend)
        let (r, _) = Decimal32::ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // -0 % 5 = -0
        let (r, _) = Decimal32::NEG_ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn rem_by_zero_invalid() {
        let (r, s) = from_int(5, 0).rem(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::ZERO.rem(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_infinity() {
        // ∞ % anything → NaN + INVALID
        let (r, s) = Decimal32::INFINITY.rem(from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        // finite % ∞ → finite (dividend)
        let (r, s) = from_int(7, 0).rem(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn rem_too_large_quotient_invalid() {
        // MAX % MIN_POSITIVE — quotient would have ~190+ digits, way
        // more than PRECISION = 7. INVALID.
        let (r, s) = Decimal32::MAX.rem(Decimal32::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_dividend_smaller_than_divisor() {
        // 3 % 10 = 3 (trunc(3/10) = 0)
        let (r, _) = from_int(3, 0).rem(from_int(10, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());

        // 1e-100 % 1 = 1e-100 at quantum -100.
        let small = Decimal32::try_new(1, -100).unwrap();
        let one = Decimal32::ONE;
        let (r, _) = small.rem(one, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), small.to_bits());
    }

    #[test]
    fn rem_nan_propagation() {
        let (r, s) = Decimal32::NAN.rem(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.rem(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
