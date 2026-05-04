//! `ln(x)` — natural logarithm, plus `log10(x)` derived as `ln(x) · (1/ln(10))`.
//!
//! ## Algorithm
//!
//! 1. Special cases:
//!    * NaN propagates; sNaN raises `INVALID`.
//!    * `ln(0) = −∞ + DIV_BY_ZERO` per IEEE 754 §9.3.
//!    * `ln(negative_finite) = NaN + INVALID`.
//!    * `ln(+∞) = +∞`. `ln(−∞) = NaN + INVALID`.
//!    * `ln(1) = +0`.
//! 2. Decompose `x = m · 10^q` with `m ∈ [1, 10)`. Then
//!
//!    ```text
//!    ln(x) = ln(m) + q · ln(10)
//!    ```
//!
//! 3. Reduce `m` further: while `m > 1.5`, divide by 2 and add `ln(2)`
//!    (and below `2/3` for the symmetric branch). After this,
//!    `m ∈ [2/3, 3/2]`, so the Taylor series for
//!    `ln(1 + u)` (`u = m − 1`, `|u| ≤ 1/2`) converges to
//!    EXT_PRECISION = 50 digits in well under 200 terms.
//! 4. `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …`. Halt when terms fall
//!    below `EXT_PRECISION` significance.
//!
//! All intermediate work runs at extended precision (`Extended`, see
//! [`super::extended`]). The final rounding to `Decimal128` happens
//! once at the end via `round_and_pack_finite`, so the result is
//! faithfully rounded (≤ 1 ULP) against `astro-float`.

use crate::bid::{classify_bits, decimal_digit_count, pack_finite, Class, BIAS};
use crate::decimal::Decimal128;
use crate::math::consts::{inv_ln10_ext, inv_ln2_ext, ln10, ln10_ext, ln2_ext};
use crate::math::extended::Extended;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Natural logarithm `ln(self)`.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        ln_kernel(self, rm)
    }

    /// Base-10 logarithm `log10(self)`. Computed as
    /// `ln_extended(self) · (1/ln(10))_extended`, then rounded once.
    #[must_use]
    pub fn log10(self, rm: RoundingMode) -> (Self, Status) {
        log10_kernel(self, rm)
    }

    /// Base-2 logarithm `log2(self)`. Computed as
    /// `ln_extended(self) · (1/ln(2))_extended`, then rounded once.
    #[must_use]
    pub fn log2(self, rm: RoundingMode) -> (Self, Status) {
        log2_kernel(self, rm)
    }
}

fn ln_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp(Decimal128::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (Decimal128::ZERO, Status::OK);
    }
    let result_ext = ln_extended(x);
    let (result, status) = result_ext.to_decimal128(0, rm);
    (result, status | Status::INEXACT)
}

fn log10_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp(Decimal128::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (Decimal128::ZERO, Status::OK);
    }
    // log10(x) = ln(x) · (1/ln(10)) at extended precision.
    let ln_ext = ln_extended(x);
    let result_ext = ln_ext.mul(inv_ln10_ext());
    let (result, status) = result_ext.to_decimal128(0, rm);
    (result, status | Status::INEXACT)
}

fn log2_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp(Decimal128::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (Decimal128::ZERO, Status::OK);
    }
    let ln_ext = ln_extended(x);
    let result_ext = ln_ext.mul(inv_ln2_ext());
    let (result, status) = result_ext.to_decimal128(0, rm);
    (result, status | Status::INEXACT)
}

/// Short-circuit the special cases shared by `ln` and `log10`.
fn ln_special_cases(x: Decimal128) -> Option<(Decimal128, Status)> {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { .. } => Some((Decimal128::NAN, Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { sign } => Some(if sign {
            (Decimal128::NAN, Status::INVALID)
        } else {
            (Decimal128::INFINITY, Status::OK)
        }),
        Class::Zero { .. } => Some((Decimal128::NEG_INFINITY, Status::DIV_BY_ZERO)),
        Class::Finite { sign, .. } if sign => Some((Decimal128::NAN, Status::INVALID)),
        Class::Finite { .. } => None,
    }
}

/// Compute `ln(x)` at extended precision. Caller has already filtered
/// NaN / Inf / zero / negative inputs and the `x == 1` edge case.
pub(super) fn ln_extended(x: Decimal128) -> Extended {
    let (m, q) = decompose_to_decade(x);

    // Reduce m into [2/3, 3/2] by halving/doubling.
    let mut m_ext = Extended::from_decimal128(m);
    let mut additional = Extended::ZERO;
    let ln2_v = ln2_ext();
    let upper = Extended::parse_str("1.5");
    let lower = Extended::parse_str("0.6666666666666666666666666666666666666666666666666667");

    // At most ~5 iterations to reach the target window (each halve/double
    // contracts by 2× and m starts in [1, 10)).
    let mut guard = 0u32;
    while guard < 20 {
        guard += 1;
        if m_ext.cmp(upper) == core::cmp::Ordering::Greater {
            m_ext = m_ext.div_u32(2);
            additional = additional.add(ln2_v);
            continue;
        }
        if m_ext.cmp(lower) == core::cmp::Ordering::Less {
            m_ext = m_ext.mul(Extended::from_i32(2));
            additional = additional.sub(ln2_v);
            continue;
        }
        break;
    }

    // u = m − 1, |u| ≤ 0.5.
    let u = m_ext.sub(Extended::ONE);
    let ln_m = taylor_log1p_ext(u);

    // ln(original_m) = ln_m + accumulated halve/double corrections.
    let ln_orig_m = ln_m.add(additional);

    // Combine: ln(x) = ln(m) + q · ln(10).
    if q == 0 {
        return ln_orig_m;
    }
    let q_ln10 = Extended::from_i32(q).mul(ln10_ext());
    ln_orig_m.add(q_ln10)
}

/// `x = m × 10^q` with `m ∈ [1, 10)`. Same logic as the legacy
/// implementation; reused verbatim because the input handling is
/// orthogonal to the precision of the work that follows.
fn decompose_to_decade(x: Decimal128) -> (Decimal128, i32) {
    match classify_bits(x.to_bits()) {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            let digits = decimal_digit_count(coefficient) as i32;
            let m_quantum = -(digits - 1);
            let q = unbiased + digits - 1;
            let m = Decimal128::from_bits(pack_finite(
                sign,
                (m_quantum + BIAS as i32) as u32,
                coefficient,
            ));
            (m, q)
        }
        _ => unreachable!("decompose_to_decade called on non-finite"),
    }
}

/// Taylor series `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …` at
/// extended precision. Halts when adding the next term doesn't change
/// the partial sum at 50-digit precision.
fn taylor_log1p_ext(u: Extended) -> Extended {
    let mut sum = Extended::ZERO;
    let mut power = Extended::ONE; // u^0; updated to u^n inside the loop
    let mut sign_alt = false;

    // |u| ≤ 0.5 → |u^n / n| ≤ 0.5^n / n. To drive the term below
    // 10^{-50} we need n large enough that 0.5^n < 10^{-50} · n,
    // i.e. n ≳ 50 · log2(10) / 1 ≈ 166. Cap at 250 for safety.
    for n in 1u32..=250 {
        let new_power = power.mul(u);
        power = new_power;
        let term = power.div_u32(n);
        let signed = if sign_alt { term.neg() } else { term };
        let next_sum = sum.add(signed);
        sign_alt = !sign_alt;
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if power.is_zero() {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
        let (diff, _) = got.sub(want, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_want = want.abs();
        if abs_want.is_zero() {
            let bound = parse(&alloc::format!("{ulps}e-33"));
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
    fn ln_one_is_zero() {
        let (r, _) = Decimal128::ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn ln_e_is_one() {
        let e_val = crate::math::e();
        let (r, _) = e_val.ln(RoundingMode::NearestEven);
        assert!(within_ulps(r, Decimal128::ONE, 1));
    }

    #[test]
    fn ln_ten_is_ln10_const() {
        let ten = Decimal128::TEN;
        let (r, _) = ten.ln(RoundingMode::NearestEven);
        let target = ln10();
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn ln_two_is_ln2_const() {
        let two = Decimal128::from_i32(2);
        let (r, _) = two.ln(RoundingMode::NearestEven);
        let target = parse("0.693147180559945309417232121458176568");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn ln_zero_is_neg_inf_div_by_zero() {
        let (r, s) = Decimal128::ZERO.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn ln_negative_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_inf_is_inf() {
        let (r, _) = Decimal128::INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());

        let (r, s) = Decimal128::NEG_INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_nan_propagates() {
        let (r, _) = Decimal128::NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn log10_powers_of_ten() {
        for &p in &[1i32, 2, 3, 4, 5, 10, -1, -3, 100, -100] {
            let x = parse(&alloc::format!("1e{p}"));
            let (r, _) = x.log10(RoundingMode::NearestEven);
            let target = Decimal128::from_i32(p);
            assert!(
                within_ulps(r, target, 1),
                "log10(1e{p}) = {r:?}, want {target:?}"
            );
        }
    }

    #[test]
    fn ln_exp_roundtrip() {
        for &v in &["0.5", "1.5", "2", "5", "10", "100"] {
            let x = parse(v);
            let (lx, _) = x.ln(RoundingMode::NearestEven);
            let (back, _) = lx.exp(RoundingMode::NearestEven);
            assert!(
                within_ulps(back, x, 5),
                "exp(ln({v})) = {back:?}, want {x:?}"
            );
        }
    }
}
