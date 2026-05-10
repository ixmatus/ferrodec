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

use crate::bid::{classify_bits, BIAS, Class, COEFFICIENT_LIMIT, PRECISION};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

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

/// Maximum u128 shift we apply to either operand during alignment.
/// `10¹⁴ × 10²⁴ = 10³⁸ < 2¹²⁸` so 24 is safe for the product side; the
/// `c` side has `coef ≤ 10⁷` so it can shift further. We unify under
/// the more conservative cap and accept that beyond 24 the smaller
/// operand only contributes via sticky.
const MAX_SHIFT: u32 = 24;

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
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite a/b"),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite a/b"),
        };
        let (sign_c, biased_c, coef_c) = match cc {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite c"),
        };

        let ab_sign = sign_a ^ sign_b;
        let ab_coef = coef_a * coef_b; // u32 × u32 → u64 (max 10¹⁴)
        let ab_exp = (biased_a as i32 - BIAS as i32) + (biased_b as i32 - BIAS as i32);
        let c_exp = biased_c as i32 - BIAS as i32;

        // Both zero: §6.3 sign rule (cancellation between ab=0 and c=0).
        if ab_coef == 0 && coef_c == 0 {
            let q_preferred = ab_exp.min(c_exp);
            let result_sign = zero_sum_sign(ab_sign, sign_c, rm);
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    result_sign,
                    (q_preferred + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            );
        }

        // Compute alignment shifts. target_q is the lower of the two
        // exponents; both operands shift left by their differential.
        let target_q = ab_exp.min(c_exp);
        let shift_ab = (ab_exp - target_q) as u32;
        let shift_c = (c_exp - target_q) as u32;

        let mut pre_sticky = false;

        let ab_u128: u128 = if shift_ab <= MAX_SHIFT {
            (ab_coef as u128) * POW10_U128[shift_ab as usize]
        } else {
            // ab dominates; c is far below the working window. Set
            // sticky from c and use ab's coefficient at its own quantum.
            // We'll re-target_q to ab_exp.
            // (Actually unreachable in practice for Decimal32: shift_ab
            // > 24 would require c_exp << ab_exp by > 24. Since ab_exp
            // ∈ [-202, 192] and c_exp ∈ [-101, 96], the maximum
            // shift_ab is 192 - (-101) = 293 — yes possible.)
            pre_sticky |= coef_c != 0;
            // Re-anchor target_q at ab_exp's level: skip c entirely.
            let ab_only = ab_coef as u128;
            return round_and_pack_into_u32(ab_only, ab_exp, ab_exp, ab_sign, pre_sticky, rm);
        };

        let c_u128: u128 = if shift_c <= MAX_SHIFT {
            (coef_c as u128) * POW10_U128[shift_c as usize]
        } else {
            // c dominates; ab is far below. Truncate ab to sticky and
            // re-target at c_exp.
            pre_sticky |= ab_coef != 0;
            let c_only = coef_c as u128;
            return round_and_pack_into_u32(c_only, c_exp, c_exp, sign_c, pre_sticky, rm);
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
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    result_sign,
                    (q_preferred + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
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
    // the product is undefined).
    let zero_inf = matches!((a, b), (Zero { .. }, Infinity { .. }) | (Infinity { .. }, Zero { .. }));
    if zero_inf {
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
    use crate::bid::pack_finite;

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
        assert!(s.is_ok(), "FMA with exact intermediate sum should be exact, got status {s:?}");
    }

    #[test]
    fn fma_with_alignment() {
        // 1.5 × 2 + 0.005 = 3.005
        let a = from_int(15, -1);
        let b = from_int(2, 0);
        let c = from_int(5, -3);
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 3, 3005));
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
        let (r, _) =
            Decimal32::ZERO.fma(from_int(5, 0), from_int(7, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
    }

    #[test]
    fn fma_signs() {
        let (r, _) =
            from_int(-2, 0).fma(from_int(3, 0), from_int(1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-5, 0).to_bits());

        let (r, _) =
            from_int(-2, 0).fma(from_int(-3, 0), from_int(-1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(5, 0).to_bits());
    }

    #[test]
    fn fma_zero_times_infinity_invalid() {
        let (r, s) =
            Decimal32::ZERO.fma(Decimal32::INFINITY, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) =
            Decimal32::INFINITY.fma(Decimal32::ZERO, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
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
        let (r, _) = from_int(2, 0).fma(from_int(3, 0), Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
    }

    #[test]
    fn fma_nan_propagation() {
        let (r, s) =
            Decimal32::NAN.fma(Decimal32::ONE, Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.fma(
            Decimal32::ONE,
            Decimal32::ONE,
            RoundingMode::NearestEven,
        );
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
        let (r, _) =
            from_int(1, 0).fma(from_int(1, 0), from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) =
            from_int(1, 0).fma(from_int(1, 0), from_int(-1, 0), RoundingMode::TowardNegative);
        assert!(r.is_zero() && r.is_sign_negative());
    }
}
