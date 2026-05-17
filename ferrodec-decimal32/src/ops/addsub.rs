//! IEEE 754-2019 add and subtract for [`Decimal32`].
//!
//! Both ops return `(Decimal32, Status)`. Subtract is implemented as
//! `add(a, -b)`, with the sign-flip composed *before* the special-case
//! dispatcher so signaling NaN propagation is unaffected.
//!
//! # Algorithm
//!
//! 1. Special-case dispatcher (NaN, Infinity, Zero).
//! 2. Finite path: align coefficients over a `u128` working width
//!    with a *dynamic* per-side shift bound, sign-aware combine,
//!    effective-subtract borrow-and-extend, then route through
//!    [`round_and_pack_finite`](super::round::round_and_pack_finite)
//!    after compressing back to `u64`.
//!
//! # Working precision
//!
//! Decimal32 coefficients are below `10^7` (24 bits). The alignment
//! runs over a `u128` working register: `10^38 < 2^128`, so any
//! aligned coefficient with at most 38 decimal digits is exact.
//!
//! The shift is *keyed on the actual digit count of `coef_hi`*, not a
//! static window. `coef_hi` is shifted left by
//! `s = min(diff, U128_DIGIT_CAP − digits(coef_hi))`; the lower
//! operand is truncated only by the unavoidable remainder `diff − s`,
//! never more. When `s == diff` the alignment is exact and the
//! round-half-even decision at the precision boundary sees the true
//! residue rather than a prematurely collapsed sticky bit. A static
//! window (the prior `ALIGN_LIMIT = 12`) truncated `coef_lo` whenever
//! the gap exceeded 12 even when `coef_hi` had a single digit and the
//! full subtraction would have fit in `u128`, losing the
//! effective-subtract borrow on widely separated operands
//! (`KNOWN_ISSUES` H1/H2).

use crate::bid::{classify_bits, Class, BIAS, PRECISION};
use crate::decimal::Decimal32;
use ferrodec_ieee::{decimal_digit_count_u128, RoundingMode, Status};

use super::round::round_and_pack_finite;

const POW10_U128: [u128; 39] = {
    let mut t = [0u128; 39];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 39 {
        t[i] = v;
        if i < 38 {
            v *= 10;
        }
        i += 1;
    }
    t
};

/// A `u128` holds at most 38 decimal digits (`10^38 < 2^128 <
/// 10^39`). The higher-quantum operand is shifted left until
/// `digits(coef_hi) + shift` reaches this cap; the lower operand is
/// truncated only when the gap genuinely exceeds what `u128` can
/// hold. Keying the alignment on the *actual* digit count of
/// `coef_hi` (rather than a static `12`) keeps the full subtraction
/// exact whenever it fits, mirroring the in-crate `fma.rs`
/// dynamic-shift bound and the post-slice decimal64 `addsub.rs`.
const U128_DIGIT_CAP: u32 = 38;

// Compile-time invariant: the largest reachable `POW10_U128` index is
// a shift bounded by `U128_DIGIT_CAP`, so the table needs ≥ 39
// entries (indices `0..=38`).
const _: () = assert!(POW10_U128.len() > U128_DIGIT_CAP as usize);

impl Decimal32 {
    /// IEEE 754-2019 `addition(self, other)` rounded by `rm`.
    ///
    /// Returns `(result, Status)`. `Status::INEXACT` is set when the
    /// rounded result differs from the infinitely precise sum;
    /// `Status::INVALID` is set on signaling-NaN inputs and on
    /// `+∞ + (−∞)`.
    #[must_use]
    pub fn add(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        add_inner(self, other, rm)
    }

    /// IEEE 754-2019 `subtraction(self, other)` rounded by `rm`.
    ///
    /// Equivalent to `add(self, neg(other))` but quiets a signaling
    /// NaN in either operand (the negation does not strip the sNaN
    /// marker; the special-case dispatcher does).
    #[must_use]
    pub fn sub(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        // neg flips the sign bit, even on NaN. The dispatcher below
        // raises INVALID for any signaling-NaN input, so the bit-flip
        // is safe; the sNaN marker (bit 25) is independent of the
        // sign bit (bit 31).
        add_inner(self, other.neg(), rm)
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the alignment / rounding pipeline.
    ///
    /// Mirrors the decimal128 convention (ADR-0016). Symbolic proofs of
    /// the NaN / Inf / Zero behaviour skip the finite-finite alignment
    /// loops; production code uses [`Decimal32::add`].
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn add_special_only_for_kani(self, rhs: Self, rm: RoundingMode) -> Option<(Self, Status)> {
        handle_specials(classify_bits(self.0), classify_bits(rhs.0), rm)
    }

    /// Kani-only entry point for `sub`'s special path. Equivalent to
    /// `add_special_only_for_kani(self, rhs.neg(), rm)`; the negation
    /// happens before the dispatcher so sNaN propagation behaves
    /// identically to [`Decimal32::sub`].
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sub_special_only_for_kani(self, rhs: Self, rm: RoundingMode) -> Option<(Self, Status)> {
        let negated = rhs.neg();
        handle_specials(classify_bits(self.0), classify_bits(negated.0), rm)
    }
}

fn add_inner(a: Decimal32, b: Decimal32, rm: RoundingMode) -> (Decimal32, Status) {
    let ca = classify_bits(a.0);
    let cb = classify_bits(b.0);

    // Special-case dispatcher.
    if let Some(out) = handle_specials(ca, cb, rm) {
        return out;
    }

    // Finite + finite: extract (sign, biased_exp, coefficient) for both.
    let (sign_a, biased_a, coef_a) = match ca {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, u64::from(coefficient)),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite already handled by dispatcher"),
    };
    let (sign_b, biased_b, coef_b) = match cb {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, u64::from(coefficient)),
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        _ => unreachable!("non-finite already handled by dispatcher"),
    };

    let exp_a = biased_a as i32 - BIAS as i32;
    let exp_b = biased_b as i32 - BIAS as i32;

    // Both coefficients zero → IEEE 754-2019 §6.3 sign rule.
    if coef_a == 0 && coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        // Both `exp_a` and `exp_b` came from `classify_bits`, so
        // `q_preferred ∈ [-BIAS, BIASED_EXP_MAX - BIAS as i32]` and the
        // unbiased-to-biased conversion is in range.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_preferred)
            .expect("q_preferred from classify_bits-derived exponents");
        return (
            Decimal32::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                crate::bid::Coefficient::ZERO,
            )),
            Status::OK,
        );
    }

    // H1 fix: when exactly one operand is zero, the result is the
    // other operand requantised to `q_preferred = min(exp_a, exp_b)`
    // per IEEE 754-2019 §5.4.1 (`x + 0` is the correctly-rounded exact
    // sum) and §6.3 (preferred quantum for additive operations).
    // Without this short-circuit, an exponent gap wide enough to push
    // the zero below the aligned window let a `±0` operand still be
    // selected as the dominant side (the prior static-window code
    // returned `coef_hi` with `coef_lo` discarded, and `coef_hi` could
    // be the zero), discarding the real magnitude. A `±0` operand must
    // never dominate; the non-zero operand's sign and value win, and
    // the §6.3 sign-of-zero rules are still carried by the
    // both-zero / exact-cancellation branches.
    if coef_a == 0 {
        let q_preferred = exp_a.min(exp_b);
        return round_and_pack_finite(coef_b, exp_b, q_preferred, sign_b, false, rm, Status::OK);
    }
    if coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        return round_and_pack_finite(coef_a, exp_a, q_preferred, sign_a, false, rm, Status::OK);
    }

    // Order so that exp_hi >= exp_lo. (If equal, ordering is irrelevant.)
    let (sign_hi, exp_hi, coef_hi, sign_lo, exp_lo, coef_lo) = if exp_a >= exp_b {
        (sign_a, exp_a, coef_a, sign_b, exp_b, coef_b)
    } else {
        (sign_b, exp_b, coef_b, sign_a, exp_a, coef_a)
    };

    let diff = (exp_hi - exp_lo) as u32;

    // Dynamic alignment over `u128`. `coef_hi` and `coef_lo` are both
    // non-zero here (the zero cases short-circuit above), so
    // `hi_digits ∈ [1, 7]` and `max_shift ∈ [31, 37]`. Shift `coef_hi`
    // left by `s = min(diff, max_shift)` so the common quantum is as
    // low as `u128` allows; the lower operand is truncated only by the
    // unavoidable remainder `diff − s`, never more. When `s == diff`
    // the alignment is exact (`pre_sticky = false`), so the
    // round-half-even decision at the precision boundary sees the true
    // residue rather than a prematurely collapsed sticky bit.
    let hi_digits = decimal_digit_count_u128(u128::from(coef_hi));
    let max_shift = U128_DIGIT_CAP - hi_digits;
    let s = diff.min(max_shift);
    let aligned_hi = u128::from(coef_hi) * POW10_U128[s as usize];
    let align_exp = exp_hi - s as i32;
    let (aligned_lo, pre_sticky): (u128, bool) = if s == diff {
        // Exact: both operands now share `align_exp == exp_lo`.
        (u128::from(coef_lo), false)
    } else {
        let trim = diff - s;
        if (trim as usize) < POW10_U128.len() {
            let factor = POW10_U128[trim as usize];
            (
                u128::from(coef_lo) / factor,
                (u128::from(coef_lo) % factor) != 0,
            )
        } else {
            // `coef_lo` sits entirely below the retained window; it
            // contributes only its non-zeroness as sticky.
            (0u128, coef_lo != 0)
        }
    };

    let (combined_coef, combined_sign, h2_borrow) = if sign_hi == sign_lo {
        (aligned_hi + aligned_lo, sign_hi, false)
    } else if aligned_hi > aligned_lo {
        // Effective subtract, hi dominates. `lo`'s truncated residue
        // subtracts from the result magnitude (handled by the
        // borrow-and-extend below).
        (aligned_hi - aligned_lo, sign_hi, pre_sticky)
    } else if aligned_lo > aligned_hi {
        // Symmetric case: lo dominates AND carries the residue, so the
        // residue is additive (`combined_coef + ε_lo` is the true
        // magnitude). The funnel's `pre_sticky = true` encoding is
        // already correct here; no borrow.
        (aligned_lo - aligned_hi, sign_lo, false)
    } else {
        // Exact cancellation in the aligned magnitudes. If pre_sticky
        // is set, the true result is non-zero with `sign_lo` (the
        // truncated tail of `coef_lo` is positive) but its magnitude
        // is strictly below 1 ULP at the alignment quantum; defer to
        // the rounding funnel with a 1-coefficient at the truncation
        // quantum so directed modes round it correctly.
        let q_preferred = exp_a.min(exp_b);
        if pre_sticky {
            return round_and_pack_finite(
                1,
                exp_lo, // the truncation residue lives at exp_lo
                q_preferred,
                sign_lo,
                false,
                rm,
                Status::OK,
            );
        }
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        // As in the both-zero early return above, `q_preferred` is
        // bounded by the classify_bits-derived exponent range.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_preferred)
            .expect("q_preferred from classify_bits-derived exponents");
        return (
            Decimal32::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                crate::bid::Coefficient::ZERO,
            )),
            Status::OK,
        );
    };

    // H2 borrow-and-extend: when the hi-magnitude operand dominates an
    // effective subtraction AND lo had a truncated sub-ULP residue,
    // the result's true value sits BELOW `combined_coef × 10^align_exp`
    // by some ε ∈ (0, 1) ULP at `align_exp`, not above. The funnel's
    // `pre_sticky = true` convention encodes residue-above-LSB; under
    // directed modes (TowardZero, TowardPositive, TowardNegative) and
    // at exact half-ULP ties under round-half-even, the wrong sign on
    // the residue picks the wrong neighbour by one ULP. Borrow one ULP
    // from `combined_coef` and extend the bottom digits to a
    // `PRECISION`-digit cohort, turning the encoding into a
    // correctly-signed positive sticky at a lower quantum.
    //
    // For `combined_coef ≥ PRECISION` digits a plain `-1` suffices.
    // For fewer digits choose `k` so `combined_coef × 10^k − 1` has
    // exactly `PRECISION` digits (one extra `k` when `combined_coef`
    // is a power of 10, where the borrow drops the leading digit).
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
    round_and_pack_into_u32(
        combined_coef,
        align_exp,
        q_preferred,
        combined_sign,
        pre_sticky,
        rm,
    )
}

/// Compress a `u128` coefficient down to `u64` (with sticky tracking)
/// and route through `round_and_pack_finite`. Decimal32 rounds at
/// PRECISION (= 7) digits, so 14 retained digits in the `u64`
/// preserve the rounding decision. Mirrors the in-crate `fma.rs`
/// helper of the same shape.
pub(crate) fn round_and_pack_into_u32(
    coef_u128: u128,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    mut pre_sticky: bool,
    rm: RoundingMode,
) -> (Decimal32, Status) {
    const KEEP: u32 = 14; // PRECISION + 7 guard digits
    let keep_threshold = 10u128.pow(KEEP);

    if coef_u128 < keep_threshold {
        // Already within `u64` range and ≤ 14 digits: pass through.
        // `10^14 < u64::MAX`, so the cast is sound.
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
    // `c < keep_threshold = 10^14 ≤ u64::MAX` holds by the loop exit
    // condition, so the `c as u64` cast is sound.
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

/// IEEE 754-2019 §6.3 sign rule for `x + (−x)` and `(±0) + (±0)`:
/// the result is `+0` in all rounding modes except
/// `roundTowardNegative`, which yields `−0`.
#[inline]
fn zero_sum_sign(sign_a: bool, sign_b: bool, rm: RoundingMode) -> bool {
    if sign_a == sign_b {
        // Both zeros (or cancellation) of the same sign retain that sign.
        return sign_a;
    }
    matches!(rm, RoundingMode::TowardNegative)
}

/// Special-case dispatcher: NaN propagation, Infinity arithmetic,
/// pure-zero reductions. Returns `Some` when the case is fully
/// handled here; `None` falls through to the finite path.
fn handle_specials(a: Class, b: Class, rm: RoundingMode) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    // Signaling NaN in either operand: result is the quieted NaN with
    // the propagated sign / payload, INVALID raised. Per IEEE 754
    // §6.2.3, a is preferred when both are sNaN.
    match (a, b) {
        (SignalingNaN { sign, payload }, _) | (_, SignalingNaN { sign, payload }) => {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
        _ => {}
    }

    // Quiet NaN in either operand: propagate (a preferred) without
    // raising flags.
    match (a, b) {
        (QuietNaN { sign, payload }, _) => {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
        (_, QuietNaN { sign, payload }) => {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
        _ => {}
    }

    // Infinity arithmetic.
    match (a, b) {
        (Infinity { sign: sa }, Infinity { sign: sb }) => {
            if sa == sb {
                Some((
                    Decimal32::from_bits(crate::bid::pack_infinity(sa)),
                    Status::OK,
                ))
            } else {
                // +∞ + (−∞) → NaN, INVALID.
                Some((Decimal32::NAN, Status::INVALID))
            }
        }
        (Infinity { sign }, _) => Some((
            Decimal32::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        )),
        (_, Infinity { sign }) => Some((
            Decimal32::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        )),
        // No infinities and no NaNs: at most one operand is Zero; the
        // other is Finite (or also Zero). Both branches are handled by
        // the finite path; only the all-zero case picks up the §6.3
        // sign rule, which we encode in the finite path's
        // zero-coefficient branch using zero_sum_sign.
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

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
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
        // 9_999_999 + 1 = 10_000_000 → renormalises to 1_000_000 × 10^1.
        let (r, _) = from_int(9_999_999, 0).add(from_int(1, 0), RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS + 1).unwrap(),
            Coefficient::try_new(1_000_000).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn add_signs_differ_cancellation() {
        // 1 + (-1) → +0 under NearestEven, -0 under TowardNegative.
        let (r, _) = from_int(1, 0).add(from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = from_int(1, 0).add(from_int(-1, 0), RoundingMode::TowardNegative);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn add_zero_plus_zero() {
        let (r, _) = Decimal32::ZERO.add(Decimal32::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ZERO.to_bits());

        let (r, _) = Decimal32::NEG_ZERO.add(Decimal32::NEG_ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::NEG_ZERO.to_bits());

        // (+0) + (-0) → +0 in NearestEven, -0 in TowardNegative.
        let (r, _) = Decimal32::ZERO.add(Decimal32::NEG_ZERO, RoundingMode::NearestEven);
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal32::ZERO.add(Decimal32::NEG_ZERO, RoundingMode::TowardNegative);
        assert!(r.is_sign_negative());
    }

    #[test]
    fn add_with_alignment() {
        // 1 + 0.5 = 1.5
        let a = from_int(1, 0);
        let b = from_int(5, -1);
        let (r, _) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 1).unwrap(),
            Coefficient::try_new(15).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());

        // 1.0 + 0.005 = 1.005
        let a = from_int(10, -1);
        let b = from_int(5, -3);
        let (r, _) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 3).unwrap(),
            Coefficient::try_new(1005).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn add_with_far_alignment_inexact() {
        // 1 + 1e-10: 1e-10 sits well below the working window.
        // Result: 1 (with sticky → INEXACT).
        let a = from_int(1, 0);
        let b = from_int(1, -10);
        let (r, s) = a.add(b, RoundingMode::NearestEven);
        // The result should round to 1.000000 (preserving the
        // alignment quantum within working precision; cohort decisions
        // are handled by round_and_pack's preferred-quantum logic).
        assert!(r.is_finite() && !r.is_sign_negative());
        assert!(s.inexact());
    }

    #[test]
    fn sub_basic() {
        let (r, _) = from_int(5, 0).sub(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());

        // 1 - 1 = +0
        let (r, _) = from_int(1, 0).sub(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // 0 - x = -x
        let (r, _) = Decimal32::ZERO.sub(from_int(5, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-5, 0).to_bits());
    }

    #[test]
    fn nan_propagation() {
        let (r, s) = Decimal32::NAN.add(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::ONE.add(Decimal32::NAN, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.add(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::ONE.sub(Decimal32::SIGNALING_NAN, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn infinity_arithmetic() {
        // +∞ + 1 = +∞
        let (r, s) = Decimal32::INFINITY.add(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        // +∞ + (−∞) = NaN, INVALID
        let (r, s) = Decimal32::INFINITY.add(Decimal32::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        // +∞ + (+∞) = +∞
        let (r, s) = Decimal32::INFINITY.add(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        // +∞ - +∞ = NaN, INVALID
        let (r, s) = Decimal32::INFINITY.sub(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn add_overflow_to_infinity() {
        // MAX + MAX → overflow, +∞ under NearestEven.
        let (r, s) = Decimal32::MAX.add(Decimal32::MAX, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn add_finite_zero_returns_finite() {
        let (r, _) = from_int(123, -2).add(Decimal32::ZERO, RoundingMode::NearestEven);
        // Cohort: `123 × 10^-2 + 0E+0` should preserve the quantum of
        // the smaller (more negative) exponent: -2.
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 2).unwrap(),
            Coefficient::try_new(123).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
