//! [`Decimal128`] → string formatting via `core::fmt::Write`. Alloc-free.
//!
//! `Display` produces the shortest round-trippable representation that
//! either reads as a fixed-point or scientific decimal:
//!
//! * NaN → `NaN` (or `sNaN`).
//! * `±∞` → `Infinity` / `-Infinity`.
//! * Finite, in the "comfortable" range `1e-6 ≤ |x| < 1e21`: fixed
//!   notation, e.g. `0.001`, `42`, `12345.6789`. The 1e-6 lower bound
//!   matches f64::Display in `std`.
//! * Otherwise: scientific, e.g. `1.234E-100`, `9.99E+6144`.
//!
//! The fixed-buffer scratch is at most 50 bytes (sign + 34 digits +
//! decimal point + scientific tail), comfortably within stack
//! constraints on Cortex-M0+.
//!
//! Trailing zeros from the decimal coefficient are preserved when they
//! reflect the value's quantum (e.g. `1.00` parses as
//! `100 × 10^-2` and prints back as `1.00`). Pure-integer values with
//! quantum `0` print without a decimal point.

use core::fmt;

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal128;

const MAX_DIGITS: usize = 34;

/// Lower bound on `|x|` for fixed-notation output: anything below this
/// uses scientific notation. Matches the `f64::Display` convention.
const FIXED_LOWER_LOG10: i32 = -6;

/// Strict upper bound on `|x|` for fixed-notation output.
const FIXED_UPPER_LOG10: i32 = 21;

impl fmt::Display for Decimal128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f)
    }
}

fn format_to(d: Decimal128, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match classify_bits(d.to_bits()) {
        Class::QuietNaN { sign, .. } => {
            if sign {
                f.write_str("-NaN")
            } else {
                f.write_str("NaN")
            }
        }
        Class::SignalingNaN { sign, .. } => {
            if sign {
                f.write_str("-sNaN")
            } else {
                f.write_str("sNaN")
            }
        }
        Class::Infinity { sign } => {
            if sign {
                f.write_str("-Infinity")
            } else {
                f.write_str("Infinity")
            }
        }
        Class::Zero { sign, biased_exp } => format_zero(sign, biased_exp, f),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => format_finite(sign, biased_exp, coefficient, f),
    }
}

fn format_zero(sign: bool, biased_exp: u32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let unbiased = biased_exp as i32 - BIAS as i32;
    if sign {
        f.write_str("-")?;
    }
    // Always print zero as "0" in fixed notation, except when its quantum
    // is non-trivial — `0E+5` is meaningful for cohort tracking but
    // distracting in default `Display`. Emit `0` for any quantum-0 zero,
    // and `0eN` for non-zero quantum.
    if unbiased == 0 {
        f.write_str("0")
    } else {
        f.write_str("0E")?;
        write_signed_int(unbiased, f)
    }
}

fn format_finite(
    sign: bool,
    biased_exp: u32,
    coefficient: u128,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let unbiased = biased_exp as i32 - BIAS as i32;
    let digits = decimal_digit_count(coefficient) as i32;

    // The decimal "scale" — value's order of magnitude (so 12.34 has scale 2).
    let scale = digits + unbiased;

    if sign {
        f.write_str("-")?;
    }

    // Decide fixed vs scientific.
    if scale > FIXED_LOWER_LOG10 && scale <= FIXED_UPPER_LOG10 && unbiased <= 0 {
        return format_fixed(coefficient, digits, unbiased, f);
    }
    if unbiased >= 0 && scale <= FIXED_UPPER_LOG10 {
        return format_fixed(coefficient, digits, unbiased, f);
    }
    format_scientific(coefficient, digits, unbiased, f)
}

/// Fixed-point output. `unbiased` may be negative (fractional part) or
/// non-negative (trailing zeros).
fn format_fixed(
    coefficient: u128,
    digits: i32,
    unbiased: i32,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    // Render the coefficient into a fixed buffer (digits, MSB first).
    let mut buf = [0u8; MAX_DIGITS];
    let written = write_digits(coefficient, &mut buf);
    let coef_digits = &buf[..written];

    if unbiased >= 0 {
        // Pure integer with optional trailing zeros from the quantum.
        f.write_str(core::str::from_utf8(coef_digits).expect("ASCII digits are valid UTF-8"))?;
        for _ in 0..unbiased {
            f.write_str("0")?;
        }
        return Ok(());
    }
    // Fractional. `unbiased < 0`, so |unbiased| digits sit after the point.
    let frac = (-unbiased) as i32;
    if frac >= digits {
        // Whole value is fractional. Pad with leading zeros: 0.00...0d_1d_2...
        f.write_str("0.")?;
        for _ in 0..(frac - digits) {
            f.write_str("0")?;
        }
        f.write_str(core::str::from_utf8(coef_digits).expect("ASCII digits are valid UTF-8"))?;
        return Ok(());
    }
    // Mixed integer + fractional: split coef_digits at (digits - frac).
    let int_len = (digits - frac) as usize;
    let int_part = &coef_digits[..int_len];
    let frac_part = &coef_digits[int_len..];
    f.write_str(core::str::from_utf8(int_part).expect("ASCII"))?;
    f.write_str(".")?;
    f.write_str(core::str::from_utf8(frac_part).expect("ASCII"))
}

/// Scientific notation: one integer digit, optional fractional, then `E±exp`.
fn format_scientific(
    coefficient: u128,
    digits: i32,
    unbiased: i32,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let mut buf = [0u8; MAX_DIGITS];
    let written = write_digits(coefficient, &mut buf);
    let coef_digits = &buf[..written];

    // Print first digit, then dot + remaining digits if any.
    f.write_str(core::str::from_utf8(&coef_digits[..1]).expect("ASCII"))?;
    if coef_digits.len() > 1 {
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&coef_digits[1..]).expect("ASCII"))?;
    }
    f.write_str("E")?;
    // Adjusted exponent is `(scale - 1)` = `digits + unbiased - 1`.
    let adjusted = digits + unbiased - 1;
    write_signed_int(adjusted, f)
}

/// Render a signed integer with explicit sign (`+` or `-`).
fn write_signed_int(n: i32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if n >= 0 {
        f.write_str("+")?;
    } else {
        f.write_str("-")?;
    }
    let abs = n.unsigned_abs();
    let mut buf = [0u8; 11];
    let len = write_u32_decimal(abs, &mut buf);
    f.write_str(core::str::from_utf8(&buf[..len]).expect("ASCII"))
}

/// Write the decimal digits of `n` into `buf`, MSB first, returning the
/// number of bytes written. `buf` must be at least `MAX_DIGITS` bytes.
fn write_digits(n: u128, buf: &mut [u8]) -> usize {
    debug_assert!(n != 0, "write_digits expects non-zero");
    let mut tmp = [0u8; MAX_DIGITS];
    let mut idx = MAX_DIGITS;
    let mut cur = n;
    while cur > 0 {
        idx -= 1;
        tmp[idx] = b'0' + (cur % 10) as u8;
        cur /= 10;
    }
    let len = MAX_DIGITS - idx;
    buf[..len].copy_from_slice(&tmp[idx..]);
    len
}

/// Write a `u32` as decimal into `buf` (MSB first). Returns byte count.
fn write_u32_decimal(n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 11];
    let mut idx = tmp.len();
    let mut cur = n;
    while cur > 0 {
        idx -= 1;
        tmp[idx] = b'0' + (cur % 10) as u8;
        cur /= 10;
    }
    let len = tmp.len() - idx;
    buf[..len].copy_from_slice(&tmp[idx..]);
    len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::RoundingMode;
    extern crate alloc;
    use alloc::format;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::default()).unwrap().0
    }

    #[test]
    fn format_specials() {
        assert_eq!(format!("{}", Decimal128::NAN), "NaN");
        assert_eq!(format!("{}", Decimal128::SIGNALING_NAN), "sNaN");
        assert_eq!(format!("{}", Decimal128::INFINITY), "Infinity");
        assert_eq!(format!("{}", Decimal128::NEG_INFINITY), "-Infinity");
        assert_eq!(format!("{}", Decimal128::ZERO), "0");
    }

    #[test]
    fn format_integers() {
        assert_eq!(format!("{}", Decimal128::ONE), "1");
        assert_eq!(format!("{}", Decimal128::NEG_ONE), "-1");
        assert_eq!(format!("{}", Decimal128::TEN), "10");
        assert_eq!(format!("{}", Decimal128::from_i64(123_456)), "123456");
        assert_eq!(
            format!("{}", Decimal128::from_i64(-987_654_321)),
            "-987654321"
        );
    }

    #[test]
    fn format_fixed() {
        assert_eq!(format!("{}", parse("0.5")), "0.5");
        assert_eq!(format!("{}", parse("-1.25")), "-1.25");
        assert_eq!(format!("{}", parse("0.001")), "0.001");
        assert_eq!(format!("{}", parse("100.500")), "100.500"); // preserves trailing zeros
        assert_eq!(format!("{}", parse("0.000001")), "0.000001"); // boundary
    }

    #[test]
    fn format_scientific() {
        // Small values below 1e-6 use scientific.
        assert_eq!(format!("{}", parse("1e-7")), "1E-7");
        assert_eq!(format!("{}", parse("1.5E-10")), "1.5E-10");
        // Very large values use scientific.
        assert_eq!(format!("{}", parse("1e30")), "1E+30");
    }

    #[test]
    fn format_roundtrip_small_integers() {
        for v in [-1000i64, -7, -1, 0, 1, 7, 1000, 1_000_000].iter().copied() {
            let d = Decimal128::from_i64(v);
            let s = format!("{}", d);
            let back = parse(&s);
            let (cmp, _) = back.partial_cmp(d);
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "{v} round-trip");
        }
    }

    #[test]
    fn format_roundtrip_decimals() {
        for s in &[
            "1",
            "10",
            "0.5",
            "-3.14159",
            "1.5E-10",
            "9.999999999999999999999999999999999E+6144",
            "0.1",
            "0.001",
            "100.500",
        ] {
            let d = parse(s);
            let formatted = format!("{}", d);
            let back = parse(&formatted);
            let (cmp, _) = back.partial_cmp(d);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "{s} -> {formatted} -> not equal"
            );
        }
    }
}
