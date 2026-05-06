//! [`Decimal128`] → string formatting via `core::fmt::Write`. Alloc-free.
//!
//! `Display` produces the shortest round-trippable representation that
//! either reads as a fixed-point or scientific decimal:
//!
//! * NaN → `NaN` (or `sNaN`).
//! * `±∞` → `Infinity` / `-Infinity`.
//! * Finite, in the "comfortable" range `1e-6 ≤ |x| < 1e21`: fixed
//!   notation, e.g. `0.001`, `42`, `12345.6789`. The 1e-6 lower bound
//!   matches `f64::Display` in `std`.
//! * Otherwise: scientific, e.g. `1.234E-100`, `9.99E+6144`.
//!
//! Format specifiers honour the standard Rust conventions:
//!
//! * `{:.N}` — render with `N` digits after the decimal point. Rounds
//!   via [`Decimal128::quantize`] at [`RoundingMode::NearestEven`];
//!   pads with trailing zeros when `N` exceeds the natural width.
//! * `{:e}` and `{:E}` — force scientific notation with lowercase or
//!   uppercase exponent character respectively. Implemented as
//!   `LowerExp` / `UpperExp` impls; combine with `.N` precision.
//!
//! The fixed-buffer scratch is at most 50 bytes (sign + 34 digits +
//! decimal point + scientific tail), comfortably within stack
//! constraints on Cortex-M0+.
//!
//! Trailing zeros from the decimal coefficient are preserved when they
//! reflect the value's quantum (e.g. `1.00` parses as
//! `100 × 10^-2` and prints back as `1.00`). Pure-integer values with
//! quantum `0` print without a decimal point.

use core::fmt::{self, Write as _};

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal128;
use crate::status::RoundingMode;

const MAX_DIGITS: usize = 34;

/// Lower bound on `|x|` for fixed-notation output: anything below this
/// uses scientific notation. Matches the `f64::Display` convention.
const FIXED_LOWER_LOG10: i32 = -6;

/// Strict upper bound on `|x|` for fixed-notation output.
const FIXED_UPPER_LOG10: i32 = 21;

/// Notation choice routed through the formatters.
#[derive(Clone, Copy)]
enum Notation {
    /// Default `Display`: pick fixed vs scientific by magnitude.
    Auto,
    /// Forced scientific with the given exponent letter (`'e'` or `'E'`).
    ScientificForced(char),
    /// Forced engineering notation (exponent multiple of 3) with the
    /// given exponent letter.
    Engineering(char),
}

impl fmt::Display for Decimal128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::Auto)
    }
}

impl fmt::LowerExp for Decimal128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::ScientificForced('e'))
    }
}

impl fmt::UpperExp for Decimal128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(*self, f, Notation::ScientificForced('E'))
    }
}

/// Wrapper that displays a `Decimal128` in *engineering* notation:
/// scientific with the exponent forced to a multiple of 3, so the
/// mantissa lies in `[1, 1000)`. Useful for finance and SI-scaled
/// scientific output.
///
/// Returned by [`Decimal128::engineering`]; implements `Display` so
/// callers can write `format!("{}", x.engineering())` or
/// `let s = x.engineering().to_string()`.
#[derive(Clone, Copy)]
pub struct Engineering(Decimal128);

impl fmt::Display for Engineering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(self.0, f, Notation::Engineering('E'))
    }
}

impl Decimal128 {
    /// Wrap `self` in a [`Engineering`] adapter that formats in
    /// engineering notation (scientific with exponent a multiple of 3,
    /// mantissa in `[1, 1000)`).
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // 12345 as engineering: 12.345 × 10^3
    /// let s = format!("{}", Decimal128::try_new(12345, 0).unwrap().engineering());
    /// assert_eq!(s, "12.345E+3");
    ///
    /// // 0.0001234 as engineering: 123.4 × 10^-6
    /// let s = format!("{}", Decimal128::try_new(1234, -7).unwrap().engineering());
    /// assert_eq!(s, "123.4E-6");
    /// ```
    #[must_use]
    pub fn engineering(self) -> Engineering {
        Engineering(self)
    }
}

/// Quantize `d` to `precision` digits after the decimal point. Used by
/// `Display` (`{:.N}`) where the precision is the fractional width.
fn quantize_to_fixed_precision(d: Decimal128, precision: usize) -> Decimal128 {
    if !d.is_finite() {
        return d;
    }
    quantize_to_target_quantum(d, -(precision as i32))
}

/// Quantize `d` so that scientific notation with `(precision + 1)`
/// significant digits in the mantissa round-trips correctly. Used by
/// `LowerExp` / `UpperExp` (`{:.Ne}`).
fn quantize_to_scientific_precision(d: Decimal128, precision: usize) -> Decimal128 {
    if !d.is_finite() || d.is_zero() {
        return d;
    }
    // The mantissa's leading digit sits at `10^(scale - 1)` where
    // `scale = digit_count + unbiased`. Target quantum gives
    // `(precision + 1)` digits in the coefficient.
    let (digits, unbiased) = match classify_bits(d.to_bits()) {
        Class::Finite {
            biased_exp,
            coefficient,
            ..
        } => (
            decimal_digit_count(coefficient) as i32,
            biased_exp as i32 - BIAS as i32,
        ),
        _ => return d,
    };
    let scale = digits + unbiased;
    let target_quantum = scale - 1 - precision as i32;
    quantize_to_target_quantum(d, target_quantum)
}

fn quantize_to_target_quantum(d: Decimal128, target_quantum: i32) -> Decimal128 {
    let target = match Decimal128::try_new(1, target_quantum) {
        Ok(t) => t,
        Err(_) => return d,
    };
    let (q, _) = d.quantize(target, RoundingMode::NearestEven);
    if q.is_nan() {
        d
    } else {
        q
    }
}

fn format_to(d: Decimal128, f: &mut fmt::Formatter<'_>, notation: Notation) -> fmt::Result {
    let precision = f.precision();
    // Adjust the value to honour `{:.N}` precision before formatting.
    // Specials (NaN/Inf) ignore precision per the f64 convention.
    let d = match (notation, precision) {
        (_, None) => d,
        (Notation::Auto, Some(p)) => quantize_to_fixed_precision(d, p),
        (Notation::ScientificForced(_) | Notation::Engineering(_), Some(p)) => {
            quantize_to_scientific_precision(d, p)
        }
    };
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
        Class::Zero { sign, biased_exp } => format_zero(sign, biased_exp, f, notation, precision),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => format_finite(sign, biased_exp, coefficient, f, notation, precision),
    }
}

fn format_zero(
    sign: bool,
    biased_exp: u32,
    f: &mut fmt::Formatter<'_>,
    notation: Notation,
    precision: Option<usize>,
) -> fmt::Result {
    let unbiased = biased_exp as i32 - BIAS as i32;
    if sign {
        f.write_str("-")?;
    }
    match notation {
        Notation::ScientificForced(exp_char) => {
            f.write_str("0")?;
            if let Some(p) = precision {
                if p > 0 {
                    f.write_str(".")?;
                    for _ in 0..p {
                        f.write_str("0")?;
                    }
                }
            }
            f.write_char(exp_char)?;
            write_signed_int(0, f)
        }
        Notation::Engineering(exp_char) => {
            f.write_str("0")?;
            if let Some(p) = precision {
                if p > 0 {
                    f.write_str(".")?;
                    for _ in 0..p {
                        f.write_str("0")?;
                    }
                }
            }
            f.write_char(exp_char)?;
            write_signed_int(0, f)
        }
        Notation::Auto => {
            if let Some(p) = precision {
                // `{:.N}` on a zero — render as 0.000... with N zeros.
                f.write_str("0")?;
                if p > 0 {
                    f.write_str(".")?;
                    for _ in 0..p {
                        f.write_str("0")?;
                    }
                }
                Ok(())
            } else if unbiased == 0 {
                f.write_str("0")
            } else {
                f.write_str("0E")?;
                write_signed_int(unbiased, f)
            }
        }
    }
}

fn format_finite(
    sign: bool,
    biased_exp: u32,
    coefficient: u128,
    f: &mut fmt::Formatter<'_>,
    notation: Notation,
    precision: Option<usize>,
) -> fmt::Result {
    let unbiased = biased_exp as i32 - BIAS as i32;
    let digits = decimal_digit_count(coefficient) as i32;

    // The decimal "scale" — value's order of magnitude (so 12.34 has scale 2).
    let scale = digits + unbiased;

    if sign {
        f.write_str("-")?;
    }

    match notation {
        Notation::ScientificForced(exp_char) => {
            return format_scientific(coefficient, digits, unbiased, f, exp_char, precision);
        }
        Notation::Engineering(exp_char) => {
            return format_engineering_into(coefficient, digits, unbiased, f, exp_char, precision);
        }
        Notation::Auto => {}
    }

    // When `{:.N}` is set with default Notation::Auto, render as fixed
    // (which honours the precision-padded fractional width directly).
    if precision.is_some() {
        return format_fixed(coefficient, digits, unbiased, f, precision);
    }

    // Decide fixed vs scientific.
    if scale > FIXED_LOWER_LOG10 && scale <= FIXED_UPPER_LOG10 && unbiased <= 0 {
        return format_fixed(coefficient, digits, unbiased, f, precision);
    }
    if unbiased >= 0 && scale <= FIXED_UPPER_LOG10 {
        return format_fixed(coefficient, digits, unbiased, f, precision);
    }
    format_scientific(coefficient, digits, unbiased, f, 'E', precision)
}

/// Fixed-point output. `unbiased` may be negative (fractional part) or
/// non-negative (trailing zeros). When `precision` is `Some(p)`, the
/// caller has already quantized to that fractional width; we just pad
/// to exactly `p` fractional digits if the natural rendering came out
/// short (e.g. integer input).
fn format_fixed(
    coefficient: u128,
    digits: i32,
    unbiased: i32,
    f: &mut fmt::Formatter<'_>,
    precision: Option<usize>,
) -> fmt::Result {
    let mut buf = [0u8; MAX_DIGITS];
    let written = write_digits(coefficient, &mut buf);
    let coef_digits = &buf[..written];

    let target_frac = precision.map(|p| p as i32);

    if unbiased >= 0 {
        // Pure integer with optional trailing zeros from the quantum.
        f.write_str(core::str::from_utf8(coef_digits).expect("ASCII digits are valid UTF-8"))?;
        for _ in 0..unbiased {
            f.write_str("0")?;
        }
        if let Some(t) = target_frac {
            if t > 0 {
                f.write_str(".")?;
                for _ in 0..t {
                    f.write_str("0")?;
                }
            }
        }
        return Ok(());
    }
    // Fractional. `unbiased < 0`, so |unbiased| digits sit after the point.
    let frac = -unbiased;
    let render_frac = target_frac.unwrap_or(frac).max(frac);
    if frac >= digits {
        f.write_str("0.")?;
        for _ in 0..(frac - digits) {
            f.write_str("0")?;
        }
        f.write_str(core::str::from_utf8(coef_digits).expect("ASCII digits are valid UTF-8"))?;
        for _ in frac..render_frac {
            f.write_str("0")?;
        }
        return Ok(());
    }
    let int_len = (digits - frac) as usize;
    let int_part = &coef_digits[..int_len];
    let frac_part = &coef_digits[int_len..];
    f.write_str(core::str::from_utf8(int_part).expect("ASCII"))?;
    f.write_str(".")?;
    f.write_str(core::str::from_utf8(frac_part).expect("ASCII"))?;
    for _ in frac..render_frac {
        f.write_str("0")?;
    }
    Ok(())
}

/// Scientific notation: one integer digit, optional fractional, then
/// `e±exp` or `E±exp` per `exp_char`. When `precision = Some(p)`, the
/// caller has already quantized to give a `(p + 1)`-sig-fig mantissa
/// in the typical case; the rendering pads to `p` fractional digits
/// in the mantissa if needed.
fn format_scientific(
    coefficient: u128,
    digits: i32,
    unbiased: i32,
    f: &mut fmt::Formatter<'_>,
    exp_char: char,
    precision: Option<usize>,
) -> fmt::Result {
    let mut buf = [0u8; MAX_DIGITS];
    let written = write_digits(coefficient, &mut buf);
    let coef_digits = &buf[..written];

    let mantissa_frac_natural = (coef_digits.len() - 1) as i32;
    let target_frac = precision.map_or(mantissa_frac_natural, |p| p as i32);

    f.write_str(core::str::from_utf8(&coef_digits[..1]).expect("ASCII"))?;
    if mantissa_frac_natural > 0 || target_frac > 0 {
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&coef_digits[1..]).expect("ASCII"))?;
        for _ in mantissa_frac_natural..target_frac {
            f.write_str("0")?;
        }
    }
    f.write_char(exp_char)?;
    let adjusted = digits + unbiased - 1;
    write_signed_int(adjusted, f)
}

/// Engineering notation: scientific with the exponent forced to a
/// multiple of 3, so the mantissa lies in `[1, 1000)`. Used by
/// `Decimal128::to_engineering_string`.
fn format_engineering_into(
    coefficient: u128,
    digits: i32,
    unbiased: i32,
    f: &mut fmt::Formatter<'_>,
    exp_char: char,
    precision: Option<usize>,
) -> fmt::Result {
    let scientific_exp = digits + unbiased - 1;
    // Round `scientific_exp` *down* to the nearest multiple of 3.
    // For -2 → -3 (so the mantissa shifts up); for 4 → 3 (mantissa
    // shifts up by one decade, into [1, 1000)).
    let eng_exp = scientific_exp.div_euclid(3) * 3;
    let shift = scientific_exp - eng_exp; // 0, 1, or 2

    let mut buf = [0u8; MAX_DIGITS];
    let written = write_digits(coefficient, &mut buf);
    let coef_digits = &buf[..written];
    let int_digits = (shift + 1) as usize;
    let int_digits = int_digits.min(coef_digits.len());

    f.write_str(core::str::from_utf8(&coef_digits[..int_digits]).expect("ASCII"))?;

    let frac_natural = coef_digits.len().saturating_sub(int_digits);
    let target_frac = precision.unwrap_or(frac_natural);
    if frac_natural > 0 || target_frac > 0 {
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&coef_digits[int_digits..]).expect("ASCII"))?;
        for _ in frac_natural..target_frac {
            f.write_str("0")?;
        }
    }
    f.write_char(exp_char)?;
    write_signed_int(eng_exp, f)
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
            let s = format!("{d}");
            let back = parse(&s);
            let (cmp, _) = back.partial_cmp(d);
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "{v} round-trip");
        }
    }

    #[test]
    fn format_precision_pads_integer() {
        assert_eq!(format!("{:.3}", parse("1")), "1.000");
        assert_eq!(format!("{:.0}", parse("3")), "3");
    }

    #[test]
    fn format_precision_rounds_fractional() {
        assert_eq!(format!("{:.2}", parse("3.14159")), "3.14");
        assert_eq!(format!("{:.4}", parse("3.14159")), "3.1416");
        assert_eq!(format!("{:.0}", parse("3.7")), "4");
        // Round-half-to-even on a 5 boundary.
        assert_eq!(format!("{:.0}", parse("2.5")), "2");
        assert_eq!(format!("{:.0}", parse("3.5")), "4");
    }

    #[test]
    fn format_precision_pads_short_fractional() {
        assert_eq!(format!("{:.5}", parse("0.1")), "0.10000");
        assert_eq!(format!("{:.4}", parse("100.5")), "100.5000");
    }

    #[test]
    fn format_lowerexp_default() {
        assert_eq!(format!("{:e}", parse("1.5")), "1.5e+0");
        assert_eq!(format!("{:e}", parse("12345")), "1.2345e+4");
        assert_eq!(format!("{:e}", parse("0.001")), "1e-3");
    }

    #[test]
    fn format_upperexp_default() {
        assert_eq!(format!("{:E}", parse("1.5")), "1.5E+0");
        assert_eq!(format!("{:E}", parse("0.001")), "1E-3");
    }

    #[test]
    fn format_lowerexp_with_precision() {
        // Mantissa has p digits after the decimal in scientific form.
        assert_eq!(format!("{:.2e}", parse("12345")), "1.23e+4");
        // Pad with zeros when natural mantissa is shorter.
        assert_eq!(format!("{:.4e}", parse("1")), "1.0000e+0");
        assert_eq!(format!("{:.0e}", parse("0.000123")), "1e-4");
    }

    #[test]
    fn format_specials_ignore_precision() {
        assert_eq!(format!("{:.3}", Decimal128::NAN), "NaN");
        assert_eq!(format!("{:.3}", Decimal128::INFINITY), "Infinity");
        assert_eq!(format!("{:.3}", Decimal128::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn format_zero_with_precision() {
        assert_eq!(format!("{:.2}", Decimal128::ZERO), "0.00");
        assert_eq!(format!("{:.0}", Decimal128::ZERO), "0");
        assert_eq!(format!("{:.2e}", Decimal128::ZERO), "0.00e+0");
    }

    #[test]
    fn engineering_basics() {
        // Mantissa lives in [1, 1000), exponent is a multiple of 3.
        assert_eq!(
            format!("{}", Decimal128::try_new(12345, 0).unwrap().engineering()),
            "12.345E+3"
        );
        // Negative exponent rounded down to next multiple of 3.
        assert_eq!(
            format!("{}", Decimal128::try_new(1234, -7).unwrap().engineering()),
            "123.4E-6"
        );
        // Exact multiple of 1000.
        assert_eq!(
            format!("{}", Decimal128::try_new(1, 3).unwrap().engineering()),
            "1E+3"
        );
    }

    #[test]
    fn engineering_handles_specials() {
        assert_eq!(format!("{}", Decimal128::NAN.engineering()), "NaN");
        assert_eq!(
            format!("{}", Decimal128::INFINITY.engineering()),
            "Infinity"
        );
        // Engineering always emits the explicit `E±N` exponent.
        assert_eq!(format!("{}", Decimal128::ZERO.engineering()), "0E+0");
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
            let formatted = format!("{d}");
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
