//! `exp(x)` — natural exponential.
//!
//! ## Algorithm
//!
//! 1. Special cases: NaN / sNaN / ±∞ / ±0.
//! 2. Range reduction. Split `x = k · ln(10) + r` with `|r| ≤ ln(10)/2`,
//!    so `r` lives in roughly `[-1.151, 1.151]`. Then
//!
//!    ```text
//!    exp(x) = 10^k · exp(r)
//!    ```
//!
//!    where `10^k` is a quantum shift on the `Extended` (and the final
//!    `Decimal128`).
//! 3. Compute `exp(r)` via Taylor series at extended precision
//!    (`Extended` — see [`super::extended`]). 50-digit working
//!    precision keeps the cumulative series error below the
//!    34-digit envelope.
//! 4. Round to `Decimal128` once at the end via
//!    `round_and_pack_finite`, threading through OVERFLOW / UNDERFLOW.
//!
//! ## Accuracy
//!
//! Faithfully rounded (≤ 1 ULP at 34 digits) against `astro-float`
//! across the supported domain `|x| ≤ 14149`. Values past the domain
//! short-circuit to ±∞ / ±0 with the appropriate IEEE 754 flags.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::consts::{inv_ln10_ext, ln10_ext, ln2_ext};
use crate::math::extended::Extended;
use crate::ops::nan_from;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Natural exponential `e^self`, rounded according to `rm`.
    ///
    /// Domain: every finite input maps to a defined IEEE result —
    /// finite, `+0` (underflow), or `+∞` (overflow).
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        exp_kernel(self, rm)
    }

    /// Base-2 exponential `2^self`. Computed as
    /// `exp(self · ln(2))` at extended precision.
    #[must_use]
    pub fn exp2(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (nan_from(self), Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { sign } => {
                return if sign {
                    (Decimal128::ZERO, Status::OK)
                } else {
                    (Decimal128::INFINITY, Status::OK)
                };
            }
            Class::Zero { .. } => return (Decimal128::ONE, Status::OK),
            Class::Finite { .. } => {}
        }
        let arg_ext = Extended::from_decimal128(self).mul(ln2_ext());
        exp_from_extended(arg_ext, rm)
    }
}

fn exp_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { .. } => return (nan_from(x), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return if sign {
                (Decimal128::ZERO, Status::OK)
            } else {
                (Decimal128::INFINITY, Status::OK)
            };
        }
        Class::Zero { .. } => return (Decimal128::ONE, Status::OK),
        Class::Finite { .. } => {}
    }

    if let Some(early) = saturate_extreme(x) {
        return early;
    }

    let x_ext = Extended::from_decimal128(x);
    exp_from_extended(x_ext, rm)
}

/// Compute `exp(x_ext)` and round to `Decimal128`. Used by the public
/// `Decimal128::exp` and by `pow`'s general `exp(y · ln(x))` path.
///
/// Caller is responsible for filtering NaN / Inf / Zero inputs (those
/// have shortcuts that don't go through Taylor). For finite inputs of
/// any magnitude this routine handles the OVERFLOW / UNDERFLOW
/// thresholds internally.
pub(super) fn exp_from_extended(x_ext: Extended, rm: RoundingMode) -> (Decimal128, Status) {
    // Magnitude gate: `exp` overflows past `+ln(MAX) ≈ +14149.4` and
    // underflows past `−ln(1/MIN_SUBNORMAL) ≈ −14223`. The
    // thresholds are asymmetric because Decimal128's exponent range
    // is lopsided (E_MAX = 6144, MIN_SUBNORMAL exponent = −6176).
    // Inputs in `(−14223, −14150]` produce subnormals — must NOT
    // short-circuit to zero, the Taylor pipeline handles them.
    let abs = x_ext.abs();
    let limit = if x_ext.sign {
        Extended::EXP_UNDERFLOW_LIMIT
    } else {
        Extended::EXP_OVERFLOW_LIMIT
    };
    if abs.cmp(limit) == core::cmp::Ordering::Greater {
        return if x_ext.sign {
            (Decimal128::ZERO, Status::UNDERFLOW | Status::INEXACT)
        } else {
            (Decimal128::INFINITY, Status::OVERFLOW | Status::INEXACT)
        };
    }

    let result_ext = exp_extended(x_ext);
    let (result, status) = result_ext.to_decimal128(0, rm);
    (result, status | Status::INEXACT)
}

/// Compute `exp(x_ext)` and return the result *at extended precision*.
/// Distinct from [`exp_from_extended`] in that no rounding to
/// `Decimal128` happens — the caller composes further at extended
/// precision and rounds once at the boundary.
///
/// Used by `sinh` / `cosh` to compute `(e^x ± e^{-x}) / 2` without
/// the precision-loss of an intermediate `Decimal128` round-trip.
///
/// Caller must guarantee `|x_ext|` is within the convergence window
/// (`|x| ≤ ~14150`); larger inputs land in [`exp_from_extended`]'s
/// saturation branch and are not handled here. The returned `Extended`
/// can have an exponent outside `Decimal128`'s representable range —
/// the boundary rounder handles that as OVERFLOW.
pub(super) fn exp_extended(x_ext: Extended) -> Extended {
    // Reduction: x = k · ln(10) + r, with |r| ≤ ln(10)/2.
    let q = x_ext.mul(inv_ln10_ext());
    let k = round_to_i32(q);
    let r = x_ext.sub(Extended::from_i32(k).mul(ln10_ext()));

    // Taylor series at extended precision.
    let exp_r = taylor_exp_ext(r);

    // exp(x) = exp(r) · 10^k.
    exp_r.mul_pow10_exp(k)
}

/// Round an Extended to the nearest `i32`. Used to recover the
/// reduction integer `k` from `q = x / ln(10)`.
fn round_to_i32(q: Extended) -> i32 {
    if q.is_zero() {
        return 0;
    }
    // Add ±0.5 (depending on sign), then truncate toward zero.
    let nudged = if q.sign {
        q.sub(Extended::HALF)
    } else {
        q.add(Extended::HALF)
    };
    truncate_to_i32(nudged)
}

/// Truncate an Extended toward zero into an `i32`. Caller guarantees
/// the magnitude is well within `i32::MAX`.
fn truncate_to_i32(v: Extended) -> i32 {
    if v.is_zero() {
        return 0;
    }
    // Shift coef by exp to recover the integer value.
    if v.exp >= 0 {
        // coef · 10^exp — but for our `k` reduction, exp should
        // always be ≤ 0 (since |x| ≤ 14149 → |q| ≤ 6145 < 10^4 and
        // the .mul produced ~50-digit coef with exp ≈ -50).
        // Defensively widen: scale up.
        let mut c = v.coef;
        for _ in 0..(v.exp as u32) {
            c = c.mul10();
        }
        let val = c.lo as i64;
        return if v.sign { -(val as i32) } else { val as i32 };
    }
    // exp < 0: shift right.
    let mut c = v.coef;
    for _ in 0..((-v.exp) as u32) {
        let (q, _) = c.div_rem10();
        c = q;
    }
    let val = c.lo as i64;
    if v.sign {
        -(val as i32)
    } else {
        val as i32
    }
}

/// `exp(r) = Σ r^n / n!` evaluated at `Extended` precision.
///
/// Convergence: `|r| ≤ ln(10)/2 ≈ 1.151`, and `|r|^n / n!` decays
/// faster than geometrically once `n > |r|`. ~36 terms drives the
/// term magnitude below `10^{-49}`, well past `EXT_PRECISION = 50`.
fn taylor_exp_ext(r: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    // Halt early if `term` falls below ~10^{-55} (well below
    // EXT_PRECISION's significance).
    for n in 1u32..=60 {
        term = term.mul(r).div_u32(n);
        let next_sum = sum.add(term);
        // Early exit: if `next_sum` matches `sum` at extended
        // precision, further terms will round to zero contribution.
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

/// Coarse extreme-magnitude detection. Returns `Some((±∞ or ±0, status))`
/// when the input is way outside the convergence window. Asymmetric
/// thresholds — see [`Extended::EXP_OVERFLOW_LIMIT`] /
/// [`Extended::EXP_UNDERFLOW_LIMIT`] for why.
fn saturate_extreme(x: Decimal128) -> Option<(Decimal128, Status)> {
    let positive = !x.is_sign_negative();
    let abs_x = x.abs();
    let threshold_str = if positive { "14150" } else { "14221" };
    let threshold = Decimal128::parse_str(threshold_str, RoundingMode::NearestEven)
        .expect("threshold parses")
        .0;
    let (cmp, _) = abs_x.partial_cmp(threshold);
    if cmp != Some(core::cmp::Ordering::Greater) {
        return None;
    }
    if positive {
        Some((Decimal128::INFINITY, Status::OVERFLOW | Status::INEXACT))
    } else {
        Some((Decimal128::ZERO, Status::UNDERFLOW | Status::INEXACT))
    }
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

    #[test]
    fn exp_zero_is_one() {
        let (r, _) = Decimal128::ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn exp_one_is_e() {
        let (r, _) = Decimal128::ONE.exp(RoundingMode::NearestEven);
        let target = parse("2.718281828459045235360287471352662");
        assert!(
            within_ulps(r, target, 1),
            "exp(1) = {r:?}, want ≈ {target:?}"
        );
    }

    #[test]
    fn exp_neg_one() {
        let (r, _) = Decimal128::NEG_ONE.exp(RoundingMode::NearestEven);
        let target = parse("0.3678794411714423215955237701614608");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn exp_two() {
        let two = parse("2");
        let (r, _) = two.exp(RoundingMode::NearestEven);
        let target = parse("7.389056098930650227230427460575008");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn exp_nan_propagates() {
        let (r, _) = Decimal128::NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_nan());

        let (r, s) = Decimal128::SIGNALING_NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn exp_pos_inf_is_pos_inf() {
        let (r, _) = Decimal128::INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn exp_neg_inf_is_zero() {
        let (r, _) = Decimal128::NEG_INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn exp_overflow() {
        let big = parse("15000");
        let (r, s) = big.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow());
    }

    #[test]
    fn exp_underflow() {
        let big_neg = parse("-15000");
        let (r, s) = big_neg.exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.underflow());
    }

    #[test]
    fn exp_subnormal_window_does_not_saturate_to_zero() {
        // Pre-1.13 the underflow gate was symmetric at ±14150, but
        // the real underflow boundary is wider on the negative side.
        // The smallest representable Decimal128 subnormal is
        // `1 × 10⁻⁶¹⁷⁶`, and round-to-nearest-even maps any
        // `exp(x) < ½ × MIN_SUBNORMAL` to +0; that cutoff sits at
        // x ≈ −14220.85. Inputs strictly between −14221 and −14150
        // produce subnormal-but-non-zero results (e.g. exp(−14200)
        // ≈ 10⁻⁶¹⁶⁷) and the kernel must NOT saturate them.
        for s in ["-14151", "-14200", "-14219"] {
            let x = parse(s);
            let (r, st) = x.exp(RoundingMode::NearestEven);
            assert!(
                !r.is_zero(),
                "exp({s}) should produce a representable subnormal, \
                 got 0 (status {st:?})",
            );
            assert!(r.is_finite() && !r.is_sign_negative());
            assert!(st.inexact());
        }
        // Past the round-to-zero boundary, saturate is correct.
        let too_far = parse("-14225");
        let (r, st) = too_far.exp(RoundingMode::NearestEven);
        assert!(r.is_zero(), "exp(-14225) is past MIN_SUBNORMAL/2");
        assert!(st.underflow());
    }

    extern crate alloc;
}
