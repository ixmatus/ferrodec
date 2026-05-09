//! `sin(x)` and `cos(x)`.
//!
//! ## Algorithm
//!
//! 1. Special cases: NaN / sNaN propagate; `sin(±0) = ±0`;
//!    `cos(±0) = +1`; `sin(±∞) = cos(±∞) = NaN + INVALID`.
//! 2. Range reduction via Payne-Hanek (see [`super::argred`]):
//!    compute `k = round(|x| · 2/π) mod 4` and the residual `r` such
//!    that `|x| = k · π/2 + r` and `|r| ≤ π/4`. The reduction works
//!    across the full `Decimal128` magnitude range — there's no
//!    `|x| ≤ 10^9` cap. `r` is returned as an [`Extended`] so the
//!    Taylor body below sees ~38-40 digits of fractional residual,
//!    not just 34.
//! 3. Taylor series for `sin(r)` and `cos(r)` on `|r| ≤ π/4`,
//!    evaluated at `Extended` (50-digit) precision. Then rotate by
//!    `k mod 4`:
//!
//!    ```text
//!    k mod 4   sin(|x|)   cos(|x|)
//!    -------  --------   --------
//!         0    sin(r)     cos(r)
//!         1    cos(r)    -sin(r)
//!         2   -sin(r)    -cos(r)
//!         3   -cos(r)     sin(r)
//!    ```
//!
//!    `sin` is odd, so `sin(x) = -sin(|x|)` when `x < 0`. `cos` is
//!    even, so `cos(x) = cos(|x|)` regardless of sign.
//! 4. Round once to `Decimal128` at the end via
//!    [`Extended::to_decimal128`]. Result is faithfully rounded
//!    (≤ 1 ULP) against `astro-float`.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::argred;
use crate::math::extended::Extended;
use crate::ops::nan_from;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Sine, in radians.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => (self, Status::OK),
            Class::Infinity { .. } => (Decimal128::NAN, Status::INVALID),
            Class::Zero { sign, .. } => (
                if sign {
                    Decimal128::NEG_ZERO
                } else {
                    Decimal128::ZERO
                },
                Status::OK,
            ),
            Class::Finite { .. } => sincos_kernel(self, rm).0,
        }
    }

    /// Cosine, in radians.
    #[must_use]
    pub fn cos(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => (self, Status::OK),
            Class::Infinity { .. } => (Decimal128::NAN, Status::INVALID),
            Class::Zero { .. } => (Decimal128::ONE, Status::OK),
            Class::Finite { .. } => sincos_kernel(self, rm).1,
        }
    }

    /// Tangent, in radians.
    ///
    /// `tan(x) = sin(x) / cos(x)`, computed by dividing the two
    /// extended-precision sin/cos values before rounding to
    /// `Decimal128`. At `cos(x) = 0` (odd multiples of π/2) the
    /// result diverges; we return `±∞` without raising
    /// `DIV_BY_ZERO` (since `tan` of a finite input doesn't fit the
    /// IEEE 754 §7.3 division-by-zero condition — it's just an
    /// asymptote).
    #[must_use]
    pub fn tan(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::Zero { sign, .. } => {
                return (
                    if sign {
                        Decimal128::NEG_ZERO
                    } else {
                        Decimal128::ZERO
                    },
                    Status::OK,
                );
            }
            Class::Finite { .. } => {}
        }
        let (sin_ext, cos_ext, status_red) = sincos_extended(self);
        if cos_ext.is_zero() {
            // sin/cos at the asymptote: return ±∞ with the sign of sin.
            let sign = sin_ext.sign;
            return (
                if sign {
                    Decimal128::NEG_INFINITY
                } else {
                    Decimal128::INFINITY
                },
                status_red | Status::INEXACT,
            );
        }
        let tan_ext = sin_ext.div(cos_ext);
        let (tan_d, st) = tan_ext.to_decimal128(0, rm);
        (tan_d, st | status_red | Status::INEXACT)
    }
}

/// Compute both `(sin(x), status)` and `(cos(x), status)` from one
/// reduction. Returns them as `((sin, sin_status), (cos, cos_status))`.
fn sincos_kernel(x: Decimal128, rm: RoundingMode) -> ((Decimal128, Status), (Decimal128, Status)) {
    let (sin_x_ext, cos_x_ext, status_red) = sincos_extended(x);
    let (sin_d, sin_status) = sin_x_ext.to_decimal128(0, rm);
    let (cos_d, cos_status) = cos_x_ext.to_decimal128(0, rm);
    let status = status_red | Status::INEXACT;
    ((sin_d, sin_status | status), (cos_d, cos_status | status))
}

/// Compute `(sin(x), cos(x))` at `Extended` precision. Used directly
/// by the public `sin` / `cos` (after rounding) and by `tan(x) =
/// sin(x) / cos(x)` (which divides the two extended values before
/// rounding). Caller filters NaN / Inf / Zero.
pub(super) fn sincos_extended(x: Decimal128) -> (Extended, Extended, Status) {
    let neg = match classify_bits(x.to_bits()) {
        Class::Finite { sign, .. } => sign,
        _ => false,
    };
    let abs_x = if neg { x.neg() } else { x };

    let (k_mod_4, r, status_red) = argred::reduce(abs_x);
    let r_sq = r.square();
    let sin_r = taylor_sin_ext(r, r_sq);
    let cos_r = taylor_cos_ext(r_sq);

    let (sin_abs_ext, cos_abs_ext) = match k_mod_4 {
        0 => (sin_r, cos_r),
        1 => (cos_r, sin_r.neg()),
        2 => (sin_r.neg(), cos_r.neg()),
        3 => (cos_r.neg(), sin_r),
        _ => unreachable!(),
    };

    let sin_x_ext = if neg { sin_abs_ext.neg() } else { sin_abs_ext };
    (sin_x_ext, cos_abs_ext, status_red)
}

/// `sin(r) = r − r³/3! + r⁵/5! − …` for `|r| ≤ π/4`. Evaluated at
/// `Extended` precision; caller passes `r²` so it can be shared with
/// the cosine evaluation.
fn taylor_sin_ext(r: Extended, r_sq: Extended) -> Extended {
    let mut sum = r;
    let mut term = r;
    let mut alt = true; // next term subtracts.
                        // n indexes the term series (term_n = r^{2n-1} / (2n-1)!).
                        // Update: term_{n+1} = term_n · r² / ((2n)(2n+1)).
    let mut n: u32 = 1;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 2) * (2 * n - 1); // u32, fits up to n ≈ 32k
        term = term.mul(r_sq).div_u32(denom);
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let next_sum = sum.add(signed);
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if term.is_zero() {
            break;
        }
    }
    sum
}

/// `cos(r) = 1 − r²/2! + r⁴/4! − …` for `|r| ≤ π/4`.
fn taylor_cos_ext(r_sq: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    let mut alt = true; // next term subtracts.
    let mut n: u32 = 0;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 1) * (2 * n);
        term = term.mul(r_sq).div_u32(denom);
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let next_sum = sum.add(signed);
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if term.is_zero() {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::consts::pi;
    extern crate alloc;
    use alloc::format;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn approx_equal_ulps(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_b = b.abs();
        if abs_b.is_zero() {
            // Absolute tolerance for values near zero.
            let bound = parse(&format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
        let bound = parse(&format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    #[test]
    fn sin_zero_is_zero() {
        let (r, _) = Decimal128::ZERO.sin(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.sin(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn cos_zero_is_one() {
        let (r, _) = Decimal128::ZERO.cos(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn sin_pi_is_approximately_zero() {
        let (r, _) = pi().sin(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(r, Decimal128::ZERO, 100));
    }

    #[test]
    fn cos_pi_is_neg_one() {
        let (r, _) = pi().cos(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(r, Decimal128::NEG_ONE, 50));
    }

    #[test]
    fn sin_half_pi_is_one() {
        let (half_pi, _) = pi().div(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (s, _) = half_pi.sin(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(s, Decimal128::ONE, 50));
    }

    #[test]
    fn cos_half_pi_is_zero() {
        let (half_pi, _) = pi().div(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (c, _) = half_pi.cos(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(c, Decimal128::ZERO, 100));
    }

    #[test]
    fn pythagorean_identity() {
        // sin²(x) + cos²(x) = 1, for various x.
        for s in &["0.5", "1", "-1", "1.5", "3", "-2.7"] {
            let x = parse(s);
            let (sin_x, _) = x.sin(RoundingMode::NearestEven);
            let (cos_x, _) = x.cos(RoundingMode::NearestEven);
            let (sin_sq, _) = sin_x.mul(sin_x, RoundingMode::NearestEven);
            let (cos_sq, _) = cos_x.mul(cos_x, RoundingMode::NearestEven);
            let (sum, _) = sin_sq.add(cos_sq, RoundingMode::NearestEven);
            assert!(
                approx_equal_ulps(sum, Decimal128::ONE, 200),
                "sin²({s}) + cos²({s}) = {sum:?}, want ≈ 1"
            );
        }
    }

    #[test]
    fn sin_nan_propagates() {
        let (r, _) = Decimal128::NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn trig_qnan_preserves_payload_bit_for_bit() {
        // The qNaN arms of `sin`/`cos`/`tan` return `self` directly,
        // so the bit pattern (sign, signaling-flag, full 110-bit
        // payload) must come through unchanged. Pin this with a
        // distinctive payload so a future refactor that funnels qNaN
        // through `nan_from` (which canonicalises) would fail loud.
        let payload: u128 = 0x0000_DEAD_BEEF_CAFE_BA5E;
        let qnan = Decimal128::from_bits(crate::bid::pack_quiet_nan(true, payload));
        for &op in &[
            Decimal128::sin as fn(Decimal128, RoundingMode) -> (Decimal128, Status),
            Decimal128::cos,
            Decimal128::tan,
        ] {
            let (r, s) = op(qnan, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), qnan.to_bits(), "qNaN bits must pass through");
            assert!(!s.invalid(), "qNaN must not raise INVALID");
        }

        // sNaN gets quieted (signaling bit cleared) but the payload
        // bits should still survive — propagate_nan / nan_from
        // routes through `pack_quiet_nan(sign, payload)`. Verifies
        // the payload-preservation invariant on the sNaN side too.
        let snan = Decimal128::from_bits(crate::bid::pack_signaling_nan(false, payload));
        for &op in &[
            Decimal128::sin as fn(Decimal128, RoundingMode) -> (Decimal128, Status),
            Decimal128::cos,
            Decimal128::tan,
        ] {
            let (r, s) = op(snan, RoundingMode::NearestEven);
            assert!(r.is_nan() && r.is_quiet_nan(), "sNaN gets quieted");
            assert!(s.invalid(), "sNaN must raise INVALID");
            let r_payload = r.to_bits() & ((1u128 << 110) - 1);
            assert_eq!(r_payload, payload, "sNaN payload must survive");
        }
    }

    #[test]
    fn sin_inf_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
        let (r, s) = Decimal128::NEG_INFINITY.cos(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sin_neg_x_is_neg_sin_x() {
        // sin is odd. Compare bit-by-bit modulo small ULP for non-special x.
        for s in &["0.7", "1.1", "2.5"] {
            let x = parse(s);
            let (sin_x, _) = x.sin(RoundingMode::NearestEven);
            let (sin_neg, _) = x.neg().sin(RoundingMode::NearestEven);
            // sin(-x) ≈ -sin(x); allow ~10 ULP of drift.
            assert!(
                approx_equal_ulps(sin_neg, sin_x.neg(), 50),
                "sin(-{s}) symmetry"
            );
        }
    }

    #[test]
    fn cos_neg_x_is_cos_x() {
        for s in &["0.7", "1.1", "2.5"] {
            let x = parse(s);
            let (cos_x, _) = x.cos(RoundingMode::NearestEven);
            let (cos_neg, _) = x.neg().cos(RoundingMode::NearestEven);
            assert!(approx_equal_ulps(cos_neg, cos_x, 50), "cos(-{s}) symmetry");
        }
    }
}
