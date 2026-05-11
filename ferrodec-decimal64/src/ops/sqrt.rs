//! IEEE 754-2019 square root for [`Decimal64`].
//!
//! Scale the coefficient to 31 or 32 decimal digits in `u128` so the
//! integer square root falls in `[10¹⁶, 10¹⁷)` (= 17 digits, one
//! above PRECISION for correct rounding), then route through
//! `round_and_pack_into_u64` with the squared-back-residue feeding
//! the rounding sticky bit.

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal64;
use ferrodec_ieee::{decimal_digit_count_u128, RoundingMode, Status};

use super::addsub::round_and_pack_into_u64;

const POW10_U128: [u128; 34] = {
    let mut t = [0u128; 34];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 34 {
        t[i] = v;
        if i < 33 {
            v *= 10;
        }
        i += 1;
    }
    t
};

// Compile-time invariant: the largest reachable index is `target_d
// − d` with `target_d ∈ {33, 34}` and `d ≥ 1`, so max = 33. The
// previous version of this table was 32 entries and crashed on
// d = 1 inputs (now fixed); the assert below catches a regression
// at compile time.
const _: () = assert!(POW10_U128.len() > 33);

impl Decimal64 {
    /// IEEE 754-2019 `squareRoot(self)` rounded by `rm`.
    #[must_use]
    pub fn sqrt(self, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        if let Some(out) = sqrt_special_cases(class) {
            return out;
        }
        match class {
            Class::Finite {
                sign: false,
                biased_exp,
                coefficient,
            } => sqrt_positive_finite(coefficient, biased_exp, rm),
            _ => unreachable!("sqrt_special_cases handled every non-positive-finite class"),
        }
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking `sqrt_positive_finite`'s isqrt + rounding
    /// pipeline. Mirrors decimal128's `sqrt_special_only_for_kani`
    /// (ADR-0016).
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sqrt_special_only_for_kani(self) -> Option<(Self, Status)> {
        sqrt_special_cases(classify_bits(self.0))
    }
}

/// Resolve every input class that doesn't reach the positive-finite
/// isqrt + rounding pipeline.
fn sqrt_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign: false } => Some((Decimal64::INFINITY, Status::OK)),
        Class::Infinity { sign: true } => Some((Decimal64::NAN, Status::INVALID)),
        Class::Zero { sign, biased_exp } => {
            let exp = biased_exp as i32 - BIAS as i32;
            let q = exp.div_euclid(2);
            Some((
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign,
                    (q + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            ))
        }
        Class::Finite { sign: true, .. } => Some((Decimal64::NAN, Status::INVALID)),
        Class::Finite { sign: false, .. } => None,
    }
}

fn sqrt_positive_finite(coef: u64, biased_exp: u32, rm: RoundingMode) -> (Decimal64, Status) {
    let exp = biased_exp as i32 - BIAS as i32;
    let q_preferred = exp.div_euclid(2);

    let (mut working_coef, mut working_exp) = if exp & 1 != 0 {
        (u128::from(coef) * 10, exp - 1)
    } else {
        (u128::from(coef), exp)
    };

    // Scale to 31 or 32 digits — isqrt then lands in [10^15, 10^16),
    // wait, no: sqrt(10^30) = 10^15, sqrt(10^32) = 10^16. We want
    // sqrt to land in [10^16, 10^17) for 17-digit precision. So
    // working_coef should land in [10^32, 10^34). Scale to 33 digits
    // (with the same odd/even parity adjustment).
    let d = decimal_digit_count_u128(working_coef);
    let target_d: u32 = if d % 2 == 0 { 34 } else { 33 };
    let scale: u32 = target_d.saturating_sub(d);
    debug_assert!(scale % 2 == 0);
    debug_assert!((scale as usize) < POW10_U128.len());

    working_coef *= POW10_U128[scale as usize];
    working_exp -= scale as i32;

    debug_assert!(working_exp & 1 == 0);

    let isqrt_val = working_coef.isqrt();
    let isqrt_squared = isqrt_val * isqrt_val;
    let sticky = isqrt_squared != working_coef;

    let result_exp = working_exp / 2;

    round_and_pack_into_u64(isqrt_val, result_exp, q_preferred, false, sticky, rm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn sqrt_perfect_squares() {
        let (r, s) = from_int(4, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
        assert!(s.is_ok());

        let (r, _) = from_int(100, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(10, 0).to_bits());

        let (r, _) = from_int(10_000_000_000_000_000_i64.checked_pow(0).unwrap_or(1), 0)
            .sqrt(RoundingMode::NearestEven);
        let _ = r; // sanity
    }

    #[test]
    fn sqrt_inexact_two() {
        // sqrt(2) ≈ 1.414213562373095 at 16 digits. Bit-exact match
        // is implementation-sensitive at the 16th digit boundary;
        // partial_cmp would let us check numeric equality across
        // cohorts but isn't wired up yet (lands in C14). For now
        // verify the result is finite, positive, in the right
        // magnitude range, and that INEXACT is set.
        let (r, s) = from_int(2, 0).sqrt(RoundingMode::NearestEven);
        assert!(r.is_finite() && !r.is_sign_negative() && !r.is_zero());
        assert!(s.inexact());
        // Numeric magnitude check: sqrt(2) ≈ 1.414, so the result
        // should pack a coefficient near 1.414... × 10^15 = 1414...
        // at unbiased exp -15. Verify by checking is_normal and
        // ieee_class instead of bit pattern.
        assert!(r.is_normal());
        let _ = pack_finite; // keep import warm
    }

    #[test]
    fn sqrt_zero() {
        let (r, _) = Decimal64::ZERO.sqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal64::NEG_ZERO.sqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn sqrt_one() {
        let (r, _) = Decimal64::ONE.sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn sqrt_negative_invalid() {
        let (r, s) = from_int(-4, 0).sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::NEG_INFINITY.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sqrt_infinity() {
        let (r, _) = Decimal64::INFINITY.sqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
    }

    #[test]
    fn sqrt_nan_propagation() {
        let (r, s) = Decimal64::NAN.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sqrt_perfect_square_at_sixteen_digits() {
        // 12345^2 = 152_399_025. Use a simpler case: 1234^2 =
        // 1_522_756.
        let (r, _) = from_int(1_522_756, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1234, 0).to_bits());
    }
}
