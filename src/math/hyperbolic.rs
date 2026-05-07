//! Hyperbolic functions and their inverses.
//!
//! ## Forward
//!
//! * `sinh(x) = (eˣ − e⁻ˣ) / 2`
//! * `cosh(x) = (eˣ + e⁻ˣ) / 2`
//! * `tanh(x) = sinh(x) / cosh(x)`
//!
//! For large `|x|` (≳ 14000) `eˣ` overflows; both `sinh` and `cosh`
//! saturate to `±∞`, and `tanh` saturates to `±1`.
//!
//! For small `|x|` (`|x| < 0.5`), the naive `(eˣ − e⁻ˣ)/2` formula
//! suffers cancellation (eˣ and e⁻ˣ are both ≈ 1). We use Taylor
//! directly there: `sinh(x) = x + x³/3! + x⁵/5! + …`. `cosh` is even
//! so the same concern doesn't apply (no cancellation between
//! adjacent terms).
//!
//! ## Inverse
//!
//! * `asinh(x) = ln(x + √(x² + 1))` for all real `x`. Stable for
//!   any sign because `x² + 1 ≥ 1`.
//! * `acosh(x) = ln(x + √(x² − 1))` for `x ≥ 1`; NaN otherwise.
//! * `atanh(x) = ½·ln((1 + x) / (1 − x))` for `|x| < 1`; ±∞ at
//!   `±1`; NaN otherwise.
//!
//! All routines run at `Extended` precision and round once at the
//! `Decimal128` boundary.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::exp::exp_extended;
use crate::math::extended::Extended;
use crate::math::ln::{ln_from_extended, log1p_extended};
use crate::multiword::U256;
use crate::ops::nan_from;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Hyperbolic sine.
    #[must_use]
    pub fn sinh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (self, Status::OK),
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        let x_ext = Extended::from_decimal128(self);
        let result_ext = sinh_ext(x_ext);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Hyperbolic cosine.
    #[must_use]
    pub fn cosh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (Decimal128::INFINITY, Status::OK),
            Class::Zero { .. } => return (Decimal128::ONE, Status::OK),
            Class::Finite { .. } => {}
        }
        let x_ext = Extended::from_decimal128(self).abs();
        let result_ext = cosh_ext(x_ext);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Hyperbolic tangent.
    #[must_use]
    pub fn tanh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { sign } => {
                return (
                    if sign {
                        Decimal128::NEG_ONE
                    } else {
                        Decimal128::ONE
                    },
                    Status::OK,
                );
            }
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        // For |x| ≳ 35 ln(10) ≈ 80, tanh saturates to ±1 within
        // Decimal128 precision. The eˣ branch would overflow well
        // before that anyway.
        let abs_x = self.abs();
        let saturate_threshold = Decimal128::parse_str("80", RoundingMode::NearestEven)
            .expect("literal")
            .0;
        let (cmp, _) = abs_x.partial_cmp(saturate_threshold);
        if matches!(cmp, Some(core::cmp::Ordering::Greater)) {
            return (
                if self.is_sign_negative() {
                    Decimal128::NEG_ONE
                } else {
                    Decimal128::ONE
                },
                Status::INEXACT,
            );
        }
        let x_ext = Extended::from_decimal128(self);
        let s = sinh_ext(x_ext);
        let c = cosh_ext(x_ext.abs());
        // tanh inherits the sign of x via sinh; cosh is symmetric.
        let result_ext = s.div(c);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Inverse hyperbolic sine, defined for all real `self`.
    #[must_use]
    pub fn asinh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (self, Status::OK),
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        // asinh(x) = sign(x) · ln(|x| + sqrt(x² + 1))
        // Working on |x| keeps the inner sum strictly positive.
        let neg = self.is_sign_negative();
        let abs_x_ext = Extended::from_decimal128(self).abs();
        let x_sq_plus_one = abs_x_ext.square().add(Extended::ONE);
        let inner = abs_x_ext.add(x_sq_plus_one.sqrt());
        // Pass `inner` to `ln_from_extended` directly — keeping the
        // argument at 50-digit working precision avoids a 34-digit
        // round trip that would propagate ≤ 1 ULP through `ln` to the
        // result.
        let result_ext = ln_from_extended(inner);
        let signed_ext = if neg { result_ext.neg() } else { result_ext };
        let (result, status) = signed_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Inverse hyperbolic cosine, defined for `self ≥ 1`.
    #[must_use]
    pub fn acosh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { sign } => {
                return if sign {
                    (Decimal128::NAN, Status::INVALID)
                } else {
                    (Decimal128::INFINITY, Status::OK)
                };
            }
            Class::Zero { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::Finite { .. } => {}
        }
        let (cmp, _) = self.partial_cmp(Decimal128::ONE);
        match cmp {
            Some(core::cmp::Ordering::Less) => return (Decimal128::NAN, Status::INVALID),
            Some(core::cmp::Ordering::Equal) => return (Decimal128::ZERO, Status::OK),
            _ => {}
        }
        // Two paths, picked by how close x is to 1:
        //
        // * For x near 1, computing `x² − 1` directly cancels and costs
        //   ~`digit_count(x − 1)` digits of precision. Extended carries
        //   ~16 digits of headroom over Decimal128, so the original
        //   formula is fine for `x − 1 ≥ 10⁻¹⁶` but loses the envelope
        //   below that. The log1p path keeps `(x − 1)` explicit and
        //   factors `x² − 1 = (x − 1)(x + 1)`, avoiding the cancellation
        //   entirely:
        //
        //       acosh(x) = ln(1 + (x − 1) + sqrt((x − 1)(x + 1)))
        //                = log1p((x − 1) + sqrt((x − 1)(x + 1)))
        //
        // * For x further from 1 the original `ln(x + sqrt(x² − 1))`
        //   path runs entirely at Extended precision (commit f43ce0e)
        //   and stays within ≤ 1 ULP at 34 digits.
        //
        // The threshold `0.01` keeps `inner` comfortably inside log1p's
        // Taylor convergence window (`inner ≤ ~0.15` at this y).
        let x_ext = Extended::from_decimal128(self);
        let y = x_ext.sub(Extended::ONE);
        const LOG1P_THRESHOLD: Extended = Extended {
            coef: U256::from_u128(1),
            exp: -2,
            sign: false,
        };
        let result_ext = if y.cmp(LOG1P_THRESHOLD) == core::cmp::Ordering::Less {
            let x_plus_one = x_ext.add(Extended::ONE);
            let inner = y.add(y.mul(x_plus_one).sqrt());
            log1p_extended(inner)
        } else {
            let x_sq_minus_one = x_ext.square().sub(Extended::ONE);
            let inner = x_ext.add(x_sq_minus_one.sqrt());
            ln_from_extended(inner)
        };
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Inverse hyperbolic tangent, defined for `|self| < 1`.
    #[must_use]
    pub fn atanh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        let abs_x = self.abs();
        let (cmp, _) = abs_x.partial_cmp(Decimal128::ONE);
        match cmp {
            Some(core::cmp::Ordering::Greater) => return (Decimal128::NAN, Status::INVALID),
            Some(core::cmp::Ordering::Equal) => {
                // atanh(±1) = ±∞, raise DIV_BY_ZERO (the formula has
                // 1/(1−|x|) at the singularity).
                return (
                    if self.is_sign_negative() {
                        Decimal128::NEG_INFINITY
                    } else {
                        Decimal128::INFINITY
                    },
                    Status::DIV_BY_ZERO,
                );
            }
            _ => {}
        }
        // atanh(x) = ½·ln((1 + x) / (1 − x)) — ratio stays at extended
        // precision through the ln call.
        let x_ext = Extended::from_decimal128(self);
        let one_plus = Extended::ONE.add(x_ext);
        let one_minus = Extended::ONE.sub(x_ext);
        let ratio = one_plus.div(one_minus);
        let ln_ratio_ext = ln_from_extended(ratio);
        let result_ext = ln_ratio_ext.div_u32(2);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }
}

/// `sinh(x)` at `Extended` precision.
fn sinh_ext(x: Extended) -> Extended {
    if x.is_zero() {
        return x;
    }
    // For |x| < 0.5 use Taylor directly to avoid cancellation in
    // (eˣ − e⁻ˣ)/2. The threshold 0.5 keeps Taylor convergence at
    // ≤ ~40 iterations for 50-digit precision.
    if x.abs().cmp(Extended::HALF) == core::cmp::Ordering::Less {
        return sinh_taylor(x);
    }
    // Saturation: |x| past the exp convergence ceiling lands outside
    // Decimal128's range. Return a pre-overflow magnitude with the
    // sign of x; the boundary round produces ±∞ + OVERFLOW.
    if x.abs().cmp(Extended::EXP_DOMAIN_LIMIT) == core::cmp::Ordering::Greater {
        return Extended::saturate_overflow(x.sign);
    }
    // sinh(x) = (e^x − e^{-x}) / 2, evaluated entirely at extended
    // precision so the cancellation is bounded by Extended's 50-digit
    // working envelope rather than Decimal128's 34-digit one. Combined
    // with the |x| < 0.5 Taylor branch above, this gives ≤ 1 ULP at the
    // 34-digit boundary across the whole representable domain.
    let e_pos = exp_extended(x);
    let e_neg = exp_extended(x.neg());
    e_pos.sub(e_neg).div_u32(2)
}

/// `sinh(x)` Taylor series for `|x| < 0.5`.
/// `sinh(x) = x + x³/3! + x⁵/5! + …` (all positive — no
/// cancellation).
fn sinh_taylor(x: Extended) -> Extended {
    let mut sum = x;
    let mut term = x;
    let x_sq = x.square();
    let mut n: u32 = 1;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 2) * (2 * n - 1);
        term = term.mul(x_sq).div_u32(denom);
        let next_sum = sum.add(term);
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

/// `cosh(x)` at `Extended` precision. Caller passes the absolute
/// value (cosh is even).
fn cosh_ext(abs_x: Extended) -> Extended {
    if abs_x.is_zero() {
        return Extended::ONE;
    }
    // For small |x| (<0.5), Taylor is more accurate (no cancellation).
    if abs_x.cmp(Extended::HALF) == core::cmp::Ordering::Less {
        return cosh_taylor(abs_x);
    }
    // Saturation: |x| past the exp convergence ceiling lands outside
    // Decimal128's range. cosh is always positive.
    if abs_x.cmp(Extended::EXP_DOMAIN_LIMIT) == core::cmp::Ordering::Greater {
        return Extended::saturate_overflow(false);
    }
    // cosh(x) = (e^x + e^{-x}) / 2, end-to-end at extended precision.
    let e_pos = exp_extended(abs_x);
    let e_neg = exp_extended(abs_x.neg());
    e_pos.add(e_neg).div_u32(2)
}

/// `cosh(x) = 1 + x²/2! + x⁴/4! + …` for small `|x|`.
fn cosh_taylor(x: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    let x_sq = x.square();
    let mut n: u32 = 0;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 1) * (2 * n);
        term = term.mul(x_sq).div_u32(denom);
        let next_sum = sum.add(term);
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

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
        let (diff, _) = got.sub(want, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_want = want.abs();
        if abs_want.is_zero() {
            let bound = parse(&alloc::format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
        let bound = parse(&alloc::format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    extern crate alloc;

    #[test]
    fn sinh_zero() {
        let (r, _) = Decimal128::ZERO.sinh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn sinh_one() {
        let (r, _) = Decimal128::ONE.sinh(RoundingMode::NearestEven);
        let want = parse("1.175201193643801456882381850595601");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn cosh_zero() {
        let (r, _) = Decimal128::ZERO.cosh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn cosh_one() {
        let (r, _) = Decimal128::ONE.cosh(RoundingMode::NearestEven);
        let want = parse("1.543080634815243778477905620757061");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn tanh_zero() {
        let (r, _) = Decimal128::ZERO.tanh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn tanh_huge_saturates() {
        let x = parse("1000");
        let (r, _) = x.tanh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn tanh_neg_huge_saturates() {
        let x = parse("-1000");
        let (r, _) = x.tanh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::NEG_ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn asinh_zero() {
        let (r, _) = Decimal128::ZERO.asinh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn asinh_one() {
        let (r, _) = Decimal128::ONE.asinh(RoundingMode::NearestEven);
        let want = parse("0.8813735870195430252326093249797923");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acosh_one_is_zero() {
        let (r, _) = Decimal128::ONE.acosh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn acosh_two() {
        let (r, _) = parse("2").acosh(RoundingMode::NearestEven);
        let want = parse("1.316957896924816708625046347307969");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acosh_below_one_is_invalid_nan() {
        let (r, st) = parse("0.5").acosh(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn acosh_just_above_one_log1p_path() {
        // x = 1 + 10⁻³³, the smallest near-1 input the test suite can
        // build. acosh(1 + ε) ≈ √(2ε); for ε = 10⁻³³ that's √2 × 10⁻¹⁶·⁵
        // ≈ 4.472_135_954_999_579_392_818_347_337_462_552 × 10⁻¹⁷ at
        // 34 digits.
        //
        // The original `ln(x + sqrt(x² − 1))` formula loses ~33 digits
        // of precision at this input via the x²−1 cancellation; the
        // log1p path closes that gap.
        let x = parse("1.000000000000000000000000000000001");
        let (r, _) = x.acosh(RoundingMode::NearestEven);
        let want = parse("4.472135954999579392818347337462552E-17");
        assert!(
            within_ulps(r, want, 1),
            "acosh({x}) = {r:?}, want ≈ {want:?}"
        );
    }

    #[test]
    fn acosh_threshold_boundary_consistent() {
        // x = 1.01 sits right at the LOG1P_THRESHOLD. acosh(1.01) =
        // ln(1.01 + sqrt(1.01² − 1)) = ln(1 + 0.14177446878757825…) ≈
        // 0.141_303_769_485_648_577_351_151_646_974_354_6 at 34 digits.
        // The threshold's ≥ branch handles this input (y = 0.01 is
        // not strictly less than LOG1P_THRESHOLD = 0.01).
        let (r, _) = parse("1.01").acosh(RoundingMode::NearestEven);
        let want = parse("0.1413037694856485773511516469743546");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atanh_zero() {
        let (r, _) = Decimal128::ZERO.atanh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atanh_half() {
        let (r, _) = parse("0.5").atanh(RoundingMode::NearestEven);
        let want = parse("0.5493061443340548456976226184612628");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atanh_one_is_inf() {
        let (r, st) = Decimal128::ONE.atanh(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.div_by_zero());
    }

    #[test]
    fn atanh_above_one_is_invalid_nan() {
        let (r, st) = parse("1.5").atanh(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn sinh_neg_x_is_neg_sinh() {
        let x = parse("0.7");
        let (s_pos, _) = x.sinh(RoundingMode::NearestEven);
        let (s_neg, _) = x.neg().sinh(RoundingMode::NearestEven);
        assert!(within_ulps(s_neg, s_pos.neg(), 1));
    }
}
