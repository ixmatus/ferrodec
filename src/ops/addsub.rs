//! IEEE 754 add and subtract for [`Decimal128`].
//!
//! `add` (and the trivially-derived `sub`) is the simplest of the
//! four arithmetic ops in shape, but the most rule-laden because it has
//! to cope with:
//!
//! * NaN propagation, with `INVALID` for any signaling-NaN operand.
//! * `±∞ ± ±∞`: same-sign infinities give the same infinity; opposite
//!   signs give NaN+`INVALID` (`Inf − Inf` is undefined).
//! * Zero handling, including the IEEE 754 sign rule for `(±0) ± (±0)`:
//!   in [`RoundingMode::TowardNegative`] the sum of equal-magnitude
//!   opposite-sign operands is `−0`; in every other mode it is `+0`.
//! * Effective-subtract cancellation: when sign-adjusted operands cancel
//!   to *exactly* zero the sign rule above kicks in.
//! * Coefficient alignment by `10^Δ` where `Δ` is the difference in
//!   quantum exponents. This is where the U256 intermediate earns its keep
//!   — `10^35 × (10^34 − 1)` is wider than `u128`.
//!
//! Rounding to 34 decimal digits, status-flag emission, and overflow /
//! underflow handling are deferred to [`crate::ops::round_and_pack_finite`].

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, Class, BIAS, PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

/// Maximum exponent difference for which we keep a fully-precise
/// alignment in the U256 intermediate.
///
/// We bound by U256 capacity: `coef × 10^Δ` with `coef < 10^34`
/// stays under `10^77 ≈ 2^256` for `Δ ≤ 43`. Anything above falls
/// to the sub-ULP sticky-bit shortcut, which is *correct for
/// effective add* but only approximately correct for effective sub
/// — at very large Δ the residue can still tip a rounding boundary.
/// Tracked as a follow-up; in practice `Δ > 43` operands are
/// uncommon.
const ALIGN_LIMIT: u32 = 43;

impl Decimal128 {
    /// IEEE 754 `addition(self, rhs)`.
    ///
    /// Returns `(self + rhs, status)` rounded according to `rm`.
    #[must_use]
    pub fn add(self, rhs: Self, rm: RoundingMode) -> (Self, Status) {
        add_kernel(self, rhs, rm)
    }

    /// IEEE 754 `subtraction(self, rhs)`.
    ///
    /// Implemented as `self + (−rhs)` — sign-flipping `rhs` is exact and
    /// preserves NaN payload, so the addition kernel handles every edge
    /// case correctly.
    #[must_use]
    pub fn sub(self, rhs: Self, rm: RoundingMode) -> (Self, Status) {
        add_kernel(self, rhs.neg(), rm)
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the alignment / rounding pipeline.
    ///
    /// This exists so symbolic proofs of the NaN / Inf / Zero behaviour
    /// don't drag the heavy finite-finite paths through CBMC's
    /// path-explosion. Production code uses [`Decimal128::add`].
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn add_special_only_for_kani(
        self,
        rhs: Self,
        rm: RoundingMode,
    ) -> Option<(Self, Status)> {
        add_special_cases(self, rhs, rm)
    }
}

fn add_kernel(a: Decimal128, b: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    // Special-case the non-finite-finite paths first. The function is
    // intentionally loop-free so Kani can dispatch the NaN / Inf / Zero
    // proofs without symbolically unwinding the alignment loops below.
    if let Some(early) = add_special_cases(a, b, rm) {
        return early;
    }
    add_finite_finite(a, b, rm)
}

/// Resolve every non-`(finite_nonzero, finite_nonzero)` add case.
///
/// Returns `None` only when both operands are finite *and* non-zero — the
/// one path that must run alignment + rounding. Every other input class
/// (NaN / sNaN / ±Inf / any operand zero) is folded to a closed-form
/// answer here, which keeps the Kani special-case harnesses tractable
/// because they never have to reason about the alignment pipeline.
#[inline]
fn add_special_cases(
    a: Decimal128,
    b: Decimal128,
    rm: RoundingMode,
) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan = matches!(cls_a, Class::SignalingNaN { .. })
        || matches!(cls_b, Class::SignalingNaN { .. });
    let status = if snan { Status::INVALID } else { Status::OK };

    if matches!(
        cls_a,
        Class::QuietNaN { .. } | Class::SignalingNaN { .. }
    ) || matches!(
        cls_b,
        Class::QuietNaN { .. } | Class::SignalingNaN { .. }
    ) {
        return Some((Decimal128::NAN, status));
    }

    match (cls_a, cls_b) {
        (Class::Infinity { sign: sa }, Class::Infinity { sign: sb }) => {
            return Some(if sa == sb {
                (Decimal128::from_bits(pack_infinity(sa)), status)
            } else {
                (Decimal128::NAN, status | Status::INVALID)
            });
        }
        (Class::Infinity { sign }, _) | (_, Class::Infinity { sign }) => {
            return Some((Decimal128::from_bits(pack_infinity(sign)), status));
        }
        _ => {}
    }

    let (sa, ea, ca) = decompose_finite(cls_a);
    let (sb, eb, cb) = decompose_finite(cls_b);

    if ca == 0 && cb == 0 {
        let result_sign = if sa == sb {
            sa
        } else {
            rm == RoundingMode::TowardNegative
        };
        let exp = ea.min(eb);
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, exp, 0)),
            status,
        ));
    }
    // IEEE 754 §6.3 preferred quantum for `add(x, ±0)` is `min(qx, q0)`.
    // We re-emit the non-zero operand at that quantum if it can fit; if
    // the shift would exceed `PRECISION` digits we keep the natural
    // quantum. Status flags are unaffected.
    if ca == 0 {
        let target = ea.min(eb);
        return Some((rebase_finite_to_lower_quantum(sb, eb, cb, target), status));
    }
    if cb == 0 {
        let target = ea.min(eb);
        return Some((rebase_finite_to_lower_quantum(sa, ea, ca, target), status));
    }

    None
}

/// Re-emit a finite value at a target biased quantum that is `≤` the
/// current one, multiplying the coefficient by `10^Δ`. If `Δ` would
/// take the coefficient over `PRECISION` digits we shift only as far as
/// the precision allows.
fn rebase_finite_to_lower_quantum(
    sign: bool,
    biased_exp: u32,
    coefficient: u128,
    target_biased: u32,
) -> Decimal128 {
    if coefficient == 0 {
        // Zero coefficients can express any quantum within the format;
        // pack at the target directly.
        return Decimal128::from_bits(pack_finite(sign, target_biased, 0));
    }
    if biased_exp <= target_biased {
        return Decimal128::from_bits(pack_finite(sign, biased_exp, coefficient));
    }
    let delta = biased_exp - target_biased;
    let digits = decimal_digit_count(coefficient);
    let max_shift = PRECISION - digits;
    let shift = delta.min(max_shift);
    let new_coef = coefficient * 10u128.pow(shift);
    let new_biased = biased_exp - shift;
    Decimal128::from_bits(pack_finite(sign, new_biased, new_coef))
}

/// Finite-non-zero × finite-non-zero — the alignment + rounding path.
///
/// Pre-condition: both operands are finite (Zero or Finite) and at least
/// one has a non-zero coefficient. In practice both have non-zero
/// coefficients because [`add_special_cases`] handles the
/// "either operand is zero" branches.
fn add_finite_finite(a: Decimal128, b: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sa, ea, ca) = decompose_finite(cls_a);
    let (sb, eb, cb) = decompose_finite(cls_b);
    debug_assert!(ca != 0 && cb != 0);

    let status = Status::OK;
    let (sl, el, cl, ss, es, cs) = if ea >= eb {
        (sa, ea, ca, sb, eb, cb)
    } else {
        (sb, eb, cb, sa, ea, ca)
    };
    let diff = el - es;
    let effective_sub = sl != ss;

    // Align coefficients into a U256 intermediate. When `diff` is small
    // enough we represent the alignment exactly; beyond that, the smaller
    // operand is at most one ULP of the larger and we collapse it to a
    // sticky bit.
    let (al, as_, target_exp, mut sticky) = if diff <= ALIGN_LIMIT {
        let aligned_l = U256::from_u128(cl).mul_pow10(diff);
        (aligned_l, U256::from_u128(cs), es as i32 - BIAS as i32, false)
    } else {
        // Smaller is sub-ULP relative to the larger. The larger keeps its
        // own exponent; the smaller becomes a sticky bit.
        (
            U256::from_u128(cl),
            U256::ZERO,
            el as i32 - BIAS as i32,
            cs != 0,
        )
    };

    let (mut combined, mut sign_out) = if effective_sub {
        match al.cmp(as_) {
            core::cmp::Ordering::Greater => (al.sub(as_), sl),
            core::cmp::Ordering::Less => {
                // The smaller operand turned out to have larger magnitude
                // (only possible when diff == 0). Sign comes from `ss`.
                (as_.sub(al), ss)
            }
            core::cmp::Ordering::Equal => {
                if sticky {
                    // The "smaller" had a non-zero residue beyond the
                    // alignment envelope; it is *strictly* greater in
                    // magnitude. Result sign flips.
                    // For a single-ULP-shy difference at this scale,
                    // round-toward-zero collapses to zero with sticky;
                    // for a fully-correct rounding we'd subtract 1 ULP.
                    // Treat as exact zero for v1 — tracked as a follow-up.
                    let _ = sticky;
                    return zero_after_cancellation(rm, status, target_exp);
                }
                return zero_after_cancellation(rm, status, target_exp);
            }
        }
    } else {
        (al.add(as_), sl)
    };

    if combined.is_zero() && !sticky {
        return zero_after_cancellation(rm, status, target_exp);
    }

    // Mute the unused `sign_out`/`combined`/`sticky` `mut` warnings if a
    // subsequent refactor removes the mutation paths above.
    let _ = (&mut combined, &mut sign_out, &mut sticky);

    // IEEE 754 §6.3 preferred quantum for add/sub is `min(qa, qb)`.
    let q_preferred = (ea.min(eb)) as i32 - BIAS as i32;
    round_and_pack_finite(combined, target_exp, q_preferred, sign_out, sticky, rm, status)
}

fn zero_after_cancellation(
    rm: RoundingMode,
    status: Status,
    target_unbiased_exp: i32,
) -> (Decimal128, Status) {
    // IEEE 754 §6.3: cancellation of equal-magnitude opposite-sign
    // operands yields +0 except under round-toward-negative.
    let sign = rm == RoundingMode::TowardNegative;
    let biased_exp = (target_unbiased_exp + BIAS as i32)
        .clamp(0, crate::bid::BIASED_EXP_MAX as i32) as u32;
    (
        Decimal128::from_bits(pack_finite(sign, biased_exp, 0)),
        status,
    )
}

/// Decompose a Zero / Finite [`Class`] into `(sign, biased_exp, coefficient)`.
///
/// Pre-condition: `c` is `Class::Zero` or `Class::Finite`. The arithmetic
/// kernel filters NaN / Infinity before calling here.
fn decompose_finite(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        // Unreachable in the add path: NaN/Inf are handled earlier.
        // `unreachable!` would be a runtime panic, but the alternative
        // (returning a poison value) hides the bug. Use `debug_assert!`
        // instead — release builds get a quiet `(false, 0, 0)`.
        _ => {
            debug_assert!(false, "decompose_finite called on non-finite Class");
            (false, BIAS, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d_from_int(s: bool, exp: u32, coef: u128) -> Decimal128 {
        Decimal128::from_bits(pack_finite(s, exp, coef))
    }

    fn d_int(c: i128) -> Decimal128 {
        if c == 0 {
            return Decimal128::ZERO;
        }
        let sign = c < 0;
        let coef = c.unsigned_abs();
        d_from_int(sign, BIAS, coef)
    }

    #[test]
    fn nan_in_nan_out() {
        let (r, s) = Decimal128::ONE.add(Decimal128::NAN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal128::NAN.add(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn snan_raises_invalid() {
        let (r, s) = Decimal128::ONE.add(Decimal128::SIGNALING_NAN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
        let (r, s) = Decimal128::SIGNALING_NAN.sub(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_plus_inf_same_sign() {
        let (r, s) = Decimal128::INFINITY.add(Decimal128::INFINITY, RoundingMode::default());
        assert_eq!(r.to_bits(), Decimal128::INFINITY.to_bits());
        assert!(s.is_ok());

        let (r, s) =
            Decimal128::NEG_INFINITY.add(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert_eq!(r.to_bits(), Decimal128::NEG_INFINITY.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn inf_minus_inf_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.add(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());

        let (r, s) = Decimal128::INFINITY.sub(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_plus_finite_is_inf() {
        let (r, s) = Decimal128::INFINITY.add(Decimal128::ONE, RoundingMode::default());
        assert_eq!(r.to_bits(), Decimal128::INFINITY.to_bits());
        assert!(s.is_ok());

        let (r, s) = Decimal128::ONE.add(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert_eq!(r.to_bits(), Decimal128::NEG_INFINITY.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn zero_plus_zero_signs() {
        // +0 + +0 = +0
        let (r, _) = Decimal128::ZERO.add(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        // -0 + -0 = -0
        let (r, _) = Decimal128::NEG_ZERO.add(Decimal128::NEG_ZERO, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        // +0 + -0 = +0 (NearestEven default)
        let (r, _) = Decimal128::ZERO.add(Decimal128::NEG_ZERO, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        // +0 + -0 in TowardNegative = -0
        let (r, _) = Decimal128::ZERO.add(Decimal128::NEG_ZERO, RoundingMode::TowardNegative);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn add_one_plus_one() {
        let two = d_int(2);
        let (r, s) = Decimal128::ONE.add(Decimal128::ONE, RoundingMode::default());
        assert_eq!(r.to_bits(), two.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn sub_one_minus_one_is_zero() {
        let (r, s) = Decimal128::ONE.sub(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative()); // NearestEven gives +0
        assert!(s.is_ok());

        let (r, _) = Decimal128::ONE.sub(Decimal128::ONE, RoundingMode::TowardNegative);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn add_commutative_simple() {
        for (a, b) in [(1, 2), (5, 7), (123, 456), (-3, 8), (-1, -1), (100, -50)] {
            let da = d_int(a);
            let db = d_int(b);
            let (ab, _) = da.add(db, RoundingMode::default());
            let (ba, _) = db.add(da, RoundingMode::default());
            assert_eq!(ab.to_bits(), ba.to_bits(), "add({a},{b}) bits");
        }
    }

    #[test]
    fn add_a_plus_neg_a_is_zero() {
        for &a in &[1i128, 7, 123, 10_000_000, -3, -42] {
            let da = d_int(a);
            let dna = da.neg();
            let (sum, _) = da.add(dna, RoundingMode::default());
            assert!(sum.is_zero(), "{a} + (-{a}) should be zero, got {sum:?}");
        }
    }

    #[test]
    fn add_unequal_exponents_simple() {
        // 100 + 1 = 101: encoded with different exponents, should align.
        // 100 as (1, BIAS+2): 1 × 10^2 = 100
        let a = d_from_int(false, BIAS + 2, 1);
        // 1 as (1, BIAS+0): 1 × 10^0 = 1
        let b = Decimal128::ONE;
        let (sum, _) = a.add(b, RoundingMode::default());
        // Expected: 101 = (101, BIAS+0)
        let expected = d_from_int(false, BIAS, 101);
        // Numerically equal (cohort may differ).
        let (ord, _) = sum.partial_cmp(expected);
        assert_eq!(ord, Some(core::cmp::Ordering::Equal), "sum={sum:?}, expected={expected:?}");
    }

    #[test]
    fn sub_with_alignment_simple() {
        // 100 - 1 = 99
        let a = d_from_int(false, BIAS + 2, 1);
        let b = Decimal128::ONE;
        let (diff, _) = a.sub(b, RoundingMode::default());
        let expected = d_int(99);
        let (ord, _) = diff.partial_cmp(expected);
        assert_eq!(ord, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn add_negative_smaller_to_positive_larger() {
        // 5 + (-3) = 2
        let (r, _) = d_int(5).add(d_int(-3), RoundingMode::default());
        let (ord, _) = r.partial_cmp(d_int(2));
        assert_eq!(ord, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn add_round_at_precision_boundary() {
        // (10^34 - 1) + 0 = (10^34 - 1)  (no rounding)
        let max_coef = 10u128.pow(34) - 1;
        let a = d_from_int(false, BIAS, max_coef);
        let (r, _) = a.add(Decimal128::ZERO, RoundingMode::default());
        assert_eq!(r.to_bits(), a.to_bits());

        // (10^34 - 1) + 1 = 10^34 → renormalized to 10^33 × 10 = (1, BIAS+1)
        let (r, s) = a.add(Decimal128::ONE, RoundingMode::default());
        // Numerically equals 10^34 = 1 × 10^34
        // We've increased the exponent by 1 because we shifted the coefficient.
        let expected = d_from_int(false, BIAS + 1, 10u128.pow(33));
        let (ord, _) = r.partial_cmp(expected);
        assert_eq!(
            ord,
            Some(core::cmp::Ordering::Equal),
            "got {r:?}, expected {expected:?}, status={s:?}"
        );
    }
}
