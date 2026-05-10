//! IEEE 754-2019 add and subtract for [`Decimal32`].
//!
//! Both ops return `(Decimal32, Status)`. Subtract is implemented as
//! `add(a, -b)`, with the sign-flip composed *before* the special-case
//! dispatcher so signaling NaN propagation is unaffected.
//!
//! # Algorithm
//!
//! 1. Special-case dispatcher (NaN, Infinity, Zero).
//! 2. Finite path: align coefficients over a `u64` working width,
//!    sign-aware combine, route through
//!    [`round_and_pack_finite`](super::round::round_and_pack_finite).
//!
//! # Working precision
//!
//! Decimal32 coefficients fit in 24 bits. The maximum `coef_hi *
//! 10^diff` we can shift without overflowing `u64` (max ≈ 1.84 × 10¹⁹)
//! is `coef_hi × 10^12 ≈ 9.999 × 10¹⁸`. So:
//!
//! * `diff ≤ 12`: shift the higher-quantum operand left by 10^diff;
//!   keep the lower-quantum operand as-is.
//! * `diff > 12` and `diff ≤ 14`: align at `exp_lo + (diff - 12)`,
//!   truncating the lower-quantum operand by `10^(diff − 12)` and
//!   feeding the residue into the sticky bit.
//! * `diff > 14`: the lower-quantum operand sits below the working
//!   window entirely. Use the higher-quantum operand as the
//!   coefficient and feed `(coef_lo != 0)` into sticky.
//!
//! 14 is the working-width precision (PRECISION + extra digits for
//! correct rounding); beyond that the lower operand contributes only
//! to the sticky bit, never to a kept digit.

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

use super::round::round_and_pack_finite;

const POW10_U64: [u64; 15] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
];

/// Maximum `diff` for which we can shift `coef_hi` left in place
/// without exceeding `u64`. `coef_hi < 10^7`, so `coef_hi × 10^12 <
/// 10^19 < 2^64`.
const ALIGN_LIMIT: u32 = 12;

/// Working-width precision for addition: we keep up to PRECISION + 7
/// trailing digits past `coef_hi`'s last digit before the lower
/// operand becomes sticky-only. 14 gives a generous guard band over
/// the 7-digit precision; beyond that, the lower operand sits below
/// the last kept digit and only contributes via sticky.
const WORKING_PRECISION: u32 = 14;

// Compile-time invariants: POW10_U64 must hold every reachable
// index. ALIGN_LIMIT = 12 needs entry 12; WORKING_PRECISION = 14
// is exclusive (the hot path uses the index `WORKING_PRECISION −
// 1 = 13`).
const _: () = assert!(POW10_U64.len() > ALIGN_LIMIT as usize);
const _: () = assert!(POW10_U64.len() > WORKING_PRECISION as usize - 1);

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

    // Both coefficients zero → IEEE 754 §6.3 sign rule.
    if coef_a == 0 && coef_b == 0 {
        let q_preferred = exp_a.min(exp_b);
        let result_sign = zero_sum_sign(sign_a, sign_b, rm);
        return (
            Decimal32::from_bits(crate::bid::pack_finite(
                result_sign,
                (q_preferred + BIAS as i32) as u32,
                0,
            )),
            Status::OK,
        );
    }

    // Order so that exp_hi >= exp_lo. (If equal, ordering is irrelevant.)
    let (sign_hi, exp_hi, coef_hi, sign_lo, exp_lo, coef_lo) = if exp_a >= exp_b {
        (sign_a, exp_a, coef_a, sign_b, exp_b, coef_b)
    } else {
        (sign_b, exp_b, coef_b, sign_a, exp_a, coef_a)
    };

    let diff = (exp_hi - exp_lo) as u32;

    // Align coefficients to a common exponent, capping by the
    // working-width to avoid u64 overflow. After this block:
    // * `aligned_hi` and `aligned_lo` are at exponent `align_exp`.
    // * `pre_sticky` carries any digits the alignment had to drop.
    let (aligned_hi, aligned_lo, align_exp, pre_sticky): (u64, u64, i32, bool) =
        if diff <= ALIGN_LIMIT {
            // Shift higher operand left by 10^diff; lower stays put.
            let shifted = coef_hi * POW10_U64[diff as usize];
            (shifted, coef_lo, exp_lo, false)
        } else if diff <= WORKING_PRECISION {
            // Truncate lower operand by 10^(diff - ALIGN_LIMIT); shift
            // higher operand left by 10^ALIGN_LIMIT. Both end at
            // exponent exp_lo + (diff - ALIGN_LIMIT) = exp_hi - ALIGN_LIMIT.
            let trim = diff - ALIGN_LIMIT;
            let factor = POW10_U64[trim as usize];
            let trunc_lo = coef_lo / factor;
            let pre_sticky = (coef_lo % factor) != 0;
            let shifted_hi = coef_hi * POW10_U64[ALIGN_LIMIT as usize];
            (
                shifted_hi,
                trunc_lo,
                exp_hi - ALIGN_LIMIT as i32,
                pre_sticky,
            )
        } else {
            // Lower operand is below the working window entirely.
            (coef_hi, 0, exp_hi, coef_lo != 0)
        };

    // Sign-aware combine.
    let (combined_coef, combined_sign) = if sign_hi == sign_lo {
        (aligned_hi + aligned_lo, sign_hi)
    } else if aligned_hi > aligned_lo {
        (aligned_hi - aligned_lo, sign_hi)
    } else if aligned_lo > aligned_hi {
        (aligned_lo - aligned_hi, sign_lo)
    } else {
        // Exact cancellation in the aligned magnitudes. If pre_sticky
        // is set, the true result is non-zero with sign_lo (since the
        // truncated tail of coef_lo is positive), but its magnitude is
        // strictly less than 1 ULP at the alignment quantum. In that
        // case the rounding step handles the sign correctly via
        // pre_sticky on a zero kept coefficient — but round_and_pack
        // sees coef = 0 with pre_sticky and routes to the zero path
        // which doesn't carry pre_sticky's sign. Handle here by
        // recovering the correct sign of the rounded magnitude
        // explicitly: a non-zero pre_sticky tail dominates, so the
        // result's sign is sign_lo. Round magnitude is below
        // representable; result is ±MIN_POSITIVE under away-from-zero
        // modes, ±0 otherwise. For NearestEven (the common case) the
        // tail rounds to 0.
        let q_preferred = exp_a.min(exp_b);
        if pre_sticky {
            // Tail rounds toward zero or away depending on rm; defer
            // to round_and_pack with sign_lo and a coef of 1 at the
            // truncation quantum.
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
        return (
            Decimal32::from_bits(crate::bid::pack_finite(
                result_sign,
                (q_preferred + BIAS as i32) as u32,
                0,
            )),
            Status::OK,
        );
    };

    let q_preferred = exp_a.min(exp_b);
    round_and_pack_finite(
        combined_coef,
        align_exp,
        q_preferred,
        combined_sign,
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
    use crate::bid::{pack_finite, BIAS};

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
        let expected = Decimal32::from_bits(pack_finite(false, BIAS + 1, 1_000_000));
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
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 1, 15));
        assert_eq!(r.to_bits(), expected.to_bits());

        // 1.0 + 0.005 = 1.005
        let a = from_int(10, -1);
        let b = from_int(5, -3);
        let (r, _) = a.add(b, RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 3, 1005));
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
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 2, 123));
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
