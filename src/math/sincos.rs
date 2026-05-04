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
//!    `|x| ≤ 10^9` cap.
//! 3. Taylor series for `sin(r)` and `cos(r)` on `|r| ≤ π/4`. Then
//!    rotate by `k mod 4`:
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

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::argred;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Sine, in radians.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => (Decimal128::NAN, Status::INVALID),
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
            Class::SignalingNaN { .. } => (Decimal128::NAN, Status::INVALID),
            Class::QuietNaN { .. } => (self, Status::OK),
            Class::Infinity { .. } => (Decimal128::NAN, Status::INVALID),
            Class::Zero { .. } => (Decimal128::ONE, Status::OK),
            Class::Finite { .. } => sincos_kernel(self, rm).1,
        }
    }
}

/// Compute both `(sin(x), status)` and `(cos(x), status)` from one
/// reduction. Returns them as `((sin, sin_status), (cos, cos_status))`.
fn sincos_kernel(
    x: Decimal128,
    rm: RoundingMode,
) -> ((Decimal128, Status), (Decimal128, Status)) {
    // Sign handling: reduce `|x|`, then flip the sin result for negative
    // inputs (sin is odd; cos is even).
    let neg = match classify_bits(x.to_bits()) {
        Class::Finite { sign, .. } => sign,
        _ => false,
    };
    let abs_x = if neg { x.neg() } else { x };

    let (k_mod_4, r, status_red) = argred::reduce(abs_x, rm);

    let (sin_r, s_sin) = taylor_sin(r);
    let (cos_r, s_cos) = taylor_cos(r);
    let status = status_red | s_sin | s_cos | Status::INEXACT;

    let (mut sin_abs, cos_abs) = match k_mod_4 {
        0 => (sin_r, cos_r),
        1 => (cos_r, sin_r.neg()),
        2 => (sin_r.neg(), cos_r.neg()),
        3 => (cos_r.neg(), sin_r),
        _ => unreachable!(),
    };

    if neg {
        sin_abs = sin_abs.neg();
    }

    ((sin_abs, status), (cos_abs, status))
}

/// `sin(r) = r − r³/3! + r⁵/5! − …` for `|r| ≤ π/4`.
fn taylor_sin(r: Decimal128) -> (Decimal128, Status) {
    let mut status = Status::OK;
    let mut sum = r;
    let mut term = r;
    let r_squared = {
        let (rs, s) = r.mul(r, RoundingMode::NearestEven);
        status |= s;
        rs
    };
    let mut alt = true; // first added term is positive `r`; next is negative.
    let mut n: i32 = 1; // current term index = (2n - 1)!
    let max_iters = 100;
    for _ in 0..max_iters {
        // term_{k+1} = -term_k * r² / ((2n)(2n+1))
        n += 1;
        let denom = Decimal128::from_i32((2 * n - 2) * (2 * n - 1));
        let (numer, s1) = term.mul(r_squared, RoundingMode::NearestEven);
        let (next_term, s2) = numer.div(denom, RoundingMode::NearestEven);
        status |= s1 | s2;
        term = next_term;
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let (next_sum, s3) = sum.add(signed, RoundingMode::NearestEven);
        status |= s3;
        let (cmp, _) = next_sum.partial_cmp(sum);
        sum = next_sum;
        if cmp == Some(core::cmp::Ordering::Equal) || term.is_zero() {
            break;
        }
    }
    (sum, status)
}

/// `cos(r) = 1 − r²/2! + r⁴/4! − …` for `|r| ≤ π/4`.
fn taylor_cos(r: Decimal128) -> (Decimal128, Status) {
    let mut status = Status::OK;
    let mut sum = Decimal128::ONE;
    let mut term = Decimal128::ONE;
    let r_squared = {
        let (rs, s) = r.mul(r, RoundingMode::NearestEven);
        status |= s;
        rs
    };
    let mut alt = true; // first added term is +1; next is -r²/2.
    let mut n: i32 = 0;
    let max_iters = 100;
    for _ in 0..max_iters {
        n += 1;
        let denom = Decimal128::from_i32((2 * n - 1) * (2 * n));
        let (numer, s1) = term.mul(r_squared, RoundingMode::NearestEven);
        let (next_term, s2) = numer.div(denom, RoundingMode::NearestEven);
        status |= s1 | s2;
        term = next_term;
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let (next_sum, s3) = sum.add(signed, RoundingMode::NearestEven);
        status |= s3;
        let (cmp, _) = next_sum.partial_cmp(sum);
        sum = next_sum;
        if cmp == Some(core::cmp::Ordering::Equal) || term.is_zero() {
            break;
        }
    }
    (sum, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::consts::pi;
    extern crate alloc;
    use alloc::format;
    use alloc::string::ToString;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
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
            assert!(
                approx_equal_ulps(cos_neg, cos_x, 50),
                "cos(-{s}) symmetry"
            );
        }
    }
}
