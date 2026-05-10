//! IEEE 754-2019 add and subtract for [`Decimal64`].
//!
//! Same algorithmic shape as ferrodec-decimal32's addsub but at
//! `u128` working width, since Decimal64's 16-digit coefficients
//! plus alignment shifts can exceed `u64`.
//!
//! # Working precision
//!
//! Decimal64 coefficients fit in 53 bits. The maximum
//! `coef_hi × 10^diff` we can shift without overflowing `u128` (max
//! ≈ 3.4 × 10³⁸) is `coef_hi × 10²²`. So:
//!
//! * `diff ≤ 22`: shift the higher-quantum operand left by 10^diff;
//!   keep the lower-quantum operand as-is.
//! * `22 < diff ≤ 23`: truncate the lower operand by `10^(diff − 22)`
//!   with the residue feeding the sticky bit.
//! * `diff > 23`: the lower operand sits below the working window
//!   entirely; only its non-zeroness contributes.

use crate::bid::{classify_bits, BIAS, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

use super::round::round_and_pack_finite;

const POW10_U128: [u128; 24] = {
    let mut t = [0u128; 24];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 24 {
        t[i] = v;
        if i < 23 {
            v *= 10;
        }
        i += 1;
    }
    t
};

const ALIGN_LIMIT: u32 = 22;
const WORKING_PRECISION: u32 = 23;

// Compile-time invariants: every `POW10_U128[k]` access in this
// module must satisfy `k < POW10_U128.len()`. The largest index
// reachable is `ALIGN_LIMIT = 22` (the cap on per-side alignment
// shift), so we need at least 23 entries.
const _: () = assert!(POW10_U128.len() > ALIGN_LIMIT as usize);
const _: () = assert!(POW10_U128.len() > WORKING_PRECISION as usize - 1);

impl Decimal64 {
    /// IEEE 754-2019 `addition(self, other)` rounded by `rm`.
    #[must_use]
    pub fn add(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        add_inner(self, other, rm)
    }

    /// IEEE 754-2019 `subtraction(self, other)` rounded by `rm`.
    #[must_use]
    pub fn sub(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        add_inner(self, other.neg(), rm)
    }
}

fn add_inner(a: Decimal64, b: Decimal64, rm: RoundingMode) -> (Decimal64, Status) {
    let ca = classify_bits(a.0);
    let cb = classify_bits(b.0);

    if let Some(out) = handle_specials(ca, cb, rm) {
        return out;
    }

    let (sign_a, biased_a, coef_a) = match ca {
        Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite handled by dispatcher"),
    };
    let (sign_b, biased_b, coef_b) = match cb {
        Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite handled by dispatcher"),
    };

    let exp_a = biased_a as i32 - BIAS as i32;
    let exp_b = biased_b as i32 - BIAS as i32;

    if coef_a == 0 && coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        return (
            Decimal64::from_bits(crate::bid::pack_finite(
                result_sign,
                (q_preferred + BIAS as i32) as u32,
                0,
            )),
            Status::OK,
        );
    }

    let (sign_hi, exp_hi, coef_hi, sign_lo, exp_lo, coef_lo) = if exp_a >= exp_b {
        (sign_a, exp_a, coef_a, sign_b, exp_b, coef_b)
    } else {
        (sign_b, exp_b, coef_b, sign_a, exp_a, coef_a)
    };

    let diff = (exp_hi - exp_lo) as u32;

    let (aligned_hi, aligned_lo, align_exp, pre_sticky): (u128, u128, i32, bool) =
        if diff <= ALIGN_LIMIT {
            let shifted = u128::from(coef_hi) * POW10_U128[diff as usize];
            (shifted, u128::from(coef_lo), exp_lo, false)
        } else if diff <= WORKING_PRECISION {
            let trim = diff - ALIGN_LIMIT;
            let factor = POW10_U128[trim as usize];
            let trunc_lo = u128::from(coef_lo) / factor;
            let pre_sticky = (u128::from(coef_lo) % factor) != 0;
            let shifted_hi = u128::from(coef_hi) * POW10_U128[ALIGN_LIMIT as usize];
            (
                shifted_hi,
                trunc_lo,
                exp_hi - ALIGN_LIMIT as i32,
                pre_sticky,
            )
        } else {
            (u128::from(coef_hi), 0, exp_hi, coef_lo != 0)
        };

    let (combined_coef, combined_sign) = if sign_hi == sign_lo {
        (aligned_hi + aligned_lo, sign_hi)
    } else if aligned_hi > aligned_lo {
        (aligned_hi - aligned_lo, sign_hi)
    } else if aligned_lo > aligned_hi {
        (aligned_lo - aligned_hi, sign_lo)
    } else {
        let q_preferred = exp_a.min(exp_b);
        if pre_sticky {
            return round_and_pack_into_u64(
                1,
                exp_lo,
                q_preferred,
                sign_lo,
                false,
                rm,
            );
        }
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        return (
            Decimal64::from_bits(crate::bid::pack_finite(
                result_sign,
                (q_preferred + BIAS as i32) as u32,
                0,
            )),
            Status::OK,
        );
    };

    let q_preferred = exp_a.min(exp_b);
    round_and_pack_into_u64(
        combined_coef,
        align_exp,
        q_preferred,
        combined_sign,
        pre_sticky,
        rm,
    )
}

/// Compress a `u128` coefficient down to `u64` (with sticky tracking)
/// and route through `round_and_pack_finite`. Decimal64 rounds at
/// PRECISION (= 16) digits, so we keep enough headroom in the u64 to
/// preserve the rounding decision (~19 retained digits suffices).
pub(crate) fn round_and_pack_into_u64(
    coef_u128: u128,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    mut pre_sticky: bool,
    rm: RoundingMode,
) -> (Decimal64, Status) {
    // KEEP = 19 fits the post-compression value into u64:
    // 10^19 = 10_000_000_000_000_000_000 < u64::MAX =
    // 18_446_744_073_709_551_615. Bumping KEEP to 20 would
    // overflow u64 silently; the invariant below catches the
    // regression at compile time. (u128::MAX comparison avoids
    // the trivially-true `u64 ≤ u64::MAX` form that clippy
    // diagnoses.)
    const KEEP: u32 = 19;
    const _: () = assert!(10u128.pow(KEEP) < 18_446_744_073_709_551_616u128);
    let keep_threshold = 10u128.pow(KEEP);

    if coef_u128 < keep_threshold {
        return round_and_pack_finite(
            coef_u128 as u64,
            unbiased_exp,
            q_preferred,
            sign,
            pre_sticky,
            rm,
            Status::OK,
        );
    }

    let mut c = coef_u128;
    let mut shift = 0u32;
    while c >= keep_threshold {
        let r = c % 10;
        c /= 10;
        if r != 0 {
            pre_sticky = true;
        }
        shift += 1;
    }
    debug_assert!(c < keep_threshold);
    debug_assert!(c <= u128::from(u64::MAX));

    round_and_pack_finite(
        c as u64,
        unbiased_exp + shift as i32,
        q_preferred,
        sign,
        pre_sticky,
        rm,
        Status::OK,
    )
}

#[inline]
fn zero_sum_sign(sign_a: bool, sign_b: bool, rm: RoundingMode) -> bool {
    if sign_a == sign_b {
        return sign_a;
    }
    matches!(rm, RoundingMode::TowardNegative)
}

fn handle_specials(a: Class, b: Class, rm: RoundingMode) -> Option<(Decimal64, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    match (a, b) {
        (SignalingNaN { sign, payload }, _) | (_, SignalingNaN { sign, payload }) => {
            return Some((
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
        _ => {}
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    match (a, b) {
        (Infinity { sign: sa }, Infinity { sign: sb }) => {
            if sa == sb {
                Some((
                    Decimal64::from_bits(crate::bid::pack_infinity(sa)),
                    Status::OK,
                ))
            } else {
                Some((Decimal64::NAN, Status::INVALID))
            }
        }
        (Infinity { sign }, _) | (_, Infinity { sign }) => Some((
            Decimal64::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        )),
        (Zero { .. } | Finite { .. }, Zero { .. } | Finite { .. }) => {
            let _ = rm;
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BIAS};

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn add_basic_integers() {
        let (r, s) = from_int(1, 0).add(from_int(1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
        assert!(s.is_ok());

        let (r, _) = from_int(123, 0).add(from_int(456, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(579, 0).to_bits());
    }

    #[test]
    fn add_with_carry_renormalises() {
        // 9_999_999_999_999_999 + 1 = 10^16 → renormalises.
        let (r, _) = from_int(9_999_999_999_999_999, 0)
            .add(from_int(1, 0), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS + 1, 1_000_000_000_000_000));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn add_signs_differ_cancellation() {
        let (r, _) = from_int(1, 0).add(from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(1, 0).add(from_int(-1, 0), RoundingMode::TowardNegative);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn add_zero_plus_zero() {
        let (r, _) = Decimal64::ZERO.add(Decimal64::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ZERO.to_bits());

        let (r, _) = Decimal64::ZERO.add(Decimal64::NEG_ZERO, RoundingMode::NearestEven);
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal64::ZERO.add(Decimal64::NEG_ZERO, RoundingMode::TowardNegative);
        assert!(r.is_sign_negative());
    }

    #[test]
    fn add_with_alignment() {
        // 1 + 0.5 = 1.5
        let a = from_int(1, 0);
        let b = from_int(5, -1);
        let (r, _) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 1, 15));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn add_with_far_alignment_inexact() {
        // 1 + 1e-30: 1e-30 well below the working window.
        let a = from_int(1, 0);
        let b = from_int(1, -30);
        let (r, s) = a.add(b, RoundingMode::NearestEven);
        assert!(r.is_finite() && !r.is_sign_negative());
        assert!(s.inexact());
    }

    #[test]
    fn sub_basic() {
        let (r, _) = from_int(5, 0).sub(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());

        let (r, _) = from_int(1, 0).sub(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn nan_propagation() {
        let (r, s) = Decimal64::NAN.add(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.add(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn infinity_arithmetic() {
        let (r, s) = Decimal64::INFINITY.add(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        let (r, s) = Decimal64::INFINITY.add(Decimal64::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::INFINITY.sub(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn add_overflow_to_infinity() {
        let (r, s) = Decimal64::MAX.add(Decimal64::MAX, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn add_finite_zero_returns_finite() {
        let (r, _) = from_int(123, -2).add(Decimal64::ZERO, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 2, 123));
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
