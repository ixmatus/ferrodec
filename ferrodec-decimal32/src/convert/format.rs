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
//! Format precision is honoured: `{:.N}` renders a fixed-point value
//! with exactly `N` fractional digits (the value is quantized to that
//! width, then padded), and `{:.Ne}` / `{:.NE}` give the scientific
//! mantissa `N` fractional digits. This mirrors the `ferrodec`
//! (Decimal128) parent's precision handling.

use core::fmt;

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

/// Lower bound on the scale (`digits + unbiased_exp`) at which the
/// [`FixedPreferred`] rule emits plain notation. Mirrors the 1.x
/// parent `Decimal128` `Display` boundary so the adapter renders
/// uniformly across the family.
const FIXED_LOWER_LOG10: i32 = -6;

/// Strict upper bound on the scale at which the [`FixedPreferred`]
/// rule emits plain notation. Above this bound the rule falls through
/// to scientific.
const FIXED_UPPER_LOG10: i32 = 21;

/// Notation choice routed through the formatters.
#[derive(Clone, Copy)]
enum Notation {
    /// Default `Display`: plain decimal vs scientific by GDA `toSci`.
    Auto,
    /// Forced scientific with the given exponent letter.
    ScientificForced(char),
    /// Forced engineering notation (exponent multiple of 3) with the
    /// given exponent letter.
    Engineering(char),
    /// The 1.x `Decimal128::Display` rule applied to a `Decimal32`
    /// value (plain when the scale fits `(-6, 21]` with non-positive
    /// quantum, or non-negative quantum with scale `≤ 21`; otherwise
    /// scientific). Routed through [`FixedPreferred`]. Additive in
    /// 2.0 per ADR-0014.
    FixedPreferred,
}

/// Default `Display` for `Decimal32`.
///
/// Uses the General Decimal Arithmetic `toSci` rule: plain notation
/// when `unbiased_exp ≤ 0 && adjusted_exp ≥ -6`, otherwise
/// scientific notation. The cohort the value was typed with is
/// preserved (`"1E+3"` displays as `"1E+3"`, not `"1000"`). Matches
/// the parent `ferrodec::Decimal128::Display` (harmonized onto the
/// same rule in 2.0; the 1.x parent rendering is available as
/// [`Decimal32::fixed_preferred`]). ADR-0014 records the rationale;
/// ADR-0029 item 3 records the 2.0 harmonization.
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

    /// Wrap `self` in a [`FixedPreferred`] adapter that formats using
    /// the 1.x parent `Decimal128::Display` rule (plain notation when
    /// the scale fits `(-6, 21]` and the unbiased exponent is
    /// non-positive, or when the unbiased exponent is non-negative
    /// and the scale stays within `≤ 21`; otherwise scientific).
    ///
    /// The default `Decimal32::Display` impl follows GDA `toSci` and
    /// preserves the cohort the value was typed with; this adapter
    /// renders `1E+3` as `"1000"` instead. Added in 2.0 alongside the
    /// parent's harmonization onto `toSci` so callers can opt into
    /// the legacy preference across every format (ADR-0014, ADR-0029
    /// item 3).
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal32::{Decimal32, RoundingMode};
    ///
    /// let (x, _) = Decimal32::parse_str("1E+3", RoundingMode::NearestEven).unwrap();
    /// // Default toSci preserves the cohort:
    /// assert_eq!(format!("{x}"), "1E+3");
    /// // FixedPreferred prefers integer rendering when the scale fits:
    /// assert_eq!(format!("{}", x.fixed_preferred()), "1000");
    /// ```
    #[must_use]
    pub fn fixed_preferred(self) -> FixedPreferred {
        FixedPreferred(self)
    }
}

/// Wrapper that displays a `Decimal32` using the 1.x parent
/// `Decimal128::Display` rule (plain notation preferred when the
/// scale fits `(-6, 21]`).
///
/// Returned by [`Decimal32::fixed_preferred`]. Added in 2.0 as the
/// cross-format companion to the parent's `FixedPreferred` (ADR-0014).
#[derive(Clone, Copy)]
pub struct FixedPreferred(Decimal32);

impl fmt::Display for FixedPreferred {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_to(self.0, f, Notation::FixedPreferred)
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

/// Quantize `d` to `precision` digits after the decimal point. Used by
/// `Display` (`{:.N}`) where the precision is the fractional width.
fn quantize_to_fixed_precision(d: Decimal32, precision: usize) -> Decimal32 {
    if !d.is_finite() {
        return d;
    }
    quantize_to_target_quantum(d, -(precision as i32))
}

/// Quantize `d` so that scientific notation with `(precision + 1)`
/// significant digits in the mantissa round-trips correctly. Used by
/// `LowerExp` / `UpperExp` (`{:.Ne}`).
fn quantize_to_scientific_precision(d: Decimal32, precision: usize) -> Decimal32 {
    if !d.is_finite() || d.is_zero() {
        return d;
    }
    // The mantissa's leading digit sits at `10^(scale - 1)` where
    // `scale = digit_count + unbiased`. The target quantum gives
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

fn quantize_to_target_quantum(d: Decimal32, target_quantum: i32) -> Decimal32 {
    let target = match Decimal32::try_new(1, target_quantum) {
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

fn format_to(d: Decimal32, f: &mut fmt::Formatter<'_>, notation: Notation) -> fmt::Result {
    let precision = f.precision();
    // Adjust the value to honour `{:.N}` precision before formatting.
    // Specials (NaN/Inf) ignore precision per the f64 convention.
    let d = match (notation, precision) {
        (_, None) => d,
        // Auto and FixedPreferred both render their precision-padded
        // shape from the fractional buffer, so quantize to fractional
        // width once and let the writers pad the rest.
        (Notation::Auto | Notation::FixedPreferred, Some(p)) => quantize_to_fixed_precision(d, p),
        (Notation::ScientificForced(_) | Notation::Engineering(_), Some(p)) => {
            quantize_to_scientific_precision(d, p)
        }
    };
    match classify_bits(d.to_bits()) {
        Class::QuietNaN { sign, payload } => format_nan(f, sign, payload, false),
        Class::SignalingNaN { sign, payload } => format_nan(f, sign, payload, true),
        Class::Infinity { sign } => {
            if sign {
                f.write_str("-")?;
            }
            f.write_str("Infinity")
        }
        Class::Zero { sign, biased_exp } => format_finite(
            f,
            sign,
            0,
            biased_exp as i32 - BIAS as i32,
            1,
            notation,
            precision,
        ),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            let digits = decimal_digit_count(coefficient);
            format_finite(f, sign, coefficient, unbiased, digits, notation, precision)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn format_finite(
    f: &mut fmt::Formatter<'_>,
    sign: bool,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    notation: Notation,
    precision: Option<usize>,
) -> fmt::Result {
    if sign {
        f.write_str("-")?;
    }
    let adjusted_exp = unbiased_exp + (digits as i32) - 1;
    let scale = unbiased_exp + digits as i32;

    match notation {
        Notation::ScientificForced(letter) => {
            return write_scientific(
                f,
                coef,
                unbiased_exp,
                digits,
                adjusted_exp,
                letter,
                precision,
            );
        }
        Notation::Engineering(letter) => {
            return write_engineering(
                f,
                coef,
                unbiased_exp,
                digits,
                adjusted_exp,
                letter,
                precision,
            );
        }
        Notation::Auto | Notation::FixedPreferred => {}
    }

    // `{:.N}` always renders fixed (the precision pin is fractional
    // width), regardless of the underlying notation rule.
    if precision.is_some() {
        return write_plain(f, coef, unbiased_exp, digits, precision);
    }

    match notation {
        Notation::Auto => {
            // toSci rule: plain notation when exp ≤ 0 and adjusted ≥ -6.
            if unbiased_exp <= 0 && adjusted_exp >= -6 {
                write_plain(f, coef, unbiased_exp, digits, precision)
            } else {
                write_scientific(f, coef, unbiased_exp, digits, adjusted_exp, 'E', precision)
            }
        }
        Notation::FixedPreferred => {
            // The 1.x parent `Decimal128::Display` rule applied to a
            // Decimal32. Plain when scale fits `(-6, 21]` and quantum
            // is non-positive, OR quantum is non-negative and scale
            // stays within `≤ 21`. Otherwise scientific.
            if (scale > FIXED_LOWER_LOG10 && scale <= FIXED_UPPER_LOG10 && unbiased_exp <= 0)
                || (unbiased_exp >= 0 && scale <= FIXED_UPPER_LOG10)
            {
                write_plain(f, coef, unbiased_exp, digits, precision)
            } else {
                write_scientific(f, coef, unbiased_exp, digits, adjusted_exp, 'E', precision)
            }
        }
        Notation::ScientificForced(_) | Notation::Engineering(_) => unreachable!(),
    }
}

/// Write the digit string for `coef` (leading zeros padded out to
/// `digits` total) into a 8-byte stack buffer, returning a `&[u8]`
/// with the digits as ASCII.
///
/// L3: every written byte is `b'0' + (n % 10)`, so `b'0'..=b'9'`.
/// Each caller slices `&buf[..digits]` and reads only written bytes,
/// so the `core::str::from_utf8(...).unwrap()` on those slices is
/// total. `digits` is `decimal_digit_count` of a decoded coefficient
/// (at most 7 for canonical Decimal32; non canonical Form B
/// canonicalises to zero at decode), so the `total <= buf.len()`
/// invariant holds and the debug assertion never fires on a decoded
/// value.
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

/// Plain fixed-point output. When `precision = Some(p)` the value has
/// already been quantized to `p` fractional digits by the caller; the
/// renderer pads to exactly `p` fractional digits (adding a decimal
/// point for an integer) if the natural rendering came out short.
fn write_plain(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    precision: Option<usize>,
) -> fmt::Result {
    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];
    let target_frac = precision.map(|p| p as i32);

    if unbiased_exp >= 0 {
        // Pure integer with optional trailing zeros from the quantum.
        f.write_str(core::str::from_utf8(digit_slice).unwrap())?;
        for _ in 0..unbiased_exp {
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
    // unbiased_exp < 0.
    let frac_len = (-unbiased_exp) as u32;
    let render_frac = target_frac.map_or(frac_len as i32, |t| t.max(frac_len as i32));
    if frac_len < digits {
        // Decimal point sits inside the digit run: split at digits - frac_len.
        let split = (digits - frac_len) as usize;
        f.write_str(core::str::from_utf8(&digit_slice[..split]).unwrap())?;
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[split..]).unwrap())?;
        for _ in (frac_len as i32)..render_frac {
            f.write_str("0")?;
        }
        Ok(())
    } else {
        // Need leading zeros: "0.0...0digits".
        f.write_str("0.")?;
        for _ in digits..frac_len {
            f.write_str("0")?;
        }
        f.write_str(core::str::from_utf8(digit_slice).unwrap())?;
        for _ in (frac_len as i32)..render_frac {
            f.write_str("0")?;
        }
        Ok(())
    }
}

/// Scientific output. When `precision = Some(p)` the mantissa is padded
/// to `p` fractional digits (the caller already quantized to give a
/// `(p + 1)`-significant-digit coefficient in the typical case).
fn write_scientific(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    adjusted_exp: i32,
    letter: char,
    precision: Option<usize>,
) -> fmt::Result {
    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];

    let mantissa_frac_natural = (digits as i32) - 1;
    let target_frac = precision.map_or(mantissa_frac_natural, |p| p as i32);

    f.write_str(core::str::from_utf8(&digit_slice[..1]).unwrap())?;
    if mantissa_frac_natural > 0 || target_frac > 0 {
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[1..]).unwrap())?;
        for _ in mantissa_frac_natural..target_frac {
            f.write_str("0")?;
        }
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

#[allow(clippy::too_many_arguments)]
fn write_engineering(
    f: &mut fmt::Formatter<'_>,
    coef: u32,
    unbiased_exp: i32,
    digits: u32,
    adjusted_exp: i32,
    letter: char,
    precision: Option<usize>,
) -> fmt::Result {
    // L1 (Phase 1 finding A5-F7): a zero coefficient has no
    // significant digit to rebase, so the engineering mantissa shift
    // below would pad the single `0` with extra integer zeros
    // (`00E…`, `000E…`), which is never a valid rendering. Zero takes
    // the same shape as `write_scientific`'s zero path: a lone `0`
    // with the non rebased adjusted exponent, so zero is consistent
    // between scientific and engineering. The GDA `to-engineering`
    // fractional zero form (`0.00E+k`) is a documented
    // simplification, not exercised by the vendored conformance
    // corpus. This mirrors the Decimal64 L13 fix.
    if coef == 0 {
        f.write_str("0")?;
        write!(f, "{letter}")?;
        return if adjusted_exp >= 0 {
            write!(f, "+{adjusted_exp}")
        } else {
            write!(f, "{adjusted_exp}")
        };
    }

    // Engineering: rebase the adjusted exponent down to the nearest
    // multiple of 3 (toward -∞), shifting the mantissa right by the
    // remainder.
    let shift = adjusted_exp.rem_euclid(3);
    let target_adjusted = adjusted_exp - shift;

    let buf = digit_string(coef, digits);
    let digit_slice = &buf[..digits as usize];

    let mantissa_int_digits = (shift + 1) as usize;
    let frac_natural = digit_slice.len().saturating_sub(mantissa_int_digits);
    let target_frac = precision.unwrap_or(frac_natural);
    if mantissa_int_digits >= digit_slice.len() {
        // All digits go before the decimal point; if there are fewer
        // digits than mantissa_int_digits (rare for Decimal32), pad
        // with trailing zeros.
        f.write_str(core::str::from_utf8(digit_slice).unwrap())?;
        for _ in digit_slice.len()..mantissa_int_digits {
            f.write_str("0")?;
        }
        if target_frac > 0 {
            f.write_str(".")?;
            for _ in 0..target_frac {
                f.write_str("0")?;
            }
        }
    } else {
        f.write_str(core::str::from_utf8(&digit_slice[..mantissa_int_digits]).unwrap())?;
        f.write_str(".")?;
        f.write_str(core::str::from_utf8(&digit_slice[mantissa_int_digits..]).unwrap())?;
        for _ in frac_natural..target_frac {
            f.write_str("0")?;
        }
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
    use ferrodec_ieee::RoundingMode;

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
    fn engineering_zero_is_a_lone_digit() {
        // L1 (A5-F7): a zero coefficient had its single `0` padded
        // with positional zeros (`0E+5` rendered `000E+3`). It now
        // takes the scientific zero shape: a lone `0` at the non
        // rebased adjusted exponent.
        assert_eq!(format!("{}", parse("0E+5").engineering()), "0E+5");
        assert_eq!(format!("{}", parse("-0E+5").engineering()), "-0E+5");
        assert_eq!(format!("{}", parse("0E-7").engineering()), "0E-7");
        assert_eq!(format!("{}", parse("0").engineering()), "0E+0");
        // The non zero engineering path is unaffected.
        assert_eq!(
            format!("{}", Decimal32::try_new(12345, 0).unwrap().engineering()),
            "12.345E+3"
        );
    }

    #[test]
    fn parse_format_roundtrip_simple() {
        for s in ["1", "-1", "10", "1.0", "0.5", "0.001", "100", "1.234E+10"] {
            let d = parse(s);
            assert_eq!(d.to_string(), s, "round-trip failed for {s}");
        }
    }

    #[test]
    fn fixed_preferred_basic() {
        // Default `Display` keeps the cohort (toSci); `fixed_preferred`
        // applies the 1.x parent `Decimal128::Display` rule and
        // prefers integer rendering when the scale fits `(-6, 21]`.
        assert_eq!(format!("{}", parse("1E+3")), "1E+3");
        assert_eq!(format!("{}", parse("1E+3").fixed_preferred()), "1000");
        assert_eq!(format!("{}", parse("100").fixed_preferred()), "100");
        assert_eq!(format!("{}", parse("0.001").fixed_preferred()), "0.001");
        assert_eq!(format!("{}", parse("1E-7").fixed_preferred()), "1E-7");
        // 1.234E+10 fits the 1.x rule (scale 11 ≤ 21) and renders as
        // fixed; toSci renders the same input as scientific.
        assert_eq!(
            format!("{}", parse("1.234E+10").fixed_preferred()),
            "12340000000"
        );
        // Zero cohorts use the legacy `"0E±N"` rendering when quantum
        // is non-zero; canonical zero stays `"0"`.
        assert_eq!(format!("{}", Decimal32::ZERO.fixed_preferred()), "0");
        // Specials pass through identically to the default.
        assert_eq!(
            format!("{}", Decimal32::INFINITY.fixed_preferred()),
            "Infinity"
        );
    }

    #[test]
    fn display_precision_pads_integer() {
        assert_eq!(format!("{:.3}", parse("1")), "1.000");
        assert_eq!(format!("{:.0}", parse("3")), "3");
        assert_eq!(format!("{:.2}", parse("10")), "10.00");
    }

    #[test]
    fn display_precision_rounds_fractional() {
        assert_eq!(format!("{:.2}", parse("3.14159")), "3.14");
        assert_eq!(format!("{:.4}", parse("3.14159")), "3.1416");
        assert_eq!(format!("{:.0}", parse("3.7")), "4");
        // Round-half-to-even on a 5 boundary.
        assert_eq!(format!("{:.0}", parse("2.5")), "2");
        assert_eq!(format!("{:.0}", parse("3.5")), "4");
    }

    #[test]
    fn display_precision_pads_short_fractional() {
        assert_eq!(format!("{:.5}", parse("0.1")), "0.10000");
        assert_eq!(format!("{:.4}", parse("100.5")), "100.5000");
    }

    #[test]
    fn display_precision_zero_and_specials() {
        assert_eq!(format!("{:.2}", Decimal32::ZERO), "0.00");
        assert_eq!(format!("{:.0}", Decimal32::ZERO), "0");
        // Specials ignore precision, matching the f64 convention.
        assert_eq!(format!("{:.3}", Decimal32::NAN), "NaN");
        assert_eq!(format!("{:.3}", Decimal32::INFINITY), "Infinity");
        assert_eq!(format!("{:.3}", Decimal32::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn scientific_precision() {
        // Mantissa gets exactly N fractional digits.
        assert_eq!(format!("{:.2e}", parse("12345")), "1.23e+4");
        assert_eq!(format!("{:.3e}", parse("12345")), "1.234e+4");
        // Pad with zeros when the natural mantissa is shorter.
        assert_eq!(format!("{:.4e}", parse("1")), "1.0000e+0");
        assert_eq!(format!("{:.0e}", parse("0.000123")), "1e-4");
        // Upper-exp variant.
        assert_eq!(format!("{:.3E}", parse("12345")), "1.234E+4");
        // Zero with scientific precision.
        assert_eq!(format!("{:.2e}", Decimal32::ZERO), "0.00e+0");
    }
}
