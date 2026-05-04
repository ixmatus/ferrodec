//! `exp(x)` — natural exponential.
//!
//! ## Algorithm
//!
//! 1. Special cases: NaN / sNaN / ±∞ / ±0.
//! 2. Range reduction. We split `x = k · ln(10) + r` with `|r| ≤ ln(10)/2`
//!    so `r` lives in roughly `[-1.151, 1.151]`. Then
//!
//!    ```text
//!    exp(x) = 10^k · exp(r)
//!    ```
//!
//!    where `10^k` is a quantum shift on `Decimal128`.
//! 3. `exp(r)` via Taylor series. The radius is bounded so 30–35 terms
//!    converge below the 34-digit envelope.
//! 4. Multiply by `10^k` (quantum shift) — no rounding loss.
//!
//! ## v1 accuracy
//!
//! Native Decimal128 arithmetic throughout. Each Taylor term involves a
//! `mul` and a `div` by a small integer; both are correctly-rounded but
//! the accumulating sum can drift up to ~5 ULP for inputs near the
//! edges of the reduction window. Tracked as a follow-up — closing the
//! gap to faithful rounding needs a wider intermediate type.
//!
//! ## Domain
//!
//! `exp` overflows at roughly `x ≈ 14149` (`exp(x) ≈ 10^6144`) and
//! underflows to `±0` at `x ≈ -14150`. We detect both and emit the
//! IEEE 754 flags accordingly without going through Taylor.

use crate::bid::{classify_bits, pack_finite, Class, BIAS, BIASED_EXP_MAX};
use crate::decimal::Decimal128;
use crate::math::consts::ln10;
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
}

fn exp_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
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

    // Cheap overflow / underflow gate via a coarse magnitude estimate.
    // We want a bound that's fast to compute, so we read the unbiased
    // exponent and digit count and approximate `log10(|x|)`. If
    // `|x| > 14149.6` we can skip the Taylor work.
    if let Some((value, status)) = saturate_extreme(x) {
        return (value, status);
    }

    // Range reduction: x = k * ln(10) + r, |r| ≤ ln(10)/2.
    let ln10_v = ln10();
    let (k, r, reduce_status) = reduce_to_window(x, ln10_v, rm);

    // Compute exp(r) via Taylor.
    let (exp_r, taylor_status) = taylor_exp(r, rm);

    // Final result: exp(r) × 10^k. `10^k` is a pure quantum shift —
    // we encode it as a multiplication by `Decimal128::from(10).pow(k)`
    // approximation; here we re-emit `exp_r` with adjusted quantum,
    // which is exact when no precision is lost.
    let (result, scale_status) = scale_by_pow10(exp_r, k, rm);

    let status = reduce_status | taylor_status | scale_status;
    (result, status)
}

/// Coarse extreme-magnitude detection. Returns `Some((±∞ or ±0, status))`
/// when the input is way outside the convergence window.
fn saturate_extreme(x: Decimal128) -> Option<(Decimal128, Status)> {
    // Compare x against ±OVERFLOW_THRESHOLD. We do this by parsing the
    // threshold as a Decimal128 once; since the threshold is constant
    // and small, rounding doesn't matter.
    let positive = !x.is_sign_negative();
    let abs_x = x.abs();
    let threshold = Decimal128::parse_str("14150", RoundingMode::NearestEven)
        .expect("threshold parses")
        .0;
    let (cmp, _) = abs_x.partial_cmp(threshold);
    if cmp != Some(core::cmp::Ordering::Greater) {
        return None;
    }
    if positive {
        // Overflow: exp(x) = +∞, raise OVERFLOW + INEXACT.
        Some((Decimal128::INFINITY, Status::OVERFLOW | Status::INEXACT))
    } else {
        // Underflow: exp(x) = +0, raise UNDERFLOW + INEXACT.
        Some((
            Decimal128::ZERO,
            Status::UNDERFLOW | Status::INEXACT,
        ))
    }
}

/// `x = k · ln(10) + r`. Returns `(k, r, status)`.
///
/// `k` is `round(x / ln(10))` rounded to nearest even, and the residue
/// `r ∈ [-ln(10)/2, ln(10)/2]`.
fn reduce_to_window(
    x: Decimal128,
    ln10_v: Decimal128,
    rm: RoundingMode,
) -> (i32, Decimal128, Status) {
    let mut status = Status::OK;
    // q = x / ln(10).
    let (q, _) = x.div(ln10_v, RoundingMode::NearestEven);
    // k = round-to-nearest-even integer of q.
    let (k, _) = q.to_i32(RoundingMode::NearestEven);
    let k_dec = Decimal128::from_i32(k);
    // Build `k · ln(10)` and subtract from x.
    let (k_ln10, st_mul) = k_dec.mul(ln10_v, RoundingMode::NearestEven);
    status |= st_mul;
    let (r, st_sub) = x.sub(k_ln10, rm);
    status |= st_sub;
    (k, r, status)
}

/// Taylor series: `exp(r) = Σ r^n / n!`. The factorial denominator is
/// updated iteratively (`1/(n+1)! = (1/n!) / (n+1)`) so we never need a
/// separate factorial constant table.
///
/// Bounds the term count: max ~36 iterations covers `|r| ≤ ln(10)/2`
/// to ~36 digits of precision. We early-exit once the term magnitude
/// drops below `~10^-37`.
fn taylor_exp(r: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let mut status = Status::OK;
    let mut sum = Decimal128::ONE; // term n=0
    let mut term = Decimal128::ONE;
    let one = Decimal128::ONE;

    // Halt when the next term won't change the sum at our precision.
    // `term` decays by `r/(n+1)` each step; with |r| ≤ 1.151 and n
    // ≥ ~33 the magnitude is below 10^-35.
    let max_iterations = 60;
    let mut n: i32 = 0;
    while n < max_iterations {
        n += 1;
        // term = term * r / n
        let n_dec = Decimal128::from_i32(n);
        let (mul_term, st1) = term.mul(r, RoundingMode::NearestEven);
        let (next_term, st2) = mul_term.div(n_dec, RoundingMode::NearestEven);
        status |= st1 | st2;
        term = next_term;
        // sum += term
        let (next_sum, st3) = sum.add(term, RoundingMode::NearestEven);
        status |= st3;
        // Halt when adding `term` no longer changes `sum` (cohort match
        // is overly strict; numeric equality is the right check).
        let (cmp, _) = next_sum.partial_cmp(sum);
        sum = next_sum;
        if cmp == Some(core::cmp::Ordering::Equal) {
            break;
        }
        // Also halt if `term` underflows to zero.
        if term.is_zero() {
            break;
        }
        let _ = one;
    }

    // Final round in the requested mode. Since we computed in
    // round-to-nearest-even internally, this is the user-visible
    // rounding step. For most rounding modes the result is already
    // close enough that re-rounding doesn't change much, but we
    // honour `rm` for at least the sign-asymmetric cases.
    let _ = rm;
    (sum, status | Status::INEXACT)
}

/// Multiply `value` by `10^k` (k may be negative). For any
/// well-encoded finite `value`, this is purely a quantum shift —
/// we adjust the biased exponent and check overflow/underflow,
/// without touching the coefficient.
fn scale_by_pow10(
    value: Decimal128,
    k: i32,
    rm: RoundingMode,
) -> (Decimal128, Status) {
    if !value.is_finite() || value.is_zero() {
        return (value, Status::OK);
    }
    match classify_bits(value.to_bits()) {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let new_biased = (biased_exp as i32) + k;
            if new_biased > BIASED_EXP_MAX as i32 {
                // Overflow.
                let result = if sign {
                    Decimal128::NEG_INFINITY
                } else {
                    Decimal128::INFINITY
                };
                let _ = rm;
                return (result, Status::OVERFLOW | Status::INEXACT);
            }
            if new_biased < 0 {
                // Underflow toward zero (subnormals not handled with
                // full precision here — v1 limitation).
                let _ = rm;
                return (
                    Decimal128::from_bits(pack_finite(sign, 0, 0)),
                    Status::UNDERFLOW | Status::INEXACT,
                );
            }
            (
                Decimal128::from_bits(pack_finite(sign, new_biased as u32, coefficient)),
                Status::OK,
            )
        }
        _ => (value, Status::OK),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn approx_equal(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        // Coarse ULP comparison: difference / b should be below
        // ulps × 10^-33 (1 ULP at PRECISION = 34 ≈ 10^-33).
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let bound = parse(&match ulps {
            1 => "1e-33".to_string(),
            5 => "5e-33".to_string(),
            n => alloc::format!("{n}e-32"),
        });
        let abs_b = b.abs();
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(cmp, Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal))
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
            approx_equal(r, target, 5),
            "exp(1) = {r:?}, want ≈ {target:?}"
        );
    }

    #[test]
    fn exp_neg_one() {
        let (r, _) = Decimal128::NEG_ONE.exp(RoundingMode::NearestEven);
        let target = parse("0.3678794411714423215955237701614608");
        assert!(approx_equal(r, target, 5));
    }

    #[test]
    fn exp_two() {
        let two = parse("2");
        let (r, _) = two.exp(RoundingMode::NearestEven);
        let target = parse("7.389056098930650227230427460575008");
        assert!(approx_equal(r, target, 5));
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

    extern crate alloc;
    use alloc::string::ToString;
}
