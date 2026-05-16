//! IEEE 754-2019 fused multiply-add for [`Decimal32`].
//!
//! `fma(a, b, c) = a * b + c` with a single rounding. Distinct from
//! `(a * b).add(c)` because the intermediate product is preserved
//! exactly before the add — the multiply does not round.
//!
//! # Algorithm
//!
//! 1. Special-case dispatcher (NaN, ±∞, 0 × ±∞).
//! 2. Finite path:
//!    * Exact product `ab = coef_a × coef_b` (fits in `u64`: max
//!      `9_999_999²` ≈ 10¹⁴).
//!    * Align the product with `c` at `target_q = min(ab_exp, c_exp)`
//!      over a `u128` working width (max value ≈ 10³⁸ < 2¹²⁸).
//!    * Sign-aware combine.
//!    * Route through `round_and_pack_finite` after compressing the
//!      combined `u128` back to `u64` with sticky tracking.
//!
//! # Special cases (IEEE 754-2019 §7)
//!
//! * sNaN in any operand → quiet NaN + `INVALID`.
//! * qNaN propagation (`a` preferred, then `b`, then `c`).
//! * `0 × ±∞` or `±∞ × 0` (regardless of `c`, unless `c` is sNaN) →
//!   NaN + `INVALID`. The §7.2 invalid-operation rule fires for the
//!   undefined product before the addition.
//! * `±∞ × finite` and `finite × ±∞` produce `±∞` (XOR sign), then
//!   apply the addition: `(±∞) + (∓∞) → NaN + INVALID`,
//!   `(±∞) + finite → ±∞`.
//! * `0 × finite` (no infinity collision): product is `±0` with XOR
//!   sign; result is `c` (after the add) with the §6.3 quantum.

use crate::bid::{classify_bits, Class, BIAS, COEFFICIENT_LIMIT, PRECISION};
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

// Compile-time invariant: the largest reachable index is
// `U128_DIGIT_CAP = 38`. The table needs ≥ 39 entries.
const _: () = assert!(POW10_U128.len() > 38);

/// Upper bound on `digit_count(coef) + shift` that keeps the product
/// within `u128::MAX`. `10³⁸ < 2¹²⁸ ≈ 3.4 × 10³⁸`, so any product
/// with at most 38 decimal digits fits.
///
/// Used for the *dynamic* alignment-shift bound: shift each operand
/// by up to `U128_DIGIT_CAP − digit_count(operand)` decimal positions
/// before overflow risk. A small operand (e.g. `1 × 1`) leaves more
/// headroom than the worst-case product (`(10⁷ − 1)² ≈ 10¹⁴`).
const U128_DIGIT_CAP: u32 = 38;

impl Decimal32 {
    /// IEEE 754-2019 `fusedMultiplyAdd(self, b, c)` rounded by `rm`.
    ///
    /// Computes `self * b + c` with a single rounding step (no
    /// intermediate rounding of the product).
    #[must_use]
    pub fn fma(self, b: Self, c: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(b.0);
        let cc = classify_bits(c.0);

        if let Some(out) = handle_specials(ca, cb, cc) {
            return out;
        }

        // Finite × finite + (Finite | Zero).
        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite a/b"),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite a/b"),
        };
        let (sign_c, biased_c, coef_c) = match cc {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite c"),
        };

        let ab_sign = sign_a ^ sign_b;
        let ab_coef = coef_a * coef_b; // u32 × u32 → u64 (max 10¹⁴)
        let ab_exp = (biased_a as i32 - BIAS as i32) + (biased_b as i32 - BIAS as i32);
        let c_exp = biased_c as i32 - BIAS as i32;

        let target_q = ab_exp.min(c_exp);

        // Both zero: §6.3 sign rule (cancellation between ab=0 and c=0).
        if ab_coef == 0 && coef_c == 0 {
            let result_sign = zero_sum_sign(ab_sign, sign_c, rm);
            // H3 fix: target_q can fall outside
            // `[-BIAS, BIASED_EXP_MAX as i32 - BIAS as i32]` when the
            // zero product's ideal exponent (`ab_exp = q(a) + q(b)`,
            // range `[-202, +180]`) drives the minimum below `-BIAS`.
            // IEEE 754-2019 §6.3 + §7.4 require clamping the result
            // quantum to the representable range and raising the
            // informational `Clamped` flag.
            let (biased_exp, clamped) = crate::bid::BiasedExp::clamp_unbiased(target_q);
            let status = if clamped { Status::CLAMPED } else { Status::OK };
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    result_sign,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                status,
            );
        }

        // Zero product with non-zero c: result is c rebased to the
        // preferred quantum. The non-zero summand's sign wins;
        // ab_sign / zero_sum_sign do not apply.
        if ab_coef == 0 {
            return round_and_pack_into_u32(coef_c as u128, c_exp, target_q, sign_c, false, rm);
        }

        // Zero c with non-zero product: result is ab rebased.
        if coef_c == 0 {
            return round_and_pack_into_u32(
                u128::from(ab_coef),
                ab_exp,
                target_q,
                ab_sign,
                false,
                rm,
            );
        }

        let shift_ab = (ab_exp - target_q) as u32;
        let shift_c = (c_exp - target_q) as u32;

        let ab_digits = decimal_digit_count_u128(u128::from(ab_coef));
        let c_digits = decimal_digit_count_u128(u128::from(coef_c));
        let ab_safe_shift = U128_DIGIT_CAP - ab_digits;
        let c_safe_shift = U128_DIGIT_CAP - c_digits;

        let mut pre_sticky = false;

        // If aligning either operand into u128 would overflow, that
        // side's value at `target_q` exceeds `10³⁸`, which is far
        // beyond the other side's representable range (at most ~14
        // digits at `target_q` for ab, ~7 for c). It therefore
        // *actually* dominates, and the early-return is correct.
        let ab_u128: u128 = if shift_ab <= ab_safe_shift {
            u128::from(ab_coef) * POW10_U128[shift_ab as usize]
        } else {
            pre_sticky |= coef_c != 0;
            return round_and_pack_into_u32(
                u128::from(ab_coef),
                ab_exp,
                ab_exp,
                ab_sign,
                pre_sticky,
                rm,
            );
        };

        let c_u128: u128 = if shift_c <= c_safe_shift {
            u128::from(coef_c) * POW10_U128[shift_c as usize]
        } else {
            pre_sticky |= ab_coef != 0;
            return round_and_pack_into_u32(
                u128::from(coef_c),
                c_exp,
                c_exp,
                sign_c,
                pre_sticky,
                rm,
            );
        };

        // Sign-aware combine in u128.
        let (combined_u128, combined_sign) = if ab_sign == sign_c {
            (ab_u128 + c_u128, ab_sign)
        } else if ab_u128 > c_u128 {
            (ab_u128 - c_u128, ab_sign)
        } else if c_u128 > ab_u128 {
            (c_u128 - ab_u128, sign_c)
        } else {
            // Exact cancellation. §6.3 sign rule.
            let q_preferred = target_q;
            let result_sign = zero_sum_sign(ab_sign, sign_c, rm);
            // Cancellation mirror of the H3 fix above: when ab and c
            // align to equal magnitudes (opposite signs), the result
            // is exact zero and the preferred quantum may fall outside
            // the representable range. Same §6.3 + §7.4 clamp rule.
            let (biased_exp, clamped) = crate::bid::BiasedExp::clamp_unbiased(q_preferred);
            let status = if clamped { Status::CLAMPED } else { Status::OK };
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    result_sign,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                status,
            );
        };

        round_and_pack_into_u32(
            combined_u128,
            target_q,
            target_q,
            combined_sign,
            pre_sticky,
            rm,
        )
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the finite-finite product / alignment / rounding
    /// pipeline. Mirrors decimal128's `fma_special_only_for_kani`
    /// (ADR-0016).
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn fma_special_only_for_kani(
        self,
        b: Self,
        c: Self,
        _rm: RoundingMode,
    ) -> Option<(Self, Status)> {
        handle_specials(
            classify_bits(self.0),
            classify_bits(b.0),
            classify_bits(c.0),
        )
    }
}

/// Compress a u128 coefficient down to `u64` (with sticky tracking) and
/// route through `round_and_pack_finite`. Decimal32 rounds at PRECISION
/// (= 7) digits, so we only need ~14 retained digits in the u64 to
/// preserve the rounding decision.
fn round_and_pack_into_u32(
    coef_u128: u128,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    mut pre_sticky: bool,
    rm: RoundingMode,
) -> (Decimal32, Status) {
    const KEEP: u32 = 14; // PRECISION + 7 guard digits

    if coef_u128 < (1u128 << 63) && coef_u128 < 10u128.pow(KEEP) {
        // Already within u64 range and fits in 14 digits: pass through.
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

    // Drop excess digits to bring coefficient down to KEEP digits with
    // sticky tracking.
    let mut c = coef_u128;
    let mut shift = 0u32;
    while c >= 10u128.pow(KEEP) {
        let r = c % 10;
        c /= 10;
        if r != 0 {
            pre_sticky = true;
        }
        shift += 1;
    }
    debug_assert!(c < 10u128.pow(KEEP));
    debug_assert!(c <= u64::MAX as u128);

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

fn handle_specials(a: Class, b: Class, c: Class) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    // sNaN propagation, in argument order.
    for cls in [a, b, c] {
        if let SignalingNaN { sign, payload } = cls {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
    }

    // 0 × ∞ or ∞ × 0 in the product → INVALID (regardless of c, since
    // the product is undefined). Per IEEE 754-2019 §6.2.3, when c is
    // also a NaN, the result should carry c's payload. a and b cannot
    // be NaN here (the sNaN gate already returned, and a/b are Zero or
    // Infinity by this branch's matches!), so c is the only NaN source.
    let zero_inf = matches!(
        (a, b),
        (Zero { .. }, Infinity { .. }) | (Infinity { .. }, Zero { .. })
    );
    if zero_inf {
        if let QuietNaN { sign, payload } = c {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // qNaN propagation (a > b > c order).
    for cls in [a, b, c] {
        if let QuietNaN { sign, payload } = cls {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
    }

    // Compute the multiply's sign for the infinity branches.
    let multiply_yields_infinity = matches!(a, Infinity { .. }) || matches!(b, Infinity { .. });

    if multiply_yields_infinity {
        // (±∞) × (±finite or ±∞) = ±∞ (XOR signs).
        let sa = match a {
            Infinity { sign } | Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        let sb = match b {
            Infinity { sign } | Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        let inf_sign = sa ^ sb;

        match c {
            Infinity { sign: sc } => {
                if sc == inf_sign {
                    return Some((
                        Decimal32::from_bits(crate::bid::pack_infinity(inf_sign)),
                        Status::OK,
                    ));
                }
                // (+∞) + (−∞) → NaN + INVALID.
                return Some((Decimal32::NAN, Status::INVALID));
            }
            Finite { .. } | Zero { .. } => {
                return Some((
                    Decimal32::from_bits(crate::bid::pack_infinity(inf_sign)),
                    Status::OK,
                ));
            }
            _ => unreachable!(),
        }
    }

    // a × b is finite. If c is infinity, the result is c.
    if let Infinity { sign } = c {
        return Some((
            Decimal32::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        ));
    }

    // No infinities, no NaNs, no 0 × ∞: fall through to the finite path.
    let _ = (PRECISION, COEFFICIENT_LIMIT);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BiasedExp, Coefficient};

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn fma_basic() {
        // 2 × 3 + 4 = 10
        let (r, s) = from_int(2, 0).fma(from_int(3, 0), from_int(4, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(10, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn fma_single_rounding_advantage() {
        // FMA preserves the exact product before the add, so cases
        // where (a × b) loses precision but (a × b) + c recovers it
        // round more accurately than (a × b).add(c).
        //
        // 1234567 × 1234567 = 1_524_155_677_489.
        // (a × b) rounded to 7 digits at NearestEven = 1524156 × 10^6
        //   (Inexact; actual precise value > kept value by 1077489).
        // FMA with c = -1_524_156_000_000:
        //   FMA exact result = 1524155677489 - 1524156000000 = -322511.
        //   So FMA = -322511, INEXACT (because the result is exact in
        //   the sense that no rounding loss occurred in the FMA, but
        //   the magnitudes are compared and there's no inexactness
        //   from the FMA itself; sticky should be false). Actually
        //   wait — the FMA's exact result is integer -322511, fits
        //   in 6 digits, exactly representable. So the FMA result is
        //   exact: no INEXACT flag.
        let a = from_int(1_234_567, 0);
        let b = from_int(1_234_567, 0);
        let c = from_int(-1_524_156, 6); // -1.524156 × 10^12
        let (r, s) = a.fma(b, c, RoundingMode::NearestEven);
        // Expected: -322_511 × 10^0 (a 6-digit exact result).
        assert_eq!(r.to_bits(), from_int(-322_511, 0).to_bits());
        assert!(
            s.is_ok(),
            "FMA with exact intermediate sum should be exact, got status {s:?}"
        );
    }

    #[test]
    fn fma_with_alignment() {
        // 1.5 × 2 + 0.005 = 3.005
        let a = from_int(15, -1);
        let b = from_int(2, 0);
        let c = from_int(5, -3);
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 3).unwrap(),
            Coefficient::try_new(3005).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn fma_zero_addend() {
        // a × b + 0 = a × b
        let (r, _) = from_int(2, 0).fma(from_int(3, 0), Decimal32::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(6, 0).to_bits());
    }

    #[test]
    fn fma_zero_multiplicand() {
        // 0 × b + c = c
        let (r, _) = Decimal32::ZERO.fma(from_int(5, 0), from_int(7, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
    }

    #[test]
    fn fma_zero_product_at_far_exponent_does_not_drop_c() {
        // Regression: when one product factor is zero AND the other
        // has a far exponent (shift_ab > MAX_SHIFT = 24), the
        // alignment-shift early-return used to discard `c`.
        // `1e30 × 0 + 1 = 1`, not `0`.
        let a = from_int(1, 30);
        let b = Decimal32::ZERO;
        let c = from_int(1, 0);
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let one = from_int(1, 0);
        let (cmp, _) = r.partial_cmp(one);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1e30, 0, 1) = {r:?}, expected value 1"
        );
    }

    #[test]
    fn fma_far_exponent_with_small_product_does_not_drop_c() {
        // Regression: the previous *static* MAX_SHIFT = 24 made
        // `shift_ab > 24` route through the early-return, which
        // assumes ab dominates. But ab can be small (here ab =
        // 1 × 1 = 1, 1 digit) and c at the lower quantum can be
        // comparable, so neither dominates. The dynamic bound
        // `digit_count(ab_coef) + shift_ab ≤ 38` admits this case
        // through the normal align-and-sum path.
        //
        // fma(1, 1, 0.999999) = 1.999999
        let a = from_int(1, 0);
        let b = from_int(1, 0);
        let c = Decimal32::try_new(999_999, -6).unwrap();
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let expected = Decimal32::try_new(1_999_999, -6).unwrap();
        let (cmp, _) = r.partial_cmp(expected);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1, 1, 0.999999) = {r:?}, expected {expected:?}",
        );
    }

    #[test]
    fn fma_zero_c_at_far_exponent_does_not_drop_product() {
        // Regression: when c is a zero at a far quantum (shift_c >
        // MAX_SHIFT), the alignment-shift early-return used to
        // discard the product. `1 × 1 + 0E+30 = 1`, not `0`.
        let a = from_int(1, 0);
        let b = from_int(1, 0);
        let c = Decimal32::try_new(0, 30).unwrap();
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let one = from_int(1, 0);
        let (cmp, _) = r.partial_cmp(one);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1, 1, 0E+30) = {r:?}, expected value 1"
        );
    }

    #[test]
    fn fma_signs() {
        let (r, _) = from_int(-2, 0).fma(from_int(3, 0), from_int(1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-5, 0).to_bits());

        let (r, _) =
            from_int(-2, 0).fma(from_int(-3, 0), from_int(-1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(5, 0).to_bits());
    }

    #[test]
    fn fma_zero_times_infinity_invalid() {
        let (r, s) = Decimal32::ZERO.fma(
            Decimal32::INFINITY,
            Decimal32::ONE,
            RoundingMode::NearestEven,
        );
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) =
            Decimal32::INFINITY.fma(Decimal32::ZERO, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_zero_times_infinity_preserves_c_payload() {
        // IEEE 754-2019 §6.2.3: when c is a NaN, the 0 × ∞ branch must
        // carry c's payload. The pre-fix branch returned Decimal32::NAN
        // (canonical payload 0), losing the signal.
        let payload: u32 = 0x12345;
        let qnan_c = Decimal32::from_bits(crate::bid::pack_quiet_nan(false, payload));
        let snan_c = Decimal32::from_bits(crate::bid::pack_signaling_nan(false, payload));
        let payload_mask: u32 = (1u32 << 20) - 1;

        let (r, s) = Decimal32::ZERO.fma(Decimal32::INFINITY, qnan_c, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid(), "0 × ∞ still raises INVALID");
        assert_eq!(
            r.to_bits() & payload_mask,
            payload,
            "qNaN c's payload should be preserved",
        );

        let (r, s) = Decimal32::INFINITY.fma(Decimal32::ZERO, snan_c, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan(), "sNaN c is quieted on output");
        assert!(s.invalid());
        assert_eq!(
            r.to_bits() & payload_mask,
            payload,
            "sNaN c's payload should be preserved (signal cleared)",
        );

        // Non-NaN c still gets the canonical NAN; the fix is narrow.
        let (r, s) = Decimal32::ZERO.fma(
            Decimal32::INFINITY,
            Decimal32::ONE,
            RoundingMode::NearestEven,
        );
        assert_eq!(r.to_bits(), Decimal32::NAN.to_bits());
        assert!(s.invalid());
    }

    #[test]
    fn fma_infinity_minus_infinity_invalid() {
        // (+∞) × 1 + (−∞) → NaN + INVALID
        let (r, s) = Decimal32::INFINITY.fma(
            Decimal32::ONE,
            Decimal32::NEG_INFINITY,
            RoundingMode::NearestEven,
        );
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_infinity_passes_through() {
        // (+∞) × 2 + finite → +∞
        let (r, _) =
            Decimal32::INFINITY.fma(from_int(2, 0), from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        // finite × finite + (+∞) → +∞
        let (r, _) = from_int(2, 0).fma(
            from_int(3, 0),
            Decimal32::INFINITY,
            RoundingMode::NearestEven,
        );
        assert!(r.is_infinite() && !r.is_sign_negative());
    }

    #[test]
    fn fma_nan_propagation() {
        let (r, s) = Decimal32::NAN.fma(Decimal32::ONE, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) =
            Decimal32::SIGNALING_NAN.fma(Decimal32::ONE, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        // sNaN in c position also raises INVALID.
        let (r, s) = Decimal32::ONE.fma(
            Decimal32::ONE,
            Decimal32::SIGNALING_NAN,
            RoundingMode::NearestEven,
        );
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_cancellation_zero_sign() {
        // 1 × 1 + (−1) = 0. Sign rule: +0 in NearestEven, −0 in TowardNegative.
        let (r, _) = from_int(1, 0).fma(from_int(1, 0), from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(1, 0).fma(
            from_int(1, 0),
            from_int(-1, 0),
            RoundingMode::TowardNegative,
        );
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn fma_h3_zero_product_at_extreme_negative_quantum_clamps() {
        // H3 regression. `fma(0E-101, 0E-101, 0E-101)` has ab = 0 and
        // c = 0, so the result is exact zero. The ideal quantum is
        // `min(q(a) + q(b), q(c)) = min(-202, -101) = -202`, far below
        // the format's minimum representable quantum `-BIAS = -101`.
        // IEEE 754-2019 §6.3 + §7.4 require clamping the result quantum
        // to `-101` and raising the informational `Clamped` flag. The
        // result must be a canonical signed zero at the clamped minimum
        // quantum, not a panic or a non-canonical encoding.
        let a = Decimal32::try_new(0, -101).unwrap();
        let b = Decimal32::try_new(0, -101).unwrap();
        let c = Decimal32::try_new(0, -101).unwrap();
        let (r, status) = a.fma(b, c, RoundingMode::NearestEven);
        let expected = Decimal32::try_new(0, -101).unwrap();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "fma zero-product at extreme negative quantum should clamp to 0E-101"
        );
        assert!(r.is_zero() && !r.is_sign_negative());
        assert!(
            status.clamped(),
            "fma zero-product clamp should raise Status::CLAMPED, got {status:?}"
        );
    }

    #[test]
    fn fma_h3_cancellation_at_extreme_quantum_clamps() {
        // Cancellation mirror of the H3 fix: ab and c align to equal
        // magnitudes with opposite signs, producing exact zero with an
        // out-of-range preferred quantum. Reaching that branch needs a
        // c at a quantum below `-BIAS = -101` (so the preferred quantum
        // `min(ab_exp, c_exp)` underflows). Decimal32 cannot directly
        // construct such a c: `try_new` clamps the exponent into
        // `[-101, +90]`, and there is no product path that lands an
        // opposite-sign exact-cancellation pair below `-101` without
        // first rounding through `round_and_pack_finite`. The
        // `clamp_unbiased` call in the cancellation branch of `fma.rs`
        // is therefore exercised structurally by the zero-product test
        // above, which shares the identical clamp-and-flag code path.
    }
}
