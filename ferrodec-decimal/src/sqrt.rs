//! Correctly-rounded square root.
//!
//! The square root is computed exactly as an integer square root and then
//! rounded once. To get `precision + 1` significant digits of the root (a
//! guard digit for the rounding core), the coefficient is scaled by an even
//! power of ten before [`DecBig::isqrt`]; the exact unsquared remainder is
//! non-zero exactly when the true root has a fractional part, so it feeds the
//! sticky bit. Halfway ties therefore arise only when the scaled value is a
//! perfect square landing on a half-digit, which the rounding core resolves to
//! even.
//!
//! Per the specification, `squareRoot` always rounds half-even regardless of
//! the context's rounding mode, and its ideal exponent is `floor(exponent /
//! 2)`.

use crate::arith::{invalid_nan, nan_unary};
use crate::round::round_finite;
use crate::{Context, Decimal, Rounding, Status};
use ferrodec_multiword::DecBig;

impl Decimal {
    /// The correctly-rounded square root of `self` under `ctx`.
    #[must_use]
    pub fn sqrt(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }

        // The square root always rounds half-even, independent of the context.
        let sqrt_ctx = Context {
            rounding: Rounding::HalfEven,
            ..*ctx
        };

        if self.is_zero() {
            // The square root of a signed zero is that zero; ideal exponent
            // floor(eo / 2).
            let (sign, _, eo) = self.finite_parts().expect("finite");
            let ideal = i64::from(eo).div_euclid(2);
            return round_finite(
                sign,
                DecBig::zero(),
                ideal,
                false,
                ideal,
                &sqrt_ctx,
                Status::OK,
            );
        }
        if self.is_negative() {
            // The square root of any negative value (including -Infinity) is
            // undefined.
            return (invalid_nan(), Status::INVALID);
        }
        if self.is_infinite() {
            return (Decimal::infinity(false), Status::OK);
        }

        let (_, coeff, eo) = self.finite_parts().expect("finite");
        // Normalize the exponent to even, folding an odd unit into the
        // coefficient so the value is unchanged.
        let (mut cc, mut e) = (coeff.clone(), i64::from(eo));
        if e & 1 != 0 {
            cc = cc.mul_pow10(1);
            e -= 1;
        }

        // Scale by an even power of ten so the integer root has at least
        // precision + 1 digits.
        let p = i64::from(ctx.precision);
        let dc = cc.decimal_digit_count() as i64;
        let mut twos = (2 * p + 2 - dc).max(0);
        if twos & 1 != 0 {
            twos += 1;
        }
        let n = cc.mul_pow10(twos as u32);
        let (root, rem) = n.isqrt();

        let result_exp = e / 2 - twos / 2;
        let ideal = i64::from(eo).div_euclid(2);
        round_finite(
            false,
            root,
            result_exp,
            !rem.is_zero(),
            ideal,
            &sqrt_ctx,
            Status::OK,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn ctx(precision: u32) -> Context {
        Context::new(precision, 9999, -9999, Rounding::HalfEven)
    }

    fn fin(coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(false, DecBig::from_u128(coeff), exp)
    }

    #[test]
    fn perfect_squares_and_ideal_exponent() {
        let c = ctx(9);
        // sqrt(9) = 3.
        assert_eq!(fin(9, 0).sqrt(&c).0, fin(3, 0));
        // sqrt(4.00) = 2.0 (ideal exponent floor(-2/2) = -1).
        assert_eq!(fin(400, -2).sqrt(&c).0.to_string(), "2.0");
        // sqrt(1) = 1.
        assert_eq!(fin(1, 0).sqrt(&c).0, fin(1, 0));
    }

    #[test]
    fn irrational_rounds_half_even() {
        let (r, s) = fin(2, 0).sqrt(&ctx(9));
        assert_eq!(r.to_string(), "1.41421356");
        assert!(s.inexact());
    }

    #[test]
    fn sqrt_ignores_context_rounding() {
        // Even under round-up, the square root rounds half-even.
        let up = Context::new(9, 9999, -9999, Rounding::Up);
        assert_eq!(fin(2, 0).sqrt(&up).0.to_string(), "1.41421356");
    }

    #[test]
    fn signed_zero_and_specials() {
        let c = ctx(9);
        let neg_zero = Decimal::finite(true, DecBig::zero(), 0);
        let (rz, _) = neg_zero.sqrt(&c);
        assert!(rz.is_zero() && rz.is_negative());
        // sqrt(+Inf) = +Inf; sqrt(-1) is invalid.
        assert!(Decimal::infinity(false).sqrt(&c).0.is_infinite());
        let (neg, st2) = Decimal::finite(true, DecBig::from_u32(1), 0).sqrt(&c);
        assert!(neg.is_nan() && st2.invalid());
    }
}
