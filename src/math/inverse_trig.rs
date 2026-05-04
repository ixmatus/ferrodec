//! `atan(x)` and friends — `asin`, `acos`, `atan2`.
//!
//! ## atan algorithm
//!
//! Two stages of argument reduction bring `|x|` into
//! `[0, tan(π/8)] ≈ [0, 0.4142]`:
//!
//! 1. **Inversion**: `atan(x) = ±π/2 − atan(1/x)` for `|x| > 1`.
//! 2. **π/4 shift**: `atan(x) = π/4 + atan((x−1)/(x+1))` for
//!    `|x| > tan(π/8)`. The shifted argument is in
//!    `[−tan(π/8), 0]` for the original input range
//!    `(tan(π/8), 1]`.
//!
//! After reduction, the Taylor series
//! `atan(y) = y − y³/3 + y⁵/5 − y⁷/7 + …` converges in ≤ 200
//! iterations for `|y| ≤ tan(π/8)` (`0.414^200 ≈ 10^{-77}` —
//! comfortably past EXT_PRECISION = 50). Sign of `x` is folded back
//! at the end (`atan` is odd).
//!
//! ## asin / acos
//!
//! `asin(x) = atan(x / sqrt(1 − x²))` near zero; uses the numerically-
//! stable `2 · atan(x / (1 + sqrt(1 − x²)))` form near `|x| = 1`.
//! `acos(x) = π/2 − asin(x)`.
//!
//! ## atan2
//!
//! Quadrant dispatch as IEEE 754-2019 §9.2.1 specifies, plus the
//! `atan2(±0, ±0)` corner cases.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::consts::{
    pi_ext, pi_over_four_ext, pi_over_two_ext, tan_pi_over_eight_ext,
};
use crate::math::extended::Extended;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Inverse tangent. Range `(-π/2, +π/2)`.
    #[must_use]
    pub fn atan(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { sign } => {
                let half_pi = pi_over_two_ext().to_decimal128(0, rm).0;
                return (
                    if sign { half_pi.neg() } else { half_pi },
                    Status::INEXACT,
                );
            }
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        let x_ext = Extended::from_decimal128(self);
        let result_ext = atan_ext(x_ext);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Inverse sine. Domain `[-1, +1]`; outside is NaN + INVALID.
    /// Range `[-π/2, +π/2]`.
    #[must_use]
    pub fn asin(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        let abs_x = self.abs();
        let (cmp_one, _) = abs_x.partial_cmp(Decimal128::ONE);
        match cmp_one {
            Some(core::cmp::Ordering::Greater) => return (Decimal128::NAN, Status::INVALID),
            Some(core::cmp::Ordering::Equal) => {
                // asin(±1) = ±π/2.
                let half_pi = pi_over_two_ext().to_decimal128(0, rm).0;
                let signed = if self.is_sign_negative() {
                    half_pi.neg()
                } else {
                    half_pi
                };
                return (signed, Status::INEXACT);
            }
            _ => {}
        }
        let x_ext = Extended::from_decimal128(self);
        let result_ext = asin_ext(x_ext);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Inverse cosine. Domain `[-1, +1]`; outside is NaN + INVALID.
    /// Range `[0, π]`.
    #[must_use]
    pub fn acos(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::Zero { .. } => {
                let half_pi = pi_over_two_ext().to_decimal128(0, rm).0;
                return (half_pi, Status::INEXACT);
            }
            Class::Finite { .. } => {}
        }
        let abs_x = self.abs();
        let (cmp_one, _) = abs_x.partial_cmp(Decimal128::ONE);
        match cmp_one {
            Some(core::cmp::Ordering::Greater) => return (Decimal128::NAN, Status::INVALID),
            Some(core::cmp::Ordering::Equal) => {
                // acos(1) = 0; acos(-1) = π.
                if self.is_sign_negative() {
                    let pi_d = pi_ext().to_decimal128(0, rm).0;
                    return (pi_d, Status::INEXACT);
                }
                return (Decimal128::ZERO, Status::OK);
            }
            _ => {}
        }
        let x_ext = Extended::from_decimal128(self);
        // acos(x) = π/2 - asin(x).
        let asin_ext_v = asin_ext(x_ext);
        let result_ext = pi_over_two_ext().sub(asin_ext_v);
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }

    /// Two-argument arctangent `atan2(y, x)`. Range `(-π, +π]`.
    /// Quadrant per IEEE 754-2019 §9.2.1.
    #[must_use]
    pub fn atan2(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        let y = self;
        // NaN propagation (sNaN raises INVALID).
        if y.is_signaling_nan() || x.is_signaling_nan() {
            return (Decimal128::NAN, Status::INVALID);
        }
        if y.is_nan() || x.is_nan() {
            return (Decimal128::NAN, Status::OK);
        }
        let pi_d = pi_ext().to_decimal128(0, rm).0;
        let half_pi = pi_over_two_ext().to_decimal128(0, rm).0;
        let three_quarter_pi = pi_over_four_ext()
            .mul(Extended::from_i32(3))
            .to_decimal128(0, rm)
            .0;
        let quarter_pi = pi_over_four_ext().to_decimal128(0, rm).0;

        let y_neg = y.is_sign_negative();
        let signed = |v: Decimal128| if y_neg { v.neg() } else { v };

        // Inf handling.
        if x.is_infinite() && y.is_infinite() {
            // ±π/4 or ±3π/4 depending on signs.
            return if x.is_sign_negative() {
                (signed(three_quarter_pi), Status::INEXACT)
            } else {
                (signed(quarter_pi), Status::INEXACT)
            };
        }
        if y.is_infinite() {
            // ±π/2.
            return (signed(half_pi), Status::INEXACT);
        }
        if x.is_infinite() {
            return if x.is_sign_negative() {
                (signed(pi_d), Status::INEXACT)
            } else {
                (
                    if y_neg {
                        Decimal128::NEG_ZERO
                    } else {
                        Decimal128::ZERO
                    },
                    Status::OK,
                )
            };
        }
        // Both finite. Cover x = 0.
        if x.is_zero() {
            if y.is_zero() {
                // atan2(±0, +0) = ±0; atan2(±0, -0) = ±π.
                if x.is_sign_negative() {
                    return (signed(pi_d), Status::OK);
                }
                return (
                    if y_neg {
                        Decimal128::NEG_ZERO
                    } else {
                        Decimal128::ZERO
                    },
                    Status::OK,
                );
            }
            return (signed(half_pi), Status::INEXACT);
        }
        if y.is_zero() {
            // atan2(±0, x): 0 if x > 0, ±π if x < 0.
            return if x.is_sign_negative() {
                (signed(pi_d), Status::INEXACT)
            } else {
                (
                    if y_neg {
                        Decimal128::NEG_ZERO
                    } else {
                        Decimal128::ZERO
                    },
                    Status::OK,
                )
            };
        }
        // Both finite non-zero. Compute y/x at extended precision, run
        // atan, then quadrant-shift.
        let y_ext = Extended::from_decimal128(y);
        let x_ext = Extended::from_decimal128(x);
        let q = y_ext.div(x_ext);
        let mut result_ext = atan_ext(q);
        if x.is_sign_negative() {
            // atan2 in quadrants 2 / 3: shift by ±π.
            if y_neg {
                result_ext = result_ext.sub(pi_ext());
            } else {
                result_ext = result_ext.add(pi_ext());
            }
        }
        let (result, status) = result_ext.to_decimal128(0, rm);
        (result, status | Status::INEXACT)
    }
}

/// `atan(x)` at `Extended` precision. Pre-conditions: `x` is finite
/// and non-zero (zero handled in the caller's special-case path).
fn atan_ext(x: Extended) -> Extended {
    let neg = x.sign;
    let mut t = x.abs();
    let mut shift = Extended::ZERO;

    // Stage 1: |x| > 1 → atan(x) = π/2 - atan(1/x) (with sign).
    let mut inverted = false;
    if t.cmp(Extended::ONE) == core::cmp::Ordering::Greater {
        t = t.recip();
        inverted = true;
    }

    // Stage 2: tan(π/8) < |x| ≤ 1 → atan(x) = π/4 + atan((x-1)/(x+1)).
    let tan_eighth = tan_pi_over_eight_ext();
    if t.cmp(tan_eighth) == core::cmp::Ordering::Greater {
        let num = t.sub(Extended::ONE);
        let den = t.add(Extended::ONE);
        t = num.div(den); // signed: in [-tan(π/8), 0]
        shift = pi_over_four_ext();
    }

    // Taylor: atan(t) = t - t³/3 + t⁵/5 - t⁷/7 + …
    let mut sum = t;
    let mut t_pow = t; // t^(2k+1); initially t^1
    let t_sq = t.square();
    let mut alt = true; // next term subtracts
    for n in 1u32..=200 {
        t_pow = t_pow.mul(t_sq);
        let denom = 2 * n + 1;
        let term = t_pow.div_u32(denom);
        let signed_term = if alt { term.neg() } else { term };
        let next_sum = sum.add(signed_term);
        alt = !alt;
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if t_pow.is_zero() {
            break;
        }
    }

    // Apply Stage 2 shift.
    let mut result = if shift.is_zero() {
        sum
    } else {
        sum.add(shift)
    };
    // Apply Stage 1 inversion.
    if inverted {
        result = pi_over_two_ext().sub(result);
    }
    // Apply original sign.
    if neg {
        result = result.neg();
    }
    result
}

/// `asin(x)` at extended precision for `|x| < 1`. Uses
/// `2 · atan(x / (1 + sqrt(1 - x²)))` — numerically stable across
/// the full domain (no blow-up at `|x| = 1`).
fn asin_ext(x: Extended) -> Extended {
    if x.is_zero() {
        return x;
    }
    let one_minus_x_sq = Extended::ONE.sub(x.square());
    let sqrt_term = one_minus_x_sq.sqrt();
    let denom = Extended::ONE.add(sqrt_term);
    let inner = x.div(denom);
    let half_atan = atan_ext(inner);
    half_atan.add(half_atan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
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
    fn atan_zero() {
        let (r, _) = Decimal128::ZERO.atan(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atan_one_is_pi_over_four() {
        let (r, _) = Decimal128::ONE.atan(RoundingMode::NearestEven);
        let want = parse("0.7853981633974483096156608458198757");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan_neg_one() {
        let (r, _) = Decimal128::NEG_ONE.atan(RoundingMode::NearestEven);
        let want = parse("-0.7853981633974483096156608458198757");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan_inf() {
        let (r, _) = Decimal128::INFINITY.atan(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
        let (r, _) = Decimal128::NEG_INFINITY.atan(RoundingMode::NearestEven);
        assert!(within_ulps(r, want.neg(), 1));
    }

    #[test]
    fn atan_small() {
        let x = parse("0.1");
        let (r, _) = x.atan(RoundingMode::NearestEven);
        let want = parse("0.09966865249116202737844611987802059");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_zero() {
        let (r, _) = Decimal128::ZERO.asin(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn asin_one_is_half_pi() {
        let (r, _) = Decimal128::ONE.asin(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_half_is_pi_over_six() {
        let x = parse("0.5");
        let (r, _) = x.asin(RoundingMode::NearestEven);
        let want = parse("0.5235987755982988730771072305465838");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_out_of_range_is_invalid_nan() {
        let x = parse("1.5");
        let (r, st) = x.asin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn acos_zero_is_half_pi() {
        let (r, _) = Decimal128::ZERO.acos(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acos_one_is_zero() {
        let (r, _) = Decimal128::ONE.acos(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn acos_neg_one_is_pi() {
        let (r, _) = Decimal128::NEG_ONE.acos(RoundingMode::NearestEven);
        let want = parse("3.141592653589793238462643383279503");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_first_quadrant() {
        let (r, _) = parse("1").atan2(parse("1"), RoundingMode::NearestEven);
        let want = parse("0.7853981633974483096156608458198757"); // π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_second_quadrant() {
        let (r, _) = parse("1").atan2(parse("-1"), RoundingMode::NearestEven);
        let want = parse("2.356194490192344928846982537459627"); // 3π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_third_quadrant() {
        let (r, _) = parse("-1").atan2(parse("-1"), RoundingMode::NearestEven);
        let want = parse("-2.356194490192344928846982537459627"); // -3π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_fourth_quadrant() {
        let (r, _) = parse("-1").atan2(parse("1"), RoundingMode::NearestEven);
        let want = parse("-0.7853981633974483096156608458198757"); // -π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_axis_corners() {
        let (r, _) = parse("0").atan2(parse("1"), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
        let (r, _) = parse("0").atan2(parse("-1"), RoundingMode::NearestEven);
        let want_pi = parse("3.141592653589793238462643383279503");
        assert!(within_ulps(r, want_pi, 1));
        let (r, _) = parse("1").atan2(parse("0"), RoundingMode::NearestEven);
        let want_half_pi = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want_half_pi, 1));
    }
}
