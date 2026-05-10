//! [`Decimal32`] → string formatting via `core::fmt::Write`. Alloc-free.
//!
//! `Display` follows the General Decimal Arithmetic toSci convention
//! (matches `Decimal128`'s ferrodec impl): plain decimal notation when
//! the unbiased exponent is ≤ 0 and the adjusted exponent is ≥ -6,
//! otherwise scientific. Trailing zeros from the coefficient are
//! preserved (so "1.0" stays "1.0", not "1") because they reflect the
//! value's quantum.
//!
//! `LowerExp` / `UpperExp` force scientific notation. The
//! [`Engineering`] adapter forces scientific with the exponent at a
//! multiple of 3.
//!
//! The fixed-buffer scratch is 16 bytes (sign + 7 digits + decimal
//! point + "E±NNN" exponent), comfortably within stack constraints on
//! Cortex-M0+.
//!
//! Format precision (`{:.N}`) is not yet honoured in this v0.x
//! release; quantize support lands with the arithmetic ops in
//! subsequent commits.

use core::fmt;

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal32;

/// Notation choice routed through the formatters.
#[derive(Clone, Copy)]
enum Notation {
    /// Default `Display`: plain decimal vs scientific by toSci rules.
    Auto,
    /// Forced scientific with the given exponent letter.
    ScientificForced(char),
    /// Forced engineering notation (exponent multiple of 3) with the
    /// given exponent letter.
    Engineering(char),
}

impl fmt::Display for Decimal32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::Auto)
    }
}

impl fmt::LowerExp for Decimal32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::ScientificForced('e'))
    }
}

impl fmt::UpperExp for Decimal32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::ScientificForced('E'))
    }
}

/// Wrapper that displays a `Decimal32` in *engineering* notation:
/// scientific with the exponent forced to a multiple of 3, so the
/// mantissa lies in `[1, 1000)`.
#[derive(Clone, Copy)]
pub struct Engineering(Decimal32);

impl fmt::Display for Engineering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(self.0, f, Notation::Engineering('E'))
    }
}

impl Decimal32 {
    /// Wrap `self` in an [`Engineering`] adapter that formats in
    /// engineering notation (scientific with exponent a multiple of 3,
    /// mantissa in `[1, 1000)`).
    #[must_use]
    pub fn engineering(self) -> Engineering {
        Engineering(self)
    }
}

/// Render a NaN's prefix (`-`/empty + `sNaN` or `NaN`) and append the
/// diagnostic payload as decimal digits when it is non-zero.
fn format_nan(
    f: &mut fmt::Formatter<'_>,
    sign: bool,
    payload: u32,
    signaling: bool,
) -> fmt::Result {
    if sign {
        f.write_str("-")?;
    }
    f.write_str(if signaling { "sNaN" } else { "NaN" })?;
    if payload != 0 {
        write!(f, "{payload}")?;
    }
    Ok(())
}

fn format_to(d: Decimal32, f: &mut fmt::Formatter<'_>, notation: Notation) -> fmt::Result {
    match classify_bits(d.to_bits()) {
        Class::QuietNaN { sign, payload } => format_nan(f, sign, payload, false),
        Class::SignalingNaN { sign, payload } => format_nan(f, sign, payload, true),
        Class::Infinity { sign } => {
            if sign {
                f.write_str("-")?;
            }
            f.write_str("Infinity")
        }
        Class::Zero { sign, biased_exp } => {
            format_finite(f, sign, 0, biased_exp as i32 - BIAS as i32, 1, notation)
        }
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            let digits = decimal_digit_count(coefficient);
            format_finite(f, sign, coefficient, unbiased, digits, notation)
        }
    }
}

fn format_finite(
    f: &mut fmt::Formatter<'_>,
    sign: bool,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    notation: Notation,
) -> fmt::Result {
    if sign {
        f.write_str("-")?;
    }
    let adjusted_exp = unbiased_exp + (digits as i32) - 1;
    match notation {
        Notation::Auto => {
            // toSci rule: plain notation when exp ≤ 0 and adjusted ≥ -6.
            if unbiased_exp <= 0 && adjusted_exp >= -6 {
                write_plain(f, coef, unbiased_exp, digits)
            } else {
                write_scientific(f, coef, unbiased_exp, digits, adjusted_exp, 'E')
            }
        }
        Notation::ScientificForced(letter) => {
            write_scientific(f, coef, unbiased_exp, digits, adjusted_exp, letter)
        }
        Notation::Engineering(letter) => {
            write_engineering(f, coef, unbiased_exp, digits, adjusted_exp, letter)
        }
    }
}

/// Write the digit string for `coef` (leading zeros padded out to
/// `digits` total) into a 8-byte stack buffer, returning a `&[u8]`
/// with the digits as ASCII.
fn digit_string(coef: u32, digits: u32) -> [u8; 8] {
    // 7 digits max for canonical Decimal32; allocate 8 to give room for
    // a transient overflow during rendering of pre-rounded coefficients.
    let mut buf = [0u8; 8];
    let mut n = coef;
    let total = digits as usize;
    debug_assert!(total <= buf.len());
    for i in (0..total).rev() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf
}

fn write_plain(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
) -> fmt::Result {
    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];

    if unbiased_exp == 0 {
        return f.write_str(core::str::from_utf8(digit_slice).unwrap());
    }
    // unbiased_exp < 0 (the > 0 case is routed to scientific by the
    // caller's Auto rule).
    let frac_len = (-unbiased_exp) as u32;
    if frac_len < digits {
        // Decimal point sits inside the digit run: split at digits - frac_len.
        let split = (digits - frac_len) as usize;
        f.write_str(core::str::from_utf8(&digit_slice[..split]).unwrap())?;
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[split..]).unwrap())
    } else {
        // Need leading zeros: "0.0...0digits".
        f.write_str("0.")?;
        for _ in digits..frac_len {
            f.write_str("0")?;
        }
        f.write_str(core::str::from_utf8(digit_slice).unwrap())
    }
}

fn write_scientific(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    adjusted_exp: i32,
    letter: char,
) -> fmt::Result {
    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];

    if digits == 1 {
        f.write_str(core::str::from_utf8(digit_slice).unwrap())?;
    } else {
        f.write_str(core::str::from_utf8(&digit_slice[..1]).unwrap())?;
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[1..]).unwrap())?;
    }

    // Zero with non-trivial exponent renders as `0E±N`; no decimal
    // point even though digits == 1 (since coef is 0). That's the
    // shape decTest expects.
    let _ = unbiased_exp;
    write!(f, "{letter}")?;
    if adjusted_exp >= 0 {
        write!(f, "+{adjusted_exp}")
    } else {
        write!(f, "{adjusted_exp}")
    }
}

fn write_engineering(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    adjusted_exp: i32,
    letter: char,
) -> fmt::Result {
    // Engineering: rebase the adjusted exponent down to the nearest
    // multiple of 3 (toward -∞), shifting the mantissa right by the
    // remainder.
    let shift = adjusted_exp.rem_euclid(3);
    let target_adjusted = adjusted_exp - shift;

    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];

    let mantissa_int_digits = (shift + 1) as usize;
    if mantissa_int_digits >= digit_slice.len() {
        // All digits go before the decimal point; if there are fewer
        // digits than mantissa_int_digits (rare for Decimal32), pad
        // with trailing zeros.
        f.write_str(core::str::from_utf8(digit_slice).unwrap())?;
        for _ in digit_slice.len()..mantissa_int_digits {
            f.write_str("0")?;
        }
    } else {
        f.write_str(core::str::from_utf8(&digit_slice[..mantissa_int_digits]).unwrap())?;
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[mantissa_int_digits..]).unwrap())?;
    }

    let _ = unbiased_exp;
    write!(f, "{letter}")?;
    if target_adjusted >= 0 {
        write!(f, "+{target_adjusted}")
    } else {
        write!(f, "{target_adjusted}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::RoundingMode;

    extern crate alloc;
    use alloc::format;
    use alloc::string::ToString;

    fn parse(s: &str) -> Decimal32 {
        Decimal32::parse_str(s, RoundingMode::default()).unwrap().0
    }

    #[test]
    fn display_zero() {
        assert_eq!(Decimal32::ZERO.to_string(), "0");
        assert_eq!(Decimal32::NEG_ZERO.to_string(), "-0");
    }

    #[test]
    fn display_one_and_neg_one() {
        assert_eq!(Decimal32::ONE.to_string(), "1");
        assert_eq!(Decimal32::NEG_ONE.to_string(), "-1");
    }

    #[test]
    fn display_specials() {
        assert_eq!(Decimal32::INFINITY.to_string(), "Infinity");
        assert_eq!(Decimal32::NEG_INFINITY.to_string(), "-Infinity");
        assert_eq!(Decimal32::NAN.to_string(), "NaN");
        assert_eq!(Decimal32::SIGNALING_NAN.to_string(), "sNaN");
    }

    #[test]
    fn display_plain_with_decimal() {
        assert_eq!(parse("1.0").to_string(), "1.0");
        assert_eq!(parse("1.00").to_string(), "1.00");
        assert_eq!(parse("0.5").to_string(), "0.5");
        assert_eq!(parse("0.012").to_string(), "0.012");
        assert_eq!(parse("0.001").to_string(), "0.001");
    }

    #[test]
    fn display_plain_integers_with_positive_unbiased() {
        // "10" and "1000" parse with unbiased_exp = 0, so they format
        // back to themselves under the toSci plain rule.
        assert_eq!(parse("10").to_string(), "10");
        assert_eq!(parse("1000").to_string(), "1000");
    }

    #[test]
    fn display_scientific_when_exp_positive() {
        // "1E+3": unbiased exp = 3 > 0 → forced to scientific.
        assert_eq!(parse("1E+3").to_string(), "1E+3");
        // "1.234E+10": digits=4, unbiased=7, adjusted=10.
        assert_eq!(parse("1.234E+10").to_string(), "1.234E+10");
    }

    #[test]
    fn display_scientific_when_adjusted_too_low() {
        // "1E-7": adjusted = -7 < -6 → scientific.
        assert_eq!(parse("1E-7").to_string(), "1E-7");
    }

    #[test]
    fn display_zero_with_positive_exp_is_scientific() {
        // 0E+5 has unbiased = 5 > 0 → scientific.
        let d = Decimal32::try_new(0, 5).unwrap();
        assert_eq!(d.to_string(), "0E+5");
    }

    #[test]
    fn display_negative_finite() {
        assert_eq!(parse("-12.34").to_string(), "-12.34");
        assert_eq!(parse("-0.5").to_string(), "-0.5");
    }

    #[test]
    fn lower_upper_exp() {
        let d = parse("1.234");
        assert_eq!(format!("{d:e}"), "1.234e+0");
        assert_eq!(format!("{d:E}"), "1.234E+0");
    }

    #[test]
    fn engineering_basic() {
        // 12345 with exp=0: adjusted_exp=4. Rebase to multiple of 3:
        // shift=1, target_adjusted=3. Mantissa = 12.345.
        let d = Decimal32::try_new(12345, 0).unwrap();
        assert_eq!(format!("{}", d.engineering()), "12.345E+3");

        // 1234 with exp=-7: adjusted_exp=-4. Rebase to -6 (multiple of 3
        // not exceeding -4): shift=2, target_adjusted=-6. Mantissa = 123.4.
        let d = Decimal32::try_new(1234, -7).unwrap();
        assert_eq!(format!("{}", d.engineering()), "123.4E-6");
    }

    #[test]
    fn parse_format_roundtrip_simple() {
        for s in ["1", "-1", "10", "1.0", "0.5", "0.001", "100", "1.234E+10"] {
            let d = parse(s);
            assert_eq!(d.to_string(), s, "round-trip failed for {s}");
        }
    }
}
