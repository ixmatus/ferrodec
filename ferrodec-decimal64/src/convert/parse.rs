//! `&str` → [`Decimal64`] parser. No allocation, no `std`.
//!
//! Grammar (case-insensitive for `e`/`E` and the special tokens):
//!
//! ```text
//! decimal       := sign? mantissa exponent?
//!                | sign? "Infinity"
//!                | sign? "Inf"
//!                | sign? "NaN"  payload?
//!                | sign? "sNaN" payload?
//! sign          := "+" | "-"
//! mantissa      := digits ("." digits?)?
//!                | "." digits
//! digits        := DIGIT+
//! payload       := DIGIT+        // diagnostic NaN payload, ≤ 15 digits
//! exponent      := ("e" | "E") sign? digits
//! ```
//!
//! NaN payloads encode in the BID significand's trailing 50 bits
//! (`T_MASK`). Payloads up to `2^50 − 1` (`1_125_899_906_842_623`) round-trip;
//! larger values are rejected with `InvalidCharacter` at the first
//! overflowing digit. Canonical NaN payloads are bounded at `< 10^15 = 1_000_000_000_000_000`
//! per IEEE 754-2019 §3.5.2; the parser accepts the wider raw-field
//! range and `canonicalize()` rewrites the rest.
//!
//! Up to 19 mantissa digits are accumulated exactly in a `u64`;
//! trailing digits beyond that contribute to the rounding sticky bit.
//! The rounding direction comes from the supplied [`RoundingMode`].

use crate::bid::{pack_quiet_nan, pack_signaling_nan, T_MASK};
use crate::decimal::Decimal64;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

/// Parse error returned by [`Decimal64::parse_str`].
///
/// Includes a byte index pointing at the offending character (or the
/// end of input for `Empty` and `InvalidExponent`) to make diagnostics
/// usable in calculator UIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseDecimalError {
    /// Empty input or input consisting only of a sign.
    Empty,
    /// Unexpected character at the given byte position.
    InvalidCharacter(usize),
    /// `e`/`E` introducer not followed by valid digits.
    InvalidExponent,
    /// Explicit exponent magnitude exceeds the format's range.
    ExponentOutOfRange,
}

impl core::fmt::Display for ParseDecimalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty decimal literal"),
            Self::InvalidCharacter(pos) => write!(f, "invalid character at byte {pos}"),
            Self::InvalidExponent => f.write_str("malformed exponent in decimal literal"),
            Self::ExponentOutOfRange => f.write_str("exponent magnitude out of range"),
        }
    }
}

impl core::error::Error for ParseDecimalError {}

const MAX_PARSED_DIGITS: u32 = 19;
const MAX_EXPONENT_MAGNITUDE: u32 = 1_000_000;

impl Decimal64 {
    /// Parse a `&str` into a `Decimal64`, rounding per `rm`.
    ///
    /// On success returns `(value, status)`. `status.inexact()` is set
    /// iff the input had more significant digits than the format can
    /// represent at the chosen precision.
    pub fn parse_str(s: &str, rm: RoundingMode) -> Result<(Self, Status), ParseDecimalError> {
        parse_str_inner(s.as_bytes(), rm)
    }
}

/// Idiomatic Rust parsing via the `str::parse` extension method.
///
/// Defaults to [`RoundingMode::NearestEven`] and discards the
/// [`Status`] flags from `parse_str`.
impl core::str::FromStr for Decimal64 {
    type Err = ParseDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_str_inner(s.as_bytes(), RoundingMode::NearestEven).map(|(v, _)| v)
    }
}

fn parse_str_inner(
    bytes: &[u8],
    rm: RoundingMode,
) -> Result<(Decimal64, Status), ParseDecimalError> {
    if bytes.is_empty() {
        return Err(ParseDecimalError::Empty);
    }

    let (sign, mut idx) = match bytes[0] {
        b'+' => (false, 1),
        b'-' => (true, 1),
        _ => (false, 0),
    };

    if idx >= bytes.len() {
        return Err(ParseDecimalError::Empty);
    }

    if let Some(special) = match_special(&bytes[idx..], idx, sign) {
        return special.map(|d| (d, Status::OK));
    }

    let mut coef: u64 = 0;
    let mut digits_total: u32 = 0;
    let mut digits_after_point: u32 = 0;
    let mut extra_int_digits: u32 = 0;
    let mut sticky = false;
    let mut decimal_seen = false;
    let mut has_digit = false;

    while idx < bytes.len() {
        let c = bytes[idx];
        match c {
            b'0'..=b'9' => {
                has_digit = true;
                let d = u64::from(c - b'0');
                if digits_total < MAX_PARSED_DIGITS {
                    let leading_int_zero = coef == 0 && d == 0 && !decimal_seen;
                    let leading_frac_zero = coef == 0 && d == 0 && decimal_seen;
                    if leading_int_zero {
                        // Pure leading zero in the integer part —
                        // ignore.
                    } else if leading_frac_zero {
                        // Leading zero AFTER the decimal point but
                        // BEFORE the first non-zero digit. Shifts the
                        // quantum down by one but does not "spend" a
                        // digit-budget slot — the value's significant
                        // figures haven't started yet.
                        digits_after_point += 1;
                    } else {
                        coef = coef * 10 + d;
                        digits_total += 1;
                        if decimal_seen {
                            digits_after_point += 1;
                        }
                    }
                } else {
                    if d != 0 {
                        sticky = true;
                    }
                    if !decimal_seen {
                        // Trailing-integer digits we cannot fold into
                        // `coef` act as a 10× shift on the value (each
                        // such digit pushes the implicit decimal point
                        // one place further right). Track them so the
                        // final `unbiased_exp` absorbs the shift.
                        extra_int_digits = extra_int_digits.saturating_add(1);
                    }
                    // Sticky-only fractional digits sit *below* the
                    // coefficient's representation precision and feed
                    // rounding only — `digits_after_point` is *not*
                    // incremented, since `unbiased_exp =
                    // -digits_after_point` must reflect the
                    // coefficient's quantum, not the input's full
                    // fractional length.
                }
                idx += 1;
            }
            b'.' => {
                if decimal_seen {
                    return Err(ParseDecimalError::InvalidCharacter(idx));
                }
                decimal_seen = true;
                idx += 1;
            }
            b'e' | b'E' => break,
            _ => return Err(ParseDecimalError::InvalidCharacter(idx)),
        }
    }

    if !has_digit {
        return Err(ParseDecimalError::Empty);
    }
    let _ = digits_total; // accumulated for clarity; unbiased_exp uses digits_after_point + extra_int_digits

    let mut exp_explicit: i32 = 0;
    if idx < bytes.len() && (bytes[idx] == b'e' || bytes[idx] == b'E') {
        idx += 1;
        let exp_sign = match bytes.get(idx) {
            Some(&b'+') => {
                idx += 1;
                false
            }
            Some(&b'-') => {
                idx += 1;
                true
            }
            _ => false,
        };
        if idx >= bytes.len() {
            return Err(ParseDecimalError::InvalidExponent);
        }
        let mut exp_val: u32 = 0;
        let mut exp_has_digit = false;
        while idx < bytes.len() {
            let c = bytes[idx];
            if !c.is_ascii_digit() {
                return Err(ParseDecimalError::InvalidCharacter(idx));
            }
            exp_has_digit = true;
            let d = u32::from(c - b'0');
            exp_val = exp_val.saturating_mul(10).saturating_add(d);
            if exp_val > MAX_EXPONENT_MAGNITUDE {
                return Err(ParseDecimalError::ExponentOutOfRange);
            }
            idx += 1;
        }
        if !exp_has_digit {
            return Err(ParseDecimalError::InvalidExponent);
        }
        exp_explicit = if exp_sign {
            -(exp_val as i32)
        } else {
            exp_val as i32
        };
    }

    if idx != bytes.len() {
        return Err(ParseDecimalError::InvalidCharacter(idx));
    }

    let unbiased_exp = exp_explicit
        .saturating_add(extra_int_digits as i32)
        .saturating_sub(digits_after_point as i32);

    let (value, status) = round_and_pack_finite(
        coef,
        unbiased_exp,
        unbiased_exp,
        sign,
        sticky,
        rm,
        Status::OK,
    );
    Ok((value, status))
}

/// Match a special-value token (`Infinity`, `Inf`, `NaN`, `sNaN`) at the
/// start of `rest`. Case-insensitive. Returns:
/// * `None` — the input doesn't start with a special token; fall
///   through to the regular numeric parser.
/// * `Some(Ok(d))` — token matched, returns the constructed value.
/// * `Some(Err(e))` — token matched but the trailing payload is
///   malformed or overflows the 20-bit field.
fn match_special(
    rest: &[u8],
    start_offset: usize,
    sign: bool,
) -> Option<Result<Decimal64, ParseDecimalError>> {
    if eq_ignore_ascii_case(rest, b"infinity") || eq_ignore_ascii_case(rest, b"inf") {
        return Some(Ok(if sign {
            Decimal64::NEG_INFINITY
        } else {
            Decimal64::INFINITY
        }));
    }
    if let Some(payload_bytes) = strip_prefix_ignore_ascii_case(rest, b"nan") {
        return Some(parse_nan_payload(
            payload_bytes,
            start_offset + 3,
            sign,
            false,
        ));
    }
    if let Some(payload_bytes) = strip_prefix_ignore_ascii_case(rest, b"snan") {
        return Some(parse_nan_payload(
            payload_bytes,
            start_offset + 4,
            sign,
            true,
        ));
    }
    None
}

/// Decode the optional `digits*` payload after a `NaN` / `sNaN` token.
/// Empty input → canonical zero-payload NaN. Otherwise: pack the
/// decimal-encoded integer into the BID's 20-bit `T_MASK` field.
fn parse_nan_payload(
    digits: &[u8],
    offset: usize,
    sign: bool,
    signaling: bool,
) -> Result<Decimal64, ParseDecimalError> {
    let mut payload: u64 = 0;
    for (i, &c) in digits.iter().enumerate() {
        if !c.is_ascii_digit() {
            return Err(ParseDecimalError::InvalidCharacter(offset + i));
        }
        let d = u64::from(c - b'0');
        payload = payload
            .checked_mul(10)
            .and_then(|p| p.checked_add(d))
            .ok_or(ParseDecimalError::InvalidCharacter(offset + i))?;
        if payload > T_MASK {
            return Err(ParseDecimalError::InvalidCharacter(offset + i));
        }
    }
    let bits = if signaling {
        pack_signaling_nan(sign, payload)
    } else {
        pack_quiet_nan(sign, payload)
    };
    Ok(Decimal64::from_bits(bits))
}

#[inline]
fn strip_prefix_ignore_ascii_case<'a>(rest: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    if rest.len() < prefix.len() {
        return None;
    }
    let (head, tail) = rest.split_at(prefix.len());
    if eq_ignore_ascii_case(head, prefix) {
        Some(tail)
    } else {
        None
    }
}

#[inline]
fn eq_ignore_ascii_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BIAS};

    fn parse(s: &str) -> Decimal64 {
        Decimal64::parse_str(s, RoundingMode::default())
            .expect("parse")
            .0
    }

    #[test]
    fn parse_zero() {
        let (d, _) = Decimal64::parse_str("0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(!d.is_sign_negative());

        let (d, _) = Decimal64::parse_str("-0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(d.is_sign_negative());
    }

    #[test]
    fn parse_integers() {
        assert_eq!(parse("1").to_bits(), Decimal64::ONE.to_bits());
        assert_eq!(parse("-1").to_bits(), Decimal64::NEG_ONE.to_bits());
        assert_eq!(parse("10").to_bits(), Decimal64::TEN.to_bits());
    }

    #[test]
    fn parse_fixed_with_decimal() {
        // "0.5" = 5 × 10^-1
        let d = parse("0.5");
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 1, 5));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "-1.25" = -125 × 10^-2
        let d = parse("-1.25");
        let expected = Decimal64::from_bits(pack_finite(true, BIAS - 2, 125));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_scientific() {
        // "1e3" = 1 × 10^3
        let d = parse("1e3");
        let expected = Decimal64::from_bits(pack_finite(false, BIAS + 3, 1));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "1.5E-2" = 15 × 10^-3
        let d = parse("1.5E-2");
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 3, 15));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_specials() {
        assert!(parse("NaN").is_nan());
        assert!(parse("nan").is_nan());
        assert!(parse("-NaN").is_nan());
        assert!(parse("sNaN").is_signaling_nan());
        assert!(parse("Infinity").is_infinite());
        assert!(parse("Inf").is_infinite());
        assert!(parse("-Infinity").is_sign_negative());
        assert!(parse("-Inf").is_sign_negative());
    }

    #[test]
    fn parse_nan_payloads() {
        let d = parse("NaN22");
        assert!(d.is_nan() && !d.is_signaling_nan());
        assert_eq!(d.to_bits() & T_MASK, 22);

        let d = parse("-NaN22");
        assert!(d.is_nan() && d.is_sign_negative());
        assert_eq!(d.to_bits() & T_MASK, 22);

        let d = parse("sNaN33");
        assert!(d.is_signaling_nan());
        assert_eq!(d.to_bits() & T_MASK, 33);

        // Larger payloads up to the 50-bit envelope (~ 10^15).
        let big = parse("NaN999999999999999"); // 15 nines, fits in 50 bits.
        assert!(big.is_nan());
        assert_eq!(big.to_bits() & T_MASK, 999_999_999_999_999);

        // Empty payload behaves the same as bare `NaN` / `sNaN`.
        assert_eq!(parse("NaN").to_bits(), parse("NaN0").to_bits());
        assert_eq!(parse("sNaN").to_bits(), parse("sNaN0").to_bits());
    }

    #[test]
    fn parse_nan_payload_overflow() {
        // 16 nines (9_999_999_999_999_999) exceeds the 50-bit T_MASK
        // field (= 2^50 − 1 ≈ 1.13 × 10^15).
        let res =
            Decimal64::parse_str("NaN9999999999999999", RoundingMode::default());
        assert!(matches!(res, Err(ParseDecimalError::InvalidCharacter(_))));
    }

    #[test]
    fn parse_leading_zeros() {
        let d = parse("0007");
        let expected = Decimal64::from_bits(pack_finite(false, BIAS, 7));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_rounding_at_precision_boundary() {
        // 17 digits → must round to 16. NearestEven with last digit 8
        // → round up. 12345678901234568 → 1234567890123457 × 10^1.
        let (d, status) =
            Decimal64::parse_str("12345678901234568", RoundingMode::NearestEven).unwrap();
        let expected =
            Decimal64::from_bits(pack_finite(false, BIAS + 1, 1_234_567_890_123_457));
        assert_eq!(d.to_bits(), expected.to_bits());
        assert!(status.inexact());
    }

    #[test]
    fn parse_invalid() {
        assert!(matches!(
            Decimal64::parse_str("", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal64::parse_str("+", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal64::parse_str("abc", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter(0))
        ));
        assert!(matches!(
            Decimal64::parse_str("1.2.3", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter(_))
        ));
        assert!(matches!(
            Decimal64::parse_str("1e", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent)
        ));
    }

    #[test]
    fn from_str_default_rounding() {
        let d: Decimal64 = "1.23".parse().unwrap();
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 2, 123));
        assert_eq!(d.to_bits(), expected.to_bits());
    }
}
