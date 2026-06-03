//! Formatting a [`Decimal`] in the two General Decimal Arithmetic numeric
//! string forms: to-scientific (the canonical [`Display`](core::fmt::Display), specification
//! §"to-scientific-string") and to-engineering ([`Decimal::to_eng_string`],
//! §"to-engineering-string").
//!
//! The two forms share the special-value rendering and the plain (no-exponent)
//! form; they differ only in how an out-of-plain-range finite magnitude is laid
//! out. To-scientific places the point after the first significant digit and
//! shows the adjusted exponent. To-engineering constrains the shown exponent to
//! a multiple of three with one to three digits before the point, so a value is
//! read in the SI prefix grid (kilo, mega, milli, micro). Both are derived from
//! the specification's rules; the engineering layout for a zero coefficient is
//! the one corner the rule special-cases (its shown exponent is the exponent
//! rounded up to a multiple of three), validated against the `toEng` cases in
//! `tests/vectors/base.decTest`.

use crate::Decimal;
use alloc::string::{String, ToString};
use core::fmt;

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if write_special(self, f)? {
            return Ok(());
        }
        let (sign, coeff, exp) = self.finite_parts().expect("finite after specials");
        if sign {
            f.write_str("-")?;
        }
        write_scientific(f, &coeff.to_string(), exp)
    }
}

impl Decimal {
    /// Render in to-engineering notation (specification §"to-engineering-string").
    ///
    /// Identical to [`Display`](core::fmt::Display) (to-scientific) for special values and for any
    /// magnitude shown in plain form, but when an exponent is shown it is a
    /// multiple of three and one to three digits precede the decimal point, so
    /// the value sits on the SI prefix grid. For example `7E-7` renders as
    /// `700E-9` and `10E12` as `10E+12`.
    ///
    /// This is the spelled-out operation that produces a string; for in-place,
    /// allocation-free formatting use the [`Display`](core::fmt::Display) (to-scientific) form.
    #[must_use]
    pub fn to_eng_string(&self) -> String {
        Engineering(self).to_string()
    }
}

/// [`Display`](core::fmt::Display) wrapper that renders its [`Decimal`] in to-engineering notation.
/// Private: [`Decimal::to_eng_string`] is the public surface, but routing
/// through a `Display` keeps the renderer allocation-free at its core.
struct Engineering<'a>(&'a Decimal);

impl fmt::Display for Engineering<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0;
        if write_special(d, f)? {
            return Ok(());
        }
        let (sign, coeff, exp) = d.finite_parts().expect("finite after specials");
        if sign {
            f.write_str("-")?;
        }
        write_engineering(f, &coeff.to_string(), exp)
    }
}

/// Render a special value (NaN, sNaN, or Infinity) with its sign and any NaN
/// payload, identically in both string forms. Returns `true` if `d` was a
/// special and was written, `false` if `d` is finite and untouched.
fn write_special(d: &Decimal, f: &mut fmt::Formatter<'_>) -> Result<bool, fmt::Error> {
    if let Some((sign, signaling, payload)) = d.nan_parts() {
        if sign {
            f.write_str("-")?;
        }
        f.write_str(if signaling { "sNaN" } else { "NaN" })?;
        if !payload.is_zero() {
            write!(f, "{payload}")?;
        }
        return Ok(true);
    }
    if d.is_infinite() {
        if d.is_negative() {
            f.write_str("-")?;
        }
        f.write_str("Infinity")?;
        return Ok(true);
    }
    Ok(false)
}

/// Format a finite magnitude in to-scientific notation given its coefficient
/// digit string `c` (no leading zeros, `"0"` for zero) and exponent `exp`.
///
/// Per the specification, let `adjexp = exp + len(c) - 1`. Plain notation is
/// used when `exp <= 0 && adjexp >= -6`; otherwise scientific notation places
/// the point after the first digit and appends `E` with the signed adjusted
/// exponent.
fn write_scientific(f: &mut fmt::Formatter<'_>, c: &str, exp: i32) -> fmt::Result {
    let len = c.len() as i64;
    let adjexp = i64::from(exp) + len - 1;

    if exp <= 0 && adjexp >= -6 {
        write_plain(f, c, exp)
    } else {
        f.write_str(&c[..1])?;
        if c.len() > 1 {
            f.write_str(".")?;
            f.write_str(&c[1..])?;
        }
        f.write_str("E")?;
        if adjexp >= 0 {
            f.write_str("+")?;
        }
        write!(f, "{adjexp}")
    }
}

/// Format a finite magnitude in to-engineering notation given its coefficient
/// digit string `c` (no leading zeros, `"0"` for zero) and exponent `exp`.
///
/// The plain-form decision is the same as to-scientific (`exp <= 0 &&
/// adjexp >= -6`); the difference is the exponential layout. For a nonzero
/// coefficient the shown exponent is `adjexp` reduced to the next lower
/// multiple of three, and `adj + 1` digits (one, two, or three) precede the
/// point, where `adj = adjexp mod 3`; trailing zeros pad the integer part when
/// the coefficient is too short, and a shown exponent of zero is omitted. A
/// zero coefficient is special-cased: its shown exponent is `exp` rounded *up*
/// to a multiple of three, with the gap rendered as fractional zeros.
fn write_engineering(f: &mut fmt::Formatter<'_>, c: &str, exp: i32) -> fmt::Result {
    let len = c.len() as i64;
    let adjexp = i64::from(exp) + len - 1;

    if exp <= 0 && adjexp >= -6 {
        return write_plain(f, c, exp);
    }

    if c == "0" {
        // Zero: the shown exponent is `exp` rounded up to a multiple of three,
        // and the difference becomes fractional zeros (e.g. e-7 -> `0.0E-6`,
        // e-8 -> `0.00E-6`, e-9 -> `0E-9`). The shown exponent is never zero in
        // this branch, since exp in [-6, 0] is handled as plain above.
        let exp = i64::from(exp);
        let rem = exp.rem_euclid(3);
        let frac = (3 - rem) % 3;
        let shown = exp + frac;
        f.write_str("0")?;
        if frac > 0 {
            f.write_str(".")?;
            for _ in 0..frac {
                f.write_str("0")?;
            }
        }
        f.write_str("E")?;
        if shown >= 0 {
            f.write_str("+")?;
        }
        return write!(f, "{shown}");
    }

    // Nonzero: place `adj + 1` integer digits and reduce the shown exponent to
    // the next lower multiple of three.
    let adj = adjexp.rem_euclid(3);
    let shown = adjexp - adj;
    let intdigits = (adj + 1) as usize;
    let clen = c.len();
    if intdigits <= clen {
        f.write_str(&c[..intdigits])?;
        if intdigits < clen {
            f.write_str(".")?;
            f.write_str(&c[intdigits..])?;
        }
    } else {
        f.write_str(c)?;
        for _ in 0..(intdigits - clen) {
            f.write_str("0")?;
        }
    }
    // A shown exponent of zero (only reachable for a nonzero coefficient with a
    // small positive `exp`) is omitted, leaving a plain integer such as `100`.
    if shown != 0 {
        f.write_str("E")?;
        if shown >= 0 {
            f.write_str("+")?;
        }
        write!(f, "{shown}")?;
    }
    Ok(())
}

/// Format a finite magnitude in plain (no-exponent) notation, shared by both
/// string forms. Assumes the caller has already established the plain range
/// (`exp <= 0 && adjexp >= -6`); `c` is the coefficient digit string and `exp`
/// its exponent.
fn write_plain(f: &mut fmt::Formatter<'_>, c: &str, exp: i32) -> fmt::Result {
    if exp == 0 {
        return f.write_str(c);
    }
    let len = c.len() as i64;
    // Position of the decimal point measured from the start of `c`.
    let point = len + i64::from(exp);
    if point > 0 {
        let point = point as usize;
        f.write_str(&c[..point])?;
        f.write_str(".")?;
        f.write_str(&c[point..])
    } else {
        f.write_str("0.")?;
        for _ in 0..-point {
            f.write_str("0")?;
        }
        f.write_str(c)
    }
}

#[cfg(test)]
mod tests {
    use crate::Decimal;
    use alloc::string::ToString;

    /// Parse an exact numeric string (no context rounding) and render it in
    /// to-engineering notation. Mirrors the `toEng` operation on an operand that
    /// already fits the precision, isolating the formatting rule under test.
    fn eng(s: &str) -> alloc::string::String {
        Decimal::parse_str(s)
            .expect("valid literal")
            .to_eng_string()
    }

    #[test]
    fn nonzero_exponential_multiple_of_three() {
        // Shown exponent is a multiple of three; one to three integer digits.
        assert_eq!(eng("10e12"), "10E+12");
        assert_eq!(eng("10e10"), "100E+9");
        assert_eq!(eng("10e9"), "10E+9");
        assert_eq!(eng("10e8"), "1.0E+9");
        assert_eq!(eng("7E11"), "700E+9");
        assert_eq!(eng("7E10"), "70E+9");
        assert_eq!(eng("7E9"), "7E+9");
        // Negative exponents below the plain range.
        assert_eq!(eng("10e-8"), "100E-9");
        assert_eq!(eng("10e-9"), "10E-9");
        assert_eq!(eng("10e-10"), "1.0E-9");
        assert_eq!(eng("7E-7"), "700E-9");
        assert_eq!(eng("7E-13"), "700E-15");
    }

    #[test]
    fn shown_exponent_zero_is_omitted() {
        // A small positive exponent reduces to a shown exponent of zero, which
        // the engineering form drops, padding the integer part instead.
        assert_eq!(eng("10e1"), "100");
        assert_eq!(eng("7E2"), "700");
        assert_eq!(eng("7E1"), "70");
    }

    #[test]
    fn plain_range_matches_scientific() {
        // exp <= 0 && adjexp >= -6: identical to to-scientific plain form.
        assert_eq!(eng("10e-2"), "0.10");
        assert_eq!(eng("10e-7"), "0.0000010");
        assert_eq!(eng("100"), "100");
        assert_eq!(eng("1000.0"), "1000.0");
        assert_eq!(eng("999.9"), "999.9");
        assert_eq!(eng("7E-1"), "0.7");
    }

    #[test]
    fn zero_special_case() {
        // Zero coefficient: shown exponent is `exp` rounded *up* to a multiple
        // of three, the gap rendered as fractional zeros. Plain still wins for
        // exp in [-6, 0].
        assert_eq!(eng("0e+1"), "0.00E+3");
        assert_eq!(eng("0.000000000"), "0E-9"); // e-9
        assert_eq!(eng("0.00000000"), "0.00E-6"); // e-8
        assert_eq!(eng("0.0000000"), "0.0E-6"); // e-7
        assert_eq!(eng("0.000000"), "0.000000"); // e-6, plain
        assert_eq!(eng("0.0"), "0.0");
        assert_eq!(eng("0E+9"), "0E+9");
        assert_eq!(eng("0E+4"), "0.00E+6");
        assert_eq!(eng("0E+0"), "0"); // exp 0 is plain
    }

    #[test]
    fn signed_zero_keeps_sign() {
        assert_eq!(eng("-0.0000000"), "-0.0E-6"); // e-7
        assert_eq!(eng("-0.000000000"), "-0E-9"); // e-9
        assert_eq!(eng("-0.0"), "-0.0");
        assert_eq!(eng("-0."), "-0");
    }

    #[test]
    fn specials_render_like_scientific() {
        assert_eq!(eng("NaN"), "NaN");
        assert_eq!(eng("-NaN"), "-NaN");
        assert_eq!(eng("sNaN"), "sNaN");
        assert_eq!(eng("-sNaN"), "-sNaN");
        assert_eq!(eng("Infinity"), "Infinity");
        assert_eq!(eng("-Infinity"), "-Infinity");
        assert_eq!(eng("NaN123"), "NaN123");
    }

    #[test]
    fn diverges_from_scientific() {
        // The same value renders differently in the two forms.
        let d = Decimal::parse_str("7E-7").expect("valid");
        assert_eq!(d.to_string(), "7E-7"); // to-scientific
        assert_eq!(d.to_eng_string(), "700E-9"); // to-engineering
    }
}
