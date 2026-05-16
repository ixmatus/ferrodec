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

use crate::bid::{classify_bits, Class, BIAS, PRECISION};
use crate::decimal::Decimal64;
use ferrodec_ieee::{decimal_digit_count_u128, RoundingMode, Status};

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
    ///
    /// Implemented as `add(self, −other)`, but the negation is
    /// applied only when `other` is *not* a NaN. GDA / IEEE 754-2019
    /// require that subtraction does not affect the sign of a NaN:
    /// `subtract x NaN` propagates `NaN` (and `subtract x -NaN`
    /// propagates `-NaN`) with the operand's original sign. Negating
    /// the NaN operand unconditionally, as an earlier revision did,
    /// flipped that sign and disagreed with the `ddSubtract.decTest`
    /// corpus (`ddsub830..` etc.). For non-NaN operands `neg()` only
    /// toggles the sign bit, so the finite / infinite / zero cases
    /// reduce to `add` on the negated operand exactly as before,
    /// including the IEEE sign-of-zero rules.
    #[must_use]
    pub fn sub(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let rhs = if other.is_nan() { other } else { other.neg() };
        add_inner(self, rhs, rm)
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the finite-finite alignment / rounding pipeline.
    /// Mirrors decimal128's `add_special_only_for_kani` (ADR-0016).
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn add_special_only_for_kani(self, rhs: Self, rm: RoundingMode) -> Option<(Self, Status)> {
        handle_specials(classify_bits(self.0), classify_bits(rhs.0), rm)
    }

    /// Kani-only entry point for `sub`'s special path. Mirrors
    /// [`Decimal64::sub`]: the operand is negated only when it is not
    /// a NaN, so NaN-sign propagation and sNaN INVALID match the
    /// production path and cannot drift.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sub_special_only_for_kani(self, rhs: Self, rm: RoundingMode) -> Option<(Self, Status)> {
        let negated = if rhs.is_nan() { rhs } else { rhs.neg() };
        handle_specials(classify_bits(self.0), classify_bits(negated.0), rm)
    }
}

fn add_inner(a: Decimal64, b: Decimal64, rm: RoundingMode) -> (Decimal64, Status) {
    let ca = classify_bits(a.0);
    let cb = classify_bits(b.0);

    if let Some(out) = handle_specials(ca, cb, rm) {
        return out;
    }

    let (sign_a, biased_a, coef_a) = match ca {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite handled by dispatcher"),
    };
    let (sign_b, biased_b, coef_b) = match cb {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite handled by dispatcher"),
    };

    let exp_a = biased_a as i32 - BIAS as i32;
    let exp_b = biased_b as i32 - BIAS as i32;

    if coef_a == 0 && coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        // Both `exp_a` and `exp_b` came from `classify_bits`, so
        // `q_preferred ∈ [-BIAS, BIASED_EXP_MAX - BIAS as i32]` and the
        // unbiased-to-biased conversion is in range.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_preferred)
            .expect("q_preferred from classify_bits-derived exponents");
        return (
            Decimal64::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                crate::bid::Coefficient::ZERO,
            )),
            Status::OK,
        );
    }

    // H1 fix (`ddadd360`): when exactly one operand is zero, the
    // result is the other operand requantised to
    // `q_preferred = min(exp_a, exp_b)` per IEEE 754-2019 §5.4.1 +
    // §6.3 (preferred quantum for additive operations). Without this
    // short-circuit, an exponent gap exceeding `WORKING_PRECISION`
    // collapses both aligned magnitudes to zero in the diff-too-wide
    // branch below (line ~158), and the rounding funnel returns
    // `0E+exp_hi` instead of the non-zero operand's value. The fix
    // also subsumes Agent 1 F3's `aligned_hi == aligned_lo`
    // degenerate case for opposite signs — that branch is reachable
    // only when both aligned magnitudes are zero, which (with the
    // both-zero early return above) requires at least one operand
    // to carry `coef == 0`.
    if coef_a == 0 {
        let q_preferred = exp_a.min(exp_b);
        return round_and_pack_finite(coef_b, exp_b, q_preferred, sign_b, false, rm, Status::OK);
    }
    if coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        return round_and_pack_finite(coef_a, exp_a, q_preferred, sign_a, false, rm, Status::OK);
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

    let (combined_coef, combined_sign, h2_borrow) = if sign_hi == sign_lo {
        (aligned_hi + aligned_lo, sign_hi, false)
    } else if aligned_hi > aligned_lo {
        // H2 fix candidate: lo's truncated residue subtracts from
        // the result magnitude. See the H2 block below for the
        // borrow-and-extend transformation.
        (aligned_hi - aligned_lo, sign_hi, pre_sticky)
    } else if aligned_lo > aligned_hi {
        // Symmetric case: lo dominates AND carries the residue, so
        // the residue is additive (`combined_coef + ε_lo` is the
        // true magnitude). The funnel's `pre_sticky = true` encoding
        // is already correct here; no borrow.
        (aligned_lo - aligned_hi, sign_lo, false)
    } else {
        let q_preferred = exp_a.min(exp_b);
        if pre_sticky {
            return round_and_pack_into_u64(1, exp_lo, q_preferred, sign_lo, false, rm);
        }
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        // As in the both-zero early return above, `q_preferred` is
        // bounded by the classify_bits-derived exponent range.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_preferred)
            .expect("q_preferred from classify_bits-derived exponents");
        return (
            Decimal64::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                crate::bid::Coefficient::ZERO,
            )),
            Status::OK,
        );
    };

    // H2 fix (`ddadd71100..71119` + 19 mirrors + 1 `ddMultiply` case +
    // 20 `ddFMA` mirrors): when the hi-magnitude operand dominates an
    // effective subtraction AND lo had a truncated sub-ULP residue,
    // the result's true value sits BELOW `combined_coef × 10^align_exp`
    // by some ε ∈ (0, 1) ULP at `align_exp`, not above. The funnel's
    // `pre_sticky = true` convention encodes residue-above-LSB; under
    // directional rounding modes (TowardZero, Ceiling, Floor) and at
    // exact half-ULP ties under round-half-even, the wrong sign on
    // the residue picks the wrong neighbour by one ULP. Borrow one
    // ULP from `combined_coef` and extend the bottom digits to a
    // `PRECISION`-digit cohort, turning the encoding into a
    // correctly-signed positive sticky at a lower quantum.
    //
    // For `combined_coef ≥ PRECISION` digits the funnel will compress
    // and round; a plain `-1` suffices. For fewer digits we choose
    // `k` such that `combined_coef × 10^k - 1` has exactly
    // `PRECISION` digits (one extra `k` is needed when `combined_coef`
    // is itself a power of 10, where the borrow drops the leading
    // digit).
    let (combined_coef, align_exp) = if h2_borrow {
        let combined_digits = decimal_digit_count_u128(combined_coef);
        if combined_digits >= PRECISION {
            (combined_coef - 1, align_exp)
        } else {
            let is_power_of_10 = combined_coef == POW10_U128[(combined_digits - 1) as usize];
            let k = if is_power_of_10 {
                PRECISION + 1 - combined_digits
            } else {
                PRECISION - combined_digits
            };
            (
                combined_coef * POW10_U128[k as usize] - 1,
                align_exp - k as i32,
            )
        }
    } else {
        (combined_coef, align_exp)
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
    // `c < keep_threshold` holds by the loop exit condition (it is the
    // negation of the `while` guard), so no assertion is needed for
    // it. The `c as u64` cast below is sound because `keep_threshold`
    // is bounded by `10^WORKING_PRECISION ≤ u64::MAX`, so `c` (now
    // below the threshold) fits in a `u64`.
    debug_assert!(
        c <= u128::from(u64::MAX),
        "coefficient fits u64 for the cast"
    );

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
    use crate::bid::{pack_finite, BiasedExp, Coefficient, BIAS};

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
        let (r, _) =
            from_int(9_999_999_999_999_999, 0).add(from_int(1, 0), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS + 1).unwrap(),
            Coefficient::try_new(1_000_000_000_000_000).unwrap(),
        ));
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
    fn add_h2_effective_subtract_residue_borrows_correctly() {
        // H2 regression (`ddAdd.decTest:802`, case `ddadd71100`):
        // `add 1e+2 -1e-383` under TowardZero should return
        // `99.99999999999999` (16 nines) per the residue-from-lo
        // sub-ULP subtractive direction. Without the H2 borrow, the
        // result was `100.0000000000000` — one ULP above the true
        // value because the funnel read the residue as additive.
        let a = Decimal64::try_new(1, 2).unwrap();
        let b = Decimal64::try_new(-1, -383).unwrap();
        let (r, status) = a.add(b, RoundingMode::TowardZero);
        let expected = Decimal64::try_new(9_999_999_999_999_999, -14).unwrap();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "1e+2 + (-1e-383) under TowardZero should equal 99.99999999999999, got {r:?}"
        );
        assert!(status.inexact());

        // Negated mirror (the `ddadd71200..71219` family): negate
        // both operands and the result should be the negation.
        let neg_a = a.neg();
        let neg_b = b.neg();
        let (r, _) = neg_a.add(neg_b, RoundingMode::TowardPositive);
        let expected = expected.neg();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "(-1e+2) + (1e-383) under TowardPositive should equal -99.99999999999999, got {r:?}"
        );
    }

    #[test]
    fn add_h1_asymmetric_zero_at_far_exponent_keeps_magnitude() {
        // H1 regression (`ddAdd.decTest:358`, case `ddadd360`).
        // `add 0E+50 10000E+1` under any rounding mode should return
        // `1.0000E+5` per IEEE 754-2019 §5.4.1 (`x + 0 = x` with
        // preferred quantum `min(quantum(x), quantum(0))`). Before the
        // fix, the diff-too-wide alignment branch collapsed both
        // aligned magnitudes to zero and the rounding funnel returned
        // `0E+50`.
        let a = Decimal64::try_new(0, 50).unwrap();
        let b = Decimal64::try_new(10000, 1).unwrap();
        let (r, status) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal64::try_new(10000, 1).unwrap();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "0E+50 + 10000E+1 should equal 1.0000E+5, got {r:?}"
        );
        assert!(
            status.is_ok(),
            "0E+50 + 10000E+1 is exact, expected no flags raised, got {status:?}"
        );

        // Symmetric: zero on the right operand.
        let (r, status) = b.add(a, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(status.is_ok());

        // Negative zero preserves the non-zero operand's sign per §6.3.
        let neg_zero = Decimal64::try_new(0, 50).unwrap().neg();
        let (r, _) = neg_zero.add(b, RoundingMode::NearestEven);
        assert!(
            !r.is_sign_negative(),
            "(-0E+50) + (+10000E+1) should be positive"
        );

        // Negative non-zero operand: the non-zero operand's sign wins.
        let neg_b = b.neg();
        let (r, _) = a.add(neg_b, RoundingMode::NearestEven);
        assert!(
            r.is_sign_negative(),
            "(+0E+50) + (-10000E+1) should be negative"
        );
    }

    #[test]
    fn add_with_alignment() {
        // 1 + 0.5 = 1.5
        let a = from_int(1, 0);
        let b = from_int(5, -1);
        let (r, _) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 1).unwrap(),
            Coefficient::try_new(15).unwrap(),
        ));
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
    fn sub_does_not_flip_nan_sign() {
        // Regression (ddSubtract.decTest ddsub830.., F1): subtraction
        // must not negate the sign of a NaN operand. `subtract x NaN`
        // propagates `NaN`; `subtract x -NaN` propagates `-NaN`.
        let (r, s) = from_int(1000, 0).sub(Decimal64::NAN, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && !r.is_sign_negative());
        assert!(s.is_ok());

        let neg_nan = Decimal64::NAN.neg();
        let (r, _) = from_int(1000, 0).sub(neg_nan, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && r.is_sign_negative());

        let (r, _) = Decimal64::NEG_INFINITY.sub(Decimal64::NAN, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && !r.is_sign_negative());

        // sNaN still raises INVALID and is quieted; sign unflipped.
        let (r, s) = from_int(-1, 0).sub(Decimal64::SIGNALING_NAN, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && !r.is_sign_negative());
        assert!(s.invalid());
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
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 2).unwrap(),
            Coefficient::try_new(123).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
