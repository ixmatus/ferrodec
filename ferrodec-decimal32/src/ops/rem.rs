//! IEEE 754-2019 remainder for [`Decimal32`].
//!
//! Truncated remainder: `rem(a, b) = a − trunc(a / b) * b`. The result
//! has the sign of the dividend `a` and magnitude strictly less than
//! `|b|`. The quantum is `min(Q(a), Q(b))` per IEEE 754-2019 §5.3.1.
//!
//! Per the General Decimal Arithmetic spec, the operation raises
//! `Invalid_operation` (`Division_impossible`) when the truncated
//! integer quotient `trunc(|a / b|)` would exceed `PRECISION` (= 7)
//! digits, that is, when it is not less than `COEFFICIENT_LIMIT =
//! 10⁷`. That digit budget is the **sole** invalid predicate
//! (IEEE 754-2019 §5.3.1 defines the remainder; the digit overflow
//! belongs to §7.2). The alignment register is `u128`, and the
//! per-side shift bound is keyed on the operand digit count, not a
//! fixed constant, so a wide exponent gap with a small quotient (for
//! example `rem(1E+13, 9999999)`) computes its finite remainder
//! instead of raising a spurious `INVALID`.
//!
//! # Special cases (IEEE 754-2019 §7)
//!
//! * sNaN / qNaN propagation (a preferred per §6.2.3).
//! * `±∞ % anything` → NaN + `INVALID`.
//! * `anything % 0` → NaN + `INVALID`.
//! * `0 % b` (b ≠ 0) → ±0 with sign of dividend at the preferred
//!   quantum.
//! * `finite % ±∞` → finite (the dividend) at the preferred quantum.

use crate::bid::{classify_bits, Class, BIAS, COEFFICIENT_LIMIT};
use crate::decimal::Decimal32;
use ferrodec_ieee::{decimal_digit_count_u128, RoundingMode, Status};

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

/// Upper bound on `digit_count(coef) + shift` that keeps an aligned
/// coefficient within `u128::MAX`. `10³⁸ < 2¹²⁸ ≈ 3.4 × 10³⁸`, so any
/// value with at most 38 decimal digits fits. Used for the *dynamic*
/// per-side alignment bound: a side may shift by up to
/// `U128_DIGIT_CAP − digit_count(coef)` decimal positions before the
/// `u128` register would overflow. A one-digit operand leaves far
/// more headroom than a seven-digit one, so the wide-exponent-gap,
/// small-quotient regime (`rem(1E+13, 9999999)`) is admitted through
/// the normal align-and-divide path rather than rejected by a fixed
/// window.
const U128_DIGIT_CAP: u32 = 38;

// Compile-time invariant: the largest reachable index is
// `U128_DIGIT_CAP = 38`. The table needs ≥ 39 entries.
const _: () = assert!(POW10_U128.len() > U128_DIGIT_CAP as usize);

impl Decimal32 {
    /// Truncated remainder: `self − trunc(self / other) × other`.
    ///
    /// Result has the sign of `self` and magnitude strictly less than
    /// `|other|`. Returns `(NaN, INVALID)` when the integer quotient
    /// would exceed `PRECISION` (= 7) digits or when an operand makes
    /// the operation undefined per IEEE 754-2019 §5.3.1.
    ///
    /// This is the truncated (GDA / C99 `fmod`, decTest `remainder`)
    /// rule. The IEEE 754-2019 §5.3.1 nearest-even remainder
    /// (`|r| ≤ |other|/2`, decTest `remaindernear`) is a *different*
    /// operation, exposed separately as [`Decimal32::rem_near`];
    /// `Decimal128::rem` is the nearest-even one while this sibling
    /// `rem` is the truncated one, a deliberate per-format choice and
    /// a documented API hazard (ADR-0027, which records the 2.0 plan
    /// to retire the bare, ambiguous spelling).
    ///
    /// The `rm` parameter is unused (`rem` is exact when defined) but
    /// kept on the signature for parity with the other arithmetic
    /// methods.
    #[must_use]
    pub fn rem(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let _ = rm; // exact operation; rm carried for API parity
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        // Zero dividend → ±0 at preferred quantum (sign preserved).
        if coef_a == 0 {
            // target_q = min(exp_a, exp_b) is within the classify_bits
            // range; biased conversion is in range.
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        // Align both operands at target_q over a u128 register. Only
        // one shift can be non-zero (the side with the larger exponent
        // shifts up to reach `target_q = min(exp_a, exp_b)`).
        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        // Dynamic per-side bound, mirroring the in-crate `fma.rs`
        // shape and the decimal64 `rem.rs` H5 fix. The static
        // `MAX_SAFE_SHIFT = 12` over a `u64` register conflated
        // "aligning the operand overflows the register" with
        // "the integer quotient exceeds PRECISION digits". Those are
        // distinct: the GDA `Division_impossible` predicate is the
        // latter alone (IEEE 754-2019 §5.3.1 defines the remainder;
        // §7.2 owns the digit-budget overflow). The static window
        // rejected pairs like `rem(1E+13, 9999999)` whose true
        // integer quotient is `1_000_000` (7 digits, well inside
        // PRECISION) and whose remainder `1_000_000` is representable.
        //
        // The remaining overflow case `shift_a > ab_safe_shift`
        // implies `D_a + shift_a > U128_DIGIT_CAP = 38`. With the
        // dividend dominant (`shift_b = 0`) and `D_b ≤ 7`, the
        // quotient digit count is at least
        // `(D_a + shift_a) − D_b − 1 ≥ 38 − 7 = 31 ≫ 7`, so the
        // operation is genuinely `Division_impossible` and the
        // `INVALID` return is spec-correct. The exact digit check for
        // in-register alignments is the `quotient >= COEFFICIENT_LIMIT`
        // test below, which stays the sole `Invalid_operation`
        // predicate.
        let d_a = decimal_digit_count_u128(coef_a as u128);
        let d_b = decimal_digit_count_u128(coef_b as u128);
        let ab_safe_shift = U128_DIGIT_CAP - d_a;
        let bb_safe_shift = U128_DIGIT_CAP - d_b;

        if shift_a > ab_safe_shift {
            // |a| ≫ |b| at target_q: the truncated integer quotient
            // necessarily exceeds PRECISION digits (see the bound
            // argument above).
            return (Decimal32::NAN, Status::INVALID);
        }

        if shift_b > bb_safe_shift {
            // |b| ≫ |a| at target_q: trunc(a / b) = 0 and the
            // remainder is `a` itself, packed at `target_q = exp_a`.
            // exp_a and coef_a came from classify_bits.
            let biased_exp =
                crate::bid::BiasedExp::try_from_unbiased(exp_a).expect("exp_a from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coef_a as u32)
                .expect("coef_a < COEFFICIENT_LIMIT from classify_bits");
            return (
                Decimal32::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
                Status::OK,
            );
        }

        let aligned_a = u128::from(coef_a) * POW10_U128[shift_a as usize];
        let aligned_b = u128::from(coef_b) * POW10_U128[shift_b as usize];
        debug_assert!(aligned_b > 0); // zero divisor handled by dispatcher

        let quotient = aligned_a / aligned_b;
        // Per the GDA spec, the integer quotient must fit in PRECISION
        // digits (≤ 9_999_999). If it doesn't, the operation is
        // invalid. This is the sole `Invalid_operation` predicate.
        if quotient >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal32::NAN, Status::INVALID);
        }
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        // Sign of remainder = sign of dividend; magnitude is `residue`
        // packed at `target_q`. The residue is strictly less than the
        // *unaligned* divisor coefficient expressed at the target
        // quantum: `residue < aligned_b` and dividing both by
        // `10^shift_b` recovers `coef_b < COEFFICIENT_LIMIT = 10^7`.
        // It therefore always fits seven digits and a `u32`. The guard
        // below is a defensive check against a non-canonical alignment
        // that the bound argument did not anticipate.
        if residue >= u128::from(COEFFICIENT_LIMIT) {
            // Should not happen for canonical inputs, but guard
            // against pathological alignment.
            return (Decimal32::NAN, Status::INVALID);
        }

        // target_q is bounded by min of two classify_bits-derived exponents,
        // residue < COEFFICIENT_LIMIT checked above.
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
            .expect("target_q from classify_bits-derived exponents");
        let coefficient = crate::bid::Coefficient::try_new(residue as u32)
            .expect("residue < COEFFICIENT_LIMIT checked above");
        (
            Decimal32::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.1 nearest-even remainder:
    /// `rem_near(a, b) = a − n·b` where `n` is `a / b` rounded to the
    /// nearest integer, ties to even.
    ///
    /// Result magnitude `≤ |b| / 2`; always exact when defined; never
    /// raises `INEXACT`. This mirrors `Decimal128::rem` (the parent's
    /// `rem` is this nearest-even op), and is distinct from this
    /// crate's truncated [`Decimal32::rem`]. The decTest spelling is
    /// `remaindernear`. ADR-0027 records why both spellings exist and
    /// the 2.0 plan to make the explicit name the only one.
    ///
    /// Special cases follow IEEE 754-2019 §5.3.1 exactly as the
    /// truncated `rem` does (they share `handle_specials`): NaN
    /// propagation, sNaN → `INVALID`, `a / 0` and `±∞ / b` →
    /// `NaN + INVALID`, `a / ±∞` → `a`, `±0` → `±0` with the sign of
    /// `a`. `Division_impossible` (the truncated integer quotient
    /// exceeds `PRECISION` digits) → `NaN + INVALID`.
    #[must_use]
    pub fn rem_near(self, other: Self) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        if coef_a == 0 {
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        // Same dynamic per-side alignment bounds as the truncated
        // `rem`; an overflowing `shift_a` means the integer quotient
        // already exceeds PRECISION digits, so `INVALID` is
        // spec-correct.
        let d_a = decimal_digit_count_u128(coef_a as u128);
        let d_b = decimal_digit_count_u128(coef_b as u128);
        let ab_safe_shift = U128_DIGIT_CAP - d_a;
        let bb_safe_shift = U128_DIGIT_CAP - d_b;

        if shift_a > ab_safe_shift {
            return (Decimal32::NAN, Status::INVALID);
        }

        if shift_b > bb_safe_shift {
            // |b| ≫ |a| at target_q: |a / b| ≪ 1/2, nearest integer
            // is 0, the remainder is exactly `a` (same as truncated).
            let biased_exp =
                crate::bid::BiasedExp::try_from_unbiased(exp_a).expect("exp_a from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coef_a as u32)
                .expect("coef_a < COEFFICIENT_LIMIT from classify_bits");
            return (
                Decimal32::from_bits(crate::bid::pack_finite(sign_a, biased_exp, coefficient)),
                Status::OK,
            );
        }

        let aligned_a = u128::from(coef_a) * POW10_U128[shift_a as usize];
        let aligned_b = u128::from(coef_b) * POW10_U128[shift_b as usize];
        debug_assert!(aligned_b > 0);

        let quotient = aligned_a / aligned_b;
        // `Division_impossible` keys off the *truncated* integer
        // quotient digit count, the same predicate as truncated `rem`.
        if quotient >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal32::NAN, Status::INVALID);
        }
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        // Round the truncated quotient to nearest-even on the residue
        // versus `aligned_b / 2`, ties to the quotient's low bit.
        let round_up = compare_remainder_to_half(residue, aligned_b, (quotient & 1) as u32);
        let (mut result_mag, sign_flip) = if round_up {
            (aligned_b - residue, true)
        } else {
            (residue, false)
        };

        if result_mag == 0 {
            // Exact multiple: ±0 with the sign of `a`, at target_q.
            let biased_exp = crate::bid::BiasedExp::try_from_unbiased(target_q)
                .expect("target_q from classify_bits-derived exponents");
            return (
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign_a,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        // The IEEE remainder is exactly representable when the
        // operands are, so any coefficient above PRECISION digits is
        // trailing zeros: shift them out, raising the quantum (mirrors
        // `Decimal128::rem`'s re-encode normalisation).
        let result_sign = sign_a ^ sign_flip;
        let mut q_unbiased = target_q;
        while result_mag >= u128::from(COEFFICIENT_LIMIT) {
            debug_assert!(
                result_mag % 10 == 0,
                "rem_near result not exactly representable"
            );
            result_mag /= 10;
            q_unbiased += 1;
        }
        let biased_exp = crate::bid::BiasedExp::try_from_unbiased(q_unbiased)
            .expect("rem_near quantum stays in range (|r| ≤ |b|/2)");
        let coefficient = crate::bid::Coefficient::try_new(result_mag as u32)
            .expect("result_mag < COEFFICIENT_LIMIT after trailing-zero normalisation");
        (
            Decimal32::from_bits(crate::bid::pack_finite(
                result_sign,
                biased_exp,
                coefficient,
            )),
            Status::OK,
        )
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the finite-finite quotient pipeline. Mirrors
    /// decimal128's `rem_special_only_for_kani` (ADR-0016).
    ///
    /// L4: there is deliberately no `verify/rem.rs` consuming this
    /// shim, matching the Decimal64 and Decimal128 state (both carry
    /// the shim without a `rem` proof). `rem`'s special cases (NaN,
    /// infinity, zero divisor) are exercised by the unit tests and
    /// the Decimal64 cross-check oracle; a `rem` Kani proof is parity
    /// neutral follow up work, not a gap unique to Decimal32. The
    /// shim is kept so the proof can be added in lockstep across the
    /// three crates if that follow up is taken.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn rem_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        handle_specials(classify_bits(self.0), classify_bits(rhs.0))
    }
}

/// `residue` versus `aligned_b / 2`, ties broken by the integer
/// quotient's low bit (round half to even). Returns `true` when the
/// truncated quotient should be rounded up by one. Mirrors the
/// `Decimal128` `rem` half-even comparison.
fn compare_remainder_to_half(residue: u128, aligned_b: u128, quotient_lsb: u32) -> bool {
    if residue == 0 {
        return false;
    }
    let (two_r, overflow) = residue.overflowing_mul(2);
    if overflow {
        // 2·residue > u128::MAX ≥ aligned_b, so residue > aligned_b / 2.
        return true;
    }
    match two_r.cmp(&aligned_b) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => quotient_lsb == 1,
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    // ±∞ % anything → NaN + INVALID.
    if matches!(a, Infinity { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // anything % 0 → NaN + INVALID.
    if matches!(b, Zero { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // finite % ±∞ → finite (the dividend, sign preserved).
    if matches!(b, Infinity { .. }) {
        if let Finite {
            sign,
            biased_exp,
            coefficient,
        } = a
        {
            let biased_exp = crate::bid::BiasedExp::try_from_biased(biased_exp)
                .expect("biased_exp from classify_bits");
            let coefficient = crate::bid::Coefficient::try_new(coefficient)
                .expect("coefficient from classify_bits");
            return Some((
                Decimal32::from_bits(crate::bid::pack_finite(sign, biased_exp, coefficient)),
                Status::OK,
            ));
        }
        if let Zero { sign, biased_exp } = a {
            let biased_exp = crate::bid::BiasedExp::try_from_biased(biased_exp)
                .expect("biased_exp from classify_bits");
            return Some((
                Decimal32::from_bits(crate::bid::pack_finite(
                    sign,
                    biased_exp,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn rem_basic() {
        // 10 % 3 = 1
        let (r, s) = from_int(10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());
        assert!(s.is_ok());

        // 10 % 5 = 0
        let (r, _) = from_int(10, 0).rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // 10 % -3 = 1 (sign of dividend)
        let (r, _) = from_int(10, 0).rem(from_int(-3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        // -10 % 3 = -1
        let (r, _) = from_int(-10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());
    }

    #[test]
    fn rem_quantum_min() {
        // 1.5 % 0.5 = 0.0 at quantum -1 (min of -1 and -1).
        let (r, _) = from_int(15, -1).rem(from_int(5, -1), RoundingMode::NearestEven);
        assert!(r.is_zero());
        // The result preserves the min-quantum cohort.
        // rem doesn't strip trailing zeros: result is "0E-1" = "0.0".
        let _ = r;
    }

    #[test]
    fn rem_zero_dividend() {
        // 0 % 5 = +0 (sign of dividend)
        let (r, _) = Decimal32::ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // -0 % 5 = -0
        let (r, _) = Decimal32::NEG_ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn rem_near_nearest_even_quotient() {
        // 7.5 rem_near 2.0: round_half_even(3.75) = 4, 7.5 − 4·2 = -0.5
        // (the documented Decimal128::rem example; the sibling now
        // matches it).
        let (r, s) = from_int(75, -1).rem_near(from_int(20, -1));
        assert_eq!(r.to_bits(), from_int(-5, -1).to_bits());
        assert!(s.is_ok());

        // 10 rem_near 3 = 1 (10/3 ≈ 3.33, n = 3).
        let (r, _) = from_int(10, 0).rem_near(from_int(3, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        // -10 rem_near 3 = -1.
        let (r, _) = from_int(-10, 0).rem_near(from_int(3, 0));
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());

        // |b| ≫ |a|: 1 rem_near 10 = 1 (n = 0).
        let (r, _) = from_int(1, 0).rem_near(from_int(10, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());
    }

    #[test]
    fn rem_near_half_tie_rounds_to_even_quotient() {
        // Exact half quotient ties to the even integer quotient, so
        // the parity of trunc(a/b) flips the sign of the result.
        // 10/4 = 2.5  → n = 2 (even)  → 10 − 8  =  2
        let (r, _) = from_int(10, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
        // 30/4 = 7.5  → n = 8 (even)  → 30 − 32 = -2
        let (r, _) = from_int(30, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(-2, 0).to_bits());
        // 50/4 = 12.5 → n = 12 (even) → 50 − 48 =  2
        let (r, _) = from_int(50, 0).rem_near(from_int(4, 0));
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
    }

    #[test]
    fn rem_near_exact_multiple_keeps_dividend_sign() {
        // -8 rem_near 4 = -0 (exact multiple; sign of the dividend).
        let (r, s) = from_int(-8, 0).rem_near(from_int(4, 0));
        assert!(r.is_zero() && r.is_sign_negative());
        assert!(s.is_ok());

        // 0 rem_near 5 = +0.
        let (r, _) = Decimal32::ZERO.rem_near(from_int(5, 0));
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn rem_near_special_cases_match_truncated() {
        // Shares handle_specials with `rem`, so the special lattice is
        // identical: x / 0 and ∞ / y → NaN+INVALID, x / ∞ → x.
        let (r, s) = from_int(1, 0).rem_near(Decimal32::ZERO);
        assert!(r.is_quiet_nan() && s.invalid());
        let (r, s) = Decimal32::INFINITY.rem_near(from_int(1, 0));
        assert!(r.is_quiet_nan() && s.invalid());
        let (r, s) = from_int(7, 0).rem_near(Decimal32::INFINITY);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
        assert!(s.is_ok());
        let (r, s) = Decimal32::SIGNALING_NAN.rem_near(from_int(1, 0));
        assert!(r.is_quiet_nan() && s.invalid());
    }

    #[test]
    fn rem_by_zero_invalid() {
        let (r, s) = from_int(5, 0).rem(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::ZERO.rem(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_infinity() {
        // ∞ % anything → NaN + INVALID
        let (r, s) = Decimal32::INFINITY.rem(from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        // finite % ∞ → finite (dividend)
        let (r, s) = from_int(7, 0).rem(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn rem_h2_wide_gap_small_quotient_is_finite() {
        // H2 regression. The static `MAX_SAFE_SHIFT = 12` over a u64
        // register raised a spurious `INVALID` whenever the alignment
        // shift exceeded 12, conflating register overflow with the
        // GDA `Division_impossible` digit-budget test. Sound witness
        // (Step 2 evidence): `rem(1E+13, 9999999)`. The shift is
        // `13 − 0 = 13 > 12`, but the true integer quotient is
        // `10^13 / 9_999_999 = 1_000_000` (7 digits, inside
        // PRECISION) and the remainder `1_000_000` is representable.
        // Spec answer: `1.000000E+6`, `OK`. The pre-fix code returned
        // `(NaN, INVALID)`.
        let a = Decimal32::try_new(1, 13).unwrap();
        let b = Decimal32::try_new(9_999_999, 0).unwrap();
        let (r, s) = a.rem(b, RoundingMode::NearestEven);
        assert!(
            s.is_ok(),
            "rem(1E+13, 9999999) must be finite, got status {s:?}"
        );
        let expected = Decimal32::try_new(1_000_000, 0).unwrap();
        let (cmp, _) = r.partial_cmp(expected);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "rem(1E+13, 9999999) = {r:?}, expected value 1_000_000",
        );

        // Zero-remainder companion witness: `rem(1E+13, 5000000)`.
        // Quotient `2_000_000` (7 digits), remainder `0`.
        let a = Decimal32::try_new(1, 13).unwrap();
        let b = Decimal32::try_new(5_000_000, 0).unwrap();
        let (r, s) = a.rem(b, RoundingMode::NearestEven);
        assert!(s.is_ok(), "rem(1E+13, 5000000) must be finite, got {s:?}");
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn rem_pinned_known_issue_h3_is_spec_invalid() {
        // The 2026-05-15 KNOWN_ISSUES H3 pin
        // `rem(4.194304E+33, -3.145728E+18)` was an unsound oracle
        // false positive, not a defect: the true integer quotient
        // `4194304E+15 / 3145728 ≈ 1.33E+15` has ~16 digits, far
        // beyond Decimal32's PRECISION = 7, so GDA mandates
        // `Invalid_operation`. Decimal64's finite remainder there is
        // an artifact of its own wider 10^16 digit budget. The
        // dynamic bound preserves this spec-correct `INVALID`.
        let a = Decimal32::try_new(4_194_304, 33).unwrap();
        let b = Decimal32::try_new(-3_145_728, 18).unwrap();
        let (r, s) = a.rem(b, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(
            s.invalid(),
            "quotient has ~16 digits ≫ PRECISION = 7, INVALID is spec-correct"
        );
    }

    #[test]
    fn rem_too_large_quotient_invalid() {
        // MAX % MIN_POSITIVE — quotient would have ~190+ digits, way
        // more than PRECISION = 7. INVALID.
        let (r, s) = Decimal32::MAX.rem(Decimal32::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_dividend_smaller_than_divisor() {
        // 3 % 10 = 3 (trunc(3/10) = 0)
        let (r, _) = from_int(3, 0).rem(from_int(10, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());

        // 1e-100 % 1 = 1e-100 at quantum -100.
        let small = Decimal32::try_new(1, -100).unwrap();
        let one = Decimal32::ONE;
        let (r, _) = small.rem(one, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), small.to_bits());
    }

    #[test]
    fn rem_nan_propagation() {
        let (r, s) = Decimal32::NAN.rem(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.rem(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
