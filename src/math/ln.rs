//! `ln(x)` — natural logarithm, plus `log10(x)` derived as `ln(x)/ln(10)`.
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
//! 3. Reduce `m` further: while `m > sqrt(2) ≈ 1.414`, divide by 2 and
//!    add `ln(2)`. (Equivalent reduction below `1/sqrt(2)` for the
//!    `m < 1` branch.) After this, `m ∈ [1/sqrt(2), sqrt(2)]`, so the
//!    Taylor series for `ln(1 + u)` (`u = m - 1`) converges in well
//!    under 100 terms at 34-digit precision.
//! 4. `ln(1 + u) = u − u^2/2 + u^3/3 − u^4/4 + …`. Halt when terms
//!    fall below the precision threshold.
//!
//! Same v1 accuracy caveats as `exp`: native Decimal128 arithmetic
//! everywhere, so the result drifts up to a few ULP. Faithful rounding
//! is a follow-up that needs a wider intermediate type.

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal128;
use crate::math::consts::{ln10, ln2};
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Natural logarithm `ln(self)`.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        ln_kernel(self, rm)
    }

    /// Base-10 logarithm `log10(self)`. Computed as `ln(self) / ln(10)`.
    #[must_use]
    pub fn log10(self, rm: RoundingMode) -> (Self, Status) {
        let (l, s1) = self.ln(rm);
        if !l.is_finite() {
            // NaN / ±∞ propagate without the divide.
            return (l, s1);
        }
        let (r, s2) = l.div(ln10(), rm);
        (r, s1 | s2)
    }
}

fn ln_kernel(x: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return if sign {
                (Decimal128::NAN, Status::INVALID)
            } else {
                (Decimal128::INFINITY, Status::OK)
            };
        }
        Class::Zero { .. } => {
            return (Decimal128::NEG_INFINITY, Status::DIV_BY_ZERO);
        }
        Class::Finite { sign, .. } if sign => {
            return (Decimal128::NAN, Status::INVALID);
        }
        Class::Finite { .. } => {}
    }

    if matches!(x.partial_cmp(Decimal128::ONE).0, Some(core::cmp::Ordering::Equal)) {
        return (Decimal128::ZERO, Status::OK);
    }

    // Decompose x = m × 10^q with m ∈ [1, 10).
    let (m, q) = decompose_to_decade(x);

    // Compute ln(m).
    let (ln_m, mut status) = ln_in_decade(m, rm);

    // Combine: ln(x) = ln(m) + q · ln(10).
    let q_dec = Decimal128::from_i32(q);
    let (q_ln10, s1) = q_dec.mul(ln10(), RoundingMode::NearestEven);
    status |= s1;
    let (result, s2) = ln_m.add(q_ln10, rm);
    status |= s2;
    (result, status)
}

/// `x = m × 10^q` with `m ∈ [1, 10)`. We extract `q` from the bid
/// decomposition (digit count + biased exponent) and re-encode `m` at
/// quantum 0... or rather, at the natural quantum that places one
/// significant digit before the decimal point.
fn decompose_to_decade(x: Decimal128) -> (Decimal128, i32) {
    use crate::bid::{decimal_digit_count, pack_finite};
    match classify_bits(x.to_bits()) {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            let digits = decimal_digit_count(coefficient) as i32;
            // Magnitude = c × 10^unbiased, in [10^(unbiased + digits − 1),
            // 10^(unbiased + digits)). For m ∈ [1, 10) we want
            // m_quantum = -(digits − 1), so coefficient at that quantum
            // gives a value in [1, 10).
            let m_quantum = -(digits - 1);
            let q = unbiased + digits - 1;
            // Re-encode coefficient with the new quantum. The biased
            // exponent for m_quantum is m_quantum + BIAS; coefficient
            // unchanged.
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

/// Compute `ln(m)` where `m ∈ [1, 10)`.
///
/// Reduce by repeated division by 2: while `m > 1.5`, divide by 2 and
/// add `ln(2)`. This contracts `m` into roughly `[0.75, 1.5]` where
/// the Taylor series converges quickly.
fn ln_in_decade(m: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let _ = rm;
    let mut status = Status::OK;
    let mut m = m;
    let mut additional = Decimal128::ZERO;
    let two = Decimal128::from_i32(2);
    let upper = Decimal128::parse_str("1.5", RoundingMode::NearestEven)
        .expect("literal")
        .0;
    let lower = Decimal128::parse_str("0.6666666666666666666666666666666667", RoundingMode::NearestEven)
        .expect("literal")
        .0;

    let ln2_v = ln2();

    // Pull `m` into [lower, upper] by halving / doubling. Bounded:
    // each step at most 2× contraction, so at most ~5 iterations to
    // reach the target window starting from m ∈ [1, 10).
    let mut guard = 0;
    while guard < 20 {
        guard += 1;
        let (cmp_hi, _) = m.partial_cmp(upper);
        if matches!(cmp_hi, Some(core::cmp::Ordering::Greater)) {
            let (next, st) = m.div(two, RoundingMode::NearestEven);
            status |= st;
            m = next;
            let (next_add, st) = additional.add(ln2_v, RoundingMode::NearestEven);
            status |= st;
            additional = next_add;
            continue;
        }
        let (cmp_lo, _) = m.partial_cmp(lower);
        if matches!(cmp_lo, Some(core::cmp::Ordering::Less)) {
            let (next, st) = m.mul(two, RoundingMode::NearestEven);
            status |= st;
            m = next;
            let (next_add, st) = additional.sub(ln2_v, RoundingMode::NearestEven);
            status |= st;
            additional = next_add;
            continue;
        }
        break;
    }

    // u = m - 1, |u| ≤ 0.5.
    let (u, st_u) = m.sub(Decimal128::ONE, RoundingMode::NearestEven);
    status |= st_u;

    // Taylor: ln(1 + u) = u - u²/2 + u³/3 - ...
    let (ln_m, st_taylor) = taylor_log1p(u);
    status |= st_taylor;

    // ln(original_m) = ln_m + additional
    let (combined, st_add) = ln_m.add(additional, RoundingMode::NearestEven);
    status |= st_add;
    (combined, status)
}

/// Taylor series for `ln(1 + u)`. Halts when adding the next term
/// no longer changes the partial sum at Decimal128 precision.
fn taylor_log1p(u: Decimal128) -> (Decimal128, Status) {
    let mut status = Status::INEXACT;
    let mut sum = Decimal128::ZERO;
    let mut power = Decimal128::ONE; // u^0; updated to u^n inside loop
    let mut sign_alt = false;
    let max_iterations = 200;
    for n in 1..=max_iterations {
        let (new_power, s1) = power.mul(u, RoundingMode::NearestEven);
        status |= s1;
        power = new_power;
        let n_dec = Decimal128::from_i32(n);
        let (term, s2) = power.div(n_dec, RoundingMode::NearestEven);
        status |= s2;
        let signed_term = if sign_alt { term.neg() } else { term };
        let (next_sum, s3) = sum.add(signed_term, RoundingMode::NearestEven);
        status |= s3;
        let (cmp, _) = next_sum.partial_cmp(sum);
        sum = next_sum;
        sign_alt = !sign_alt;
        if cmp == Some(core::cmp::Ordering::Equal) {
            break;
        }
        if power.is_zero() {
            break;
        }
    }
    (sum, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::string::ToString;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn approx_equal_ulps(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_b = b.abs();
        if abs_b.is_zero() {
            // Compare to absolute tolerance instead.
            let bound = parse(&alloc::format!("{ulps}e-33"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
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
        assert!(approx_equal_ulps(r, Decimal128::ONE, 10));
    }

    #[test]
    fn ln_ten_is_ln10_const() {
        let ten = Decimal128::TEN;
        let (r, _) = ten.ln(RoundingMode::NearestEven);
        let target = ln10();
        assert!(approx_equal_ulps(r, target, 10));
    }

    #[test]
    fn ln_two_is_ln2_const() {
        let two = Decimal128::from_i32(2);
        let (r, _) = two.ln(RoundingMode::NearestEven);
        let target = ln2();
        assert!(approx_equal_ulps(r, target, 10));
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
        for &p in &[1i32, 2, 3, 4, 5, 10, -1, -3] {
            let x = parse(&alloc::format!("1e{p}"));
            let (r, _) = x.log10(RoundingMode::NearestEven);
            let target = Decimal128::from_i32(p);
            assert!(
                approx_equal_ulps(r, target, 50),
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
                approx_equal_ulps(back, x, 100),
                "exp(ln({v})) = {back:?}, want {x:?}"
            );
        }
    }
}
