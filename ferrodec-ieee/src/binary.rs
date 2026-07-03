//! Exactness test for decimal to binary-float conversion (fd-aqs.12).
//!
//! `Decimal*::to_f64` / `to_f32` render the value to a decimal string and
//! parse it into the binary float, which is correctly rounded but leaves
//! the `INEXACT` flag to be decided separately. The pre-fd-aqs.12
//! implementations decided it wrong in both directions: the Decimal128
//! string path raised `INEXACT` unconditionally (even for `ONE.to_f64`),
//! while the Decimal64/32 `to_f64` numerical path never raised it at all.
//! This predicate decides it exactly, and [`binary_conversion_status`]
//! packages the surrounding overflow / underflow / subnormal rules so all
//! three siblings share one implementation.

use crate::status::Status;

/// IEEE 754-2019 §5.4.2 `convertFormat` status for a finite nonzero
/// decimal `coefficient × 10^exponent` converted to a correctly-rounded
/// binary float with `mantissa_bits` significand bits (fd-aqs.12).
///
/// The caller passes the converted float's flags: `is_inf` (rounded to
/// ±∞ ⇒ `OVERFLOW | INEXACT`), `is_zero` (a finite nonzero value rounding
/// to ±0 ⇒ `UNDERFLOW | INEXACT`), and `is_subnormal` (a subnormal result
/// is conservatively `INEXACT`, because [`decimal_is_binary_exact`] models
/// only the significand, not the reduced precision of the subnormal
/// range). Otherwise the conversion is exact — `Status::OK` — iff the
/// value is exactly representable.
#[must_use]
pub fn binary_conversion_status(
    coefficient: u128,
    exponent: i32,
    is_inf: bool,
    is_zero: bool,
    is_subnormal: bool,
    mantissa_bits: u32,
) -> Status {
    if is_inf {
        Status::OVERFLOW | Status::INEXACT
    } else if is_zero {
        Status::UNDERFLOW | Status::INEXACT
    } else if is_subnormal {
        Status::INEXACT
    } else if decimal_is_binary_exact(coefficient, exponent, mantissa_bits) {
        Status::OK
    } else {
        Status::INEXACT
    }
}

/// Whether the finite decimal value `coefficient × 10^exponent` is
/// *exactly* representable in a binary floating-point format with
/// `mantissa_bits` significand bits (53 for `f64`, 24 for `f32`).
///
/// The target's exponent range is deliberately ignored: overflow (the
/// value rounds to ±∞) and underflow into the subnormal range are
/// detected by the caller from the converted float, where a subnormal or
/// infinite result is treated as inexact regardless of this predicate. So
/// this answers only the significand question, which is what a
/// normal-range conversion needs.
///
/// A decimal is exactly a binary float iff, written in lowest terms, its
/// denominator is a power of two and its odd part fits the significand:
/// `coefficient × 10^exponent = odd × 2^k` with `odd < 2^mantissa_bits`.
///
/// * For `exponent ≥ 0` the value is the integer
///   `coefficient × 5^exponent × 2^exponent`, whose odd part is
///   `oddpart(coefficient) × 5^exponent` (`5^exponent` is odd). It fits iff
///   that product stays below `2^mantissa_bits`.
/// * For `exponent < 0` the value is
///   `coefficient / (2^-exponent × 5^-exponent)`, which is dyadic iff
///   `5^-exponent` divides `coefficient`; the quotient's odd part must
///   then fit.
///
/// Both loops terminate early and without overflow: in the `exponent ≥ 0`
/// arm the odd part is checked against the bound *before* each multiply
/// by five (so it never exceeds `2^mantissa_bits · 5 < 2^56`), and in the
/// `exponent < 0` arm the coefficient loses a factor of five each step, so
/// the loop stops after at most `⌊log₅(coefficient)⌋ + 1` iterations
/// however large `|exponent|` is. `coefficient == 0` is exact.
#[must_use]
pub fn decimal_is_binary_exact(coefficient: u128, exponent: i32, mantissa_bits: u32) -> bool {
    if coefficient == 0 {
        return true;
    }
    let bound = 1u128 << mantissa_bits; // 2^mantissa_bits
    if exponent >= 0 {
        // Odd part of coefficient × 5^exponent = oddpart(coefficient) × 5^exponent.
        let mut odd = coefficient;
        while odd & 1 == 0 {
            odd >>= 1;
        }
        for _ in 0..exponent {
            if odd >= bound {
                // Already too large; multiplying by five only grows it.
                return false;
            }
            odd *= 5; // odd < bound ≤ 2^53 ⇒ odd·5 < 2^56, no overflow.
        }
        odd < bound
    } else {
        // Value = coefficient / (2^-exponent · 5^-exponent). Dyadic iff
        // 5^-exponent divides the coefficient.
        let mut c = coefficient;
        // `exponent` is i32; negating in i64 avoids i32::MIN overflow.
        let steps = -(i64::from(exponent));
        for _ in 0..steps {
            if c % 5 != 0 {
                return false;
            }
            c /= 5;
        }
        while c & 1 == 0 {
            c >>= 1;
        }
        c < bound
    }
}

#[cfg(test)]
mod tests {
    use super::decimal_is_binary_exact;

    #[test]
    fn exact_and_inexact_f64() {
        // Exactly representable in f64 (mantissa 53).
        assert!(decimal_is_binary_exact(0, 0, 53)); // 0
        assert!(decimal_is_binary_exact(1, 0, 53)); // 1
        assert!(decimal_is_binary_exact(5, -1, 53)); // 0.5
        assert!(decimal_is_binary_exact(25, -2, 53)); // 0.25
        assert!(decimal_is_binary_exact(125, -3, 53)); // 0.125
        assert!(decimal_is_binary_exact(15, -1, 53)); // 1.5
        assert!(decimal_is_binary_exact(1 << 53, 0, 53)); // 2^53
        assert!(decimal_is_binary_exact(100, 0, 53)); // 100
        assert!(decimal_is_binary_exact(3, 3, 53)); // 3000
                                                    // Not representable.
        assert!(!decimal_is_binary_exact(1, -1, 53)); // 0.1
        assert!(!decimal_is_binary_exact(3, -1, 53)); // 0.3
        assert!(!decimal_is_binary_exact(2, -1, 53)); // 0.2
        assert!(!decimal_is_binary_exact((1 << 53) + 1, 0, 53)); // 2^53+1 (54 bits)
    }

    #[test]
    fn f32_tighter_than_f64() {
        // 2^24 needs one bit beyond f32's 24-bit significand once you add 1.
        assert!(decimal_is_binary_exact(1 << 24, 0, 24)); // 2^24 (odd part 1)
        assert!(!decimal_is_binary_exact((1 << 24) + 1, 0, 24)); // 2^24+1
                                                                 // 0.1 is exact in neither.
        assert!(!decimal_is_binary_exact(1, -1, 24));
        // A value exact in f64 but not f32: 2^24+1 above; and 16-digit-ish
        // integers exceed f32 but fit f64.
        assert!(decimal_is_binary_exact((1 << 30) + 1, 0, 53));
        assert!(!decimal_is_binary_exact((1 << 30) + 1, 0, 24));
    }

    #[test]
    fn large_negative_exponent_terminates() {
        // A coefficient with few factors of five and a huge negative
        // exponent bails out quickly rather than looping |exponent| times.
        assert!(!decimal_is_binary_exact(7, -6000, 53));
        // 5^k · small with matching exponent is exact.
        assert!(decimal_is_binary_exact(5 * 5 * 5, -3, 53)); // 0.125
    }
}
