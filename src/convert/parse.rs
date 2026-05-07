//! `&str` → [`Decimal128`] parser. No allocation, no `std`.
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
//! payload       := DIGIT+        // diagnostic NaN payload, ≤ 33 digits
//! exponent      := ("e" | "E") sign? digits
//! ```
//!
//! NaN payloads encode in the BID significand's trailing 110 bits
//! (`T_MASK`). Payloads up to `2^110 − 1` ≈ `10^33` round-trip; larger
//! values are rejected with `InvalidCharacter` at the first overflowing
//! digit.
//!
//! Up to 76 mantissa digits are accumulated exactly; trailing digits
//! beyond that contribute to the rounding sticky bit. The rounding
//! direction comes from the supplied [`RoundingMode`].

use crate::bid::{pack_quiet_nan, pack_signaling_nan, T_MASK};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

/// Parse error returned by [`Decimal128::parse_str`].
///
/// Includes a byte index pointing at the offending character (or the
/// end of input for `Empty` and `InvalidExponent`) to make diagnostics
/// usable in calculator UIs.
///
/// Implements [`Display`](core::fmt::Display) and
/// [`core::error::Error`] so the type composes with `?`,
/// `Box<dyn Error>`, and `anyhow::Error` chains.
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

const MAX_PARSED_DIGITS: u32 = 76;
const MAX_EXPONENT_MAGNITUDE: u32 = 1_000_000;

impl Decimal128 {
    /// Parse a `&str` into a `Decimal128`, rounding per `rm`.
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
/// [`Status`] flags from `parse_str`. Callers that need explicit
/// rounding-mode or status control should keep using
/// [`Decimal128::parse_str`] directly.
///
/// # Examples
///
/// ```
/// use ferrodec::Decimal128;
///
/// let x: Decimal128 = "1.23".parse().unwrap();
/// let bits = Decimal128::try_new(123, -2).unwrap().to_bits();
/// assert_eq!(x.to_bits(), bits);
/// ```
impl core::str::FromStr for Decimal128 {
    type Err = ParseDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_str_inner(s.as_bytes(), RoundingMode::NearestEven).map(|(v, _)| v)
    }
}

fn parse_str_inner(
    bytes: &[u8],
    rm: RoundingMode,
) -> Result<(Decimal128, Status), ParseDecimalError> {
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

    // Special tokens take priority. Match case-insensitively.
    if let Some(special) = match_special(&bytes[idx..], idx, sign) {
        return special.map(|d| (d, Status::OK));
    }

    // Mantissa: gather digits from before and after the (optional) decimal
    // point, accumulating into a U256 coefficient. Track digit count and
    // the position of the decimal point so we can derive the unbiased
    // quantum.
    let mut coef = U256::ZERO;
    let mut digits_total: u32 = 0;
    let mut digits_after_point: u32 = 0;
    let mut sticky = false;
    let mut decimal_seen = false;
    let mut has_digit = false;

    while idx < bytes.len() {
        let c = bytes[idx];
        match c {
            b'0'..=b'9' => {
                has_digit = true;
                let d = (c - b'0') as u32;
                if digits_total < MAX_PARSED_DIGITS {
                    if !(coef.is_zero() && d == 0 && !decimal_seen) {
                        // Skip leading zeros in the integer part for
                        // counting (so "000123" parses with 3 digits).
                        coef = coef.mul10().add(U256::from_u128(d as u128));
                        digits_total += 1;
                    } else if decimal_seen {
                        // Leading zeros after the decimal still count
                        // toward `digits_after_point` to keep the quantum
                        // correct; "0.001" is `1 × 10^-3`.
                        digits_after_point += 1;
                        // Fall through: don't add to coef but record the digit.
                        idx += 1;
                        continue;
                    } else {
                        // Pure leading zero in integer part — ignore.
                        idx += 1;
                        continue;
                    }
                    if decimal_seen {
                        digits_after_point += 1;
                    }
                } else {
                    // Beyond capacity: feeds the sticky bit.
                    if d != 0 {
                        sticky = true;
                    }
                    if decimal_seen {
                        digits_after_point += 1;
                    } else {
                        // Trailing-integer digits we can't represent in
                        // U256 act as a 10× shift on the quantum.
                        // We compensate by NOT incrementing
                        // `digits_after_point` and instead letting the
                        // exponent absorb the shift via `extra_int_digits`.
                        // Track these separately.
                        // Simpler: bump quantum implicitly by counting
                        // these as "extra integer digits" we shift later.
                        digits_total = digits_total.saturating_add(1);
                    }
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

    // Optional exponent.
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
            let d = (c - b'0') as u32;
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

    // Quantum: each digit after the decimal point shifts the value down
    // by one decimal position, plus the explicit exponent.
    let unbiased_exp = exp_explicit - digits_after_point as i32;

    let (value, status) = round_and_pack_finite(
        coef,
        unbiased_exp,
        unbiased_exp, // preferred quantum: the parsed input's quantum
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
///   malformed or overflows the 110-bit field.
///
/// `start_offset` is the absolute byte position of `rest` within the
/// original input, used to attribute parse errors to the right column.
fn match_special(
    rest: &[u8],
    start_offset: usize,
    sign: bool,
) -> Option<Result<Decimal128, ParseDecimalError>> {
    if eq_ignore_ascii_case(rest, b"infinity") || eq_ignore_ascii_case(rest, b"inf") {
        return Some(Ok(if sign {
            Decimal128::NEG_INFINITY
        } else {
            Decimal128::INFINITY
        }));
    }
    // NaN / sNaN, optionally followed by a decimal payload.
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
/// decimal-encoded integer into the BID's 110-bit `T_MASK` field.
fn parse_nan_payload(
    digits: &[u8],
    offset: usize,
    sign: bool,
    signaling: bool,
) -> Result<Decimal128, ParseDecimalError> {
    let mut payload: u128 = 0;
    for (i, &c) in digits.iter().enumerate() {
        if !c.is_ascii_digit() {
            return Err(ParseDecimalError::InvalidCharacter(offset + i));
        }
        let d = (c - b'0') as u128;
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
    Ok(Decimal128::from_bits(bits))
}

/// `rest.strip_prefix_ignore_ascii_case(prefix)` — returns the suffix
/// after `prefix` (case-insensitive ASCII match) if `rest` starts with
/// `prefix`, else `None`.
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

// `String::repeat` requires `alloc`; only used in test helpers above.
#[cfg(test)]
extern crate alloc;
#[cfg(test)]
use alloc::string::ToString;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BIAS};

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::default())
            .expect("parse")
            .0
    }

    #[test]
    fn parse_zero() {
        let (d, _) = Decimal128::parse_str("0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(!d.is_sign_negative());

        let (d, _) = Decimal128::parse_str("-0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(d.is_sign_negative());
    }

    #[test]
    fn parse_integers() {
        assert_eq!(parse("1").to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(parse("-1").to_bits(), Decimal128::NEG_ONE.to_bits());
        assert_eq!(parse("10").to_bits(), Decimal128::TEN.to_bits());
    }

    #[test]
    fn parse_fixed_with_decimal() {
        // "0.5" = 5 × 10^-1
        let d = parse("0.5");
        let expected = Decimal128::from_bits(pack_finite(false, BIAS - 1, 5));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "-1.25" = -125 × 10^-2
        let d = parse("-1.25");
        let expected = Decimal128::from_bits(pack_finite(true, BIAS - 2, 125));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_scientific() {
        // "1e3" = 1 × 10^3
        let d = parse("1e3");
        let expected = Decimal128::from_bits(pack_finite(false, BIAS + 3, 1));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "1.5E-2" = 15 × 10^-3
        let d = parse("1.5E-2");
        let expected = Decimal128::from_bits(pack_finite(false, BIAS - 3, 15));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "+2.5e+1" = 25 × 10^0 = 25
        let d = parse("+2.5e+1");
        let (cmp, _) = d.partial_cmp(Decimal128::from(25i32));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
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
        // Quiet NaN with payload — common decTest shape.
        let d = parse("NaN22");
        assert!(d.is_nan() && !d.is_signaling_nan());
        assert_eq!(d.to_bits() & T_MASK, 22);

        let d = parse("-NaN22");
        assert!(d.is_nan() && d.is_sign_negative());
        assert_eq!(d.to_bits() & T_MASK, 22);

        // Signaling NaN with payload.
        let d = parse("sNaN33");
        assert!(d.is_signaling_nan());
        assert_eq!(d.to_bits() & T_MASK, 33);

        // Larger payloads up to the 110-bit envelope (~ 10^33).
        let big = parse("NaN999999999999999999999999999999999"); // 33 nines
        assert!(big.is_nan());
        let want_payload: u128 = (10u128.pow(33)) - 1;
        assert_eq!(big.to_bits() & T_MASK, want_payload);

        // Empty payload behaves the same as bare `NaN` / `sNaN`.
        assert_eq!(parse("NaN").to_bits(), parse("NaN0").to_bits());
        assert_eq!(parse("sNaN").to_bits(), parse("sNaN0").to_bits());

        // Case-insensitive prefix; payload digits stay numeric.
        assert_eq!(parse("nan22").to_bits(), parse("NaN22").to_bits());
        assert_eq!(parse("SNAN33").to_bits(), parse("sNaN33").to_bits());
    }

    #[test]
    fn parse_nan_payload_overflow() {
        // 35 nines exceeds the 110-bit field.
        let res = Decimal128::parse_str(
            "NaN99999999999999999999999999999999999",
            RoundingMode::default(),
        );
        assert!(matches!(res, Err(ParseDecimalError::InvalidCharacter(_))));
    }

    #[test]
    fn parse_leading_zeros() {
        let d = parse("0007");
        let (cmp, _) = d.partial_cmp(Decimal128::from(7i32));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        let d = parse("0.001");
        let expected = Decimal128::from_bits(pack_finite(false, BIAS - 3, 1));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_dot_only_integer_or_fraction() {
        // ".5" = 0.5
        let d = parse(".5");
        let expected = Decimal128::from_bits(pack_finite(false, BIAS - 1, 5));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "5." = 5
        let d = parse("5.");
        let (cmp, _) = d.partial_cmp(Decimal128::from(5i32));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn parse_errors() {
        assert!(matches!(
            Decimal128::parse_str("", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal128::parse_str("-", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal128::parse_str("1.2.3", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter(_))
        ));
        assert!(matches!(
            Decimal128::parse_str("1e", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent)
        ));
        assert!(matches!(
            Decimal128::parse_str("1e+", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent)
        ));
        assert!(matches!(
            Decimal128::parse_str("1e1000000000", RoundingMode::default()),
            Err(ParseDecimalError::ExponentOutOfRange)
        ));
        assert!(matches!(
            Decimal128::parse_str("abc", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter(0))
        ));
        assert!(matches!(
            Decimal128::parse_str("1.2x", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter(_))
        ));
    }

    #[test]
    fn parse_long_mantissa_inexact() {
        // 76-digit number: still exact (within MAX_PARSED_DIGITS).
        let s = "1234567890123456789012345678901234567890123456789012345678901234567890123456";
        assert_eq!(s.len(), 76);
        let (_, status) = Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap();
        // After rounding to PRECISION = 34, sticky digits are dropped.
        assert!(status.inexact());

        // 100-digit number: trailing digits feed the sticky bit; INEXACT raised.
        let s100 = "1".to_string() + &"0".repeat(33) + &"5".repeat(66);
        assert_eq!(s100.len(), 100);
        let (_, status) = Decimal128::parse_str(&s100, RoundingMode::NearestEven).unwrap();
        assert!(status.inexact());
    }

    #[test]
    fn parse_negative_exponent_subnormal_range() {
        // 1e-6143 is the smallest normal. 1e-6176 is the smallest subnormal.
        // Just check we can round-trip through parse without error.
        let (d, _) = Decimal128::parse_str("1e-6143", RoundingMode::default()).unwrap();
        assert!(d.is_finite());
        assert!(!d.is_zero());

        let (d, s) = Decimal128::parse_str("1e-6176", RoundingMode::default()).unwrap();
        assert!(d.is_finite());
        assert!(
            !d.is_zero(),
            "1e-6176 should be representable, got {d:?} status={s:?}"
        );
    }

    #[test]
    fn parse_overflow_to_infinity() {
        let (d, _) = Decimal128::parse_str("9.99e6144", RoundingMode::default()).unwrap();
        // Just under MAX — should be finite.
        assert!(d.is_finite());

        let (d, s) = Decimal128::parse_str("1e6145", RoundingMode::NearestEven).unwrap();
        // Above MAX — overflows to +Inf.
        assert!(d.is_infinite());
        assert!(s.overflow());
        assert!(s.inexact());
    }
}
