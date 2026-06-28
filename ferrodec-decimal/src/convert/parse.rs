//! Parsing a General Decimal Arithmetic numeric string into an exact value.
//!
//! The grammar (specification §"Numbers from strings"):
//!
//! ```text
//! sign           ::=  '+' | '-'
//! digits         ::=  digit [digit]...
//! decimal-part   ::=  digits '.' [digits] | ['.'] digits
//! exponent-part  ::=  ('e' | 'E') [sign] digits
//! infinity       ::=  'Infinity' | 'Inf'                    (any case)
//! nan            ::=  'NaN' [digits] | 'sNaN' [digits]      (any case)
//! numeric-string ::=  [sign] (decimal-part [exponent-part] | infinity | nan)
//! ```

use crate::Decimal;
use core::fmt;
use ferrodec_multiword::DecBig;

/// Why a string failed to parse as a decimal numeric string.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseDecimalError {
    /// The input was empty.
    Empty,
    /// The input was not a valid General Decimal Arithmetic numeric string.
    InvalidSyntax,
    /// The exponent, after folding the explicit exponent together with the
    /// fractional digit count, does not fit the supported `i32` range.
    ExponentOverflow,
}

impl fmt::Display for ParseDecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseDecimalError::Empty => "empty decimal string",
            ParseDecimalError::InvalidSyntax => "invalid decimal numeric string",
            ParseDecimalError::ExponentOverflow => "decimal exponent out of range",
        })
    }
}

impl core::error::Error for ParseDecimalError {}

impl Decimal {
    /// Parse a General Decimal Arithmetic numeric string into an *exact*
    /// value, with no context rounding: the coefficient and exponent are taken
    /// verbatim from the literal, so a coefficient wider than any working
    /// precision is preserved. Context-aware parsing (which rounds to the
    /// working precision) arrives with the rounding core.
    ///
    /// # Errors
    ///
    /// Returns [`ParseDecimalError`] for an empty string, a string that does
    /// not match the grammar, or an exponent outside the supported range.
    pub fn parse_str(s: &str) -> Result<Decimal, ParseDecimalError> {
        parse(s.as_bytes())
    }
}

impl core::str::FromStr for Decimal {
    type Err = ParseDecimalError;

    /// Parse via [`Decimal::parse_str`], so `"...".parse::<Decimal>()` is the
    /// same exact, non-rounding parse: the coefficient and exponent are taken
    /// verbatim from the literal. Exactness and error semantics are inherited
    /// unchanged from `parse_str`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseDecimalError`], exactly as [`Decimal::parse_str`] does.
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s)
    }
}

/// ASCII case-insensitive slice equality.
fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn parse(bytes: &[u8]) -> Result<Decimal, ParseDecimalError> {
    if bytes.is_empty() {
        return Err(ParseDecimalError::Empty);
    }
    let (sign, rest) = match bytes[0] {
        b'+' => (false, &bytes[1..]),
        b'-' => (true, &bytes[1..]),
        _ => (false, bytes),
    };
    if rest.is_empty() {
        return Err(ParseDecimalError::InvalidSyntax);
    }

    if eq_ci(rest, b"inf") || eq_ci(rest, b"infinity") {
        return Ok(Decimal::infinity(sign));
    }
    if let Some(nan) = parse_nan(sign, rest)? {
        return Ok(nan);
    }
    parse_finite(sign, rest)
}

/// Parse a `NaN` / `sNaN` with an optional digit payload. Returns `Ok(None)`
/// when `rest` has no NaN prefix, so the caller falls through to a finite
/// parse.
fn parse_nan(sign: bool, rest: &[u8]) -> Result<Option<Decimal>, ParseDecimalError> {
    let (signaling, payload_bytes) = if rest.len() >= 4 && eq_ci(&rest[..4], b"snan") {
        (true, &rest[4..])
    } else if rest.len() >= 3 && eq_ci(&rest[..3], b"nan") {
        (false, &rest[3..])
    } else {
        return Ok(None);
    };
    if !payload_bytes.iter().all(u8::is_ascii_digit) {
        return Err(ParseDecimalError::InvalidSyntax);
    }
    let payload = DecBig::from_ascii_digits(payload_bytes);
    Ok(Some(if signaling {
        Decimal::signaling_nan(sign, payload)
    } else {
        Decimal::quiet_nan(sign, payload)
    }))
}

fn parse_finite(sign: bool, rest: &[u8]) -> Result<Decimal, ParseDecimalError> {
    // Split off an exponent part, if any.
    let (coeff_part, explicit_exp) = match rest.iter().position(|&b| b == b'e' || b == b'E') {
        Some(pos) => (&rest[..pos], parse_exponent(&rest[pos + 1..])?),
        None => (rest, 0i64),
    };

    // Walk the coefficient: digits with at most one decimal point.
    let mut digits = alloc::vec::Vec::with_capacity(coeff_part.len());
    let mut frac_count = 0i64;
    let mut seen_point = false;
    let mut seen_digit = false;
    for &b in coeff_part {
        match b {
            b'.' => {
                if seen_point {
                    return Err(ParseDecimalError::InvalidSyntax);
                }
                seen_point = true;
            }
            b'0'..=b'9' => {
                digits.push(b);
                seen_digit = true;
                if seen_point {
                    frac_count += 1;
                }
            }
            _ => return Err(ParseDecimalError::InvalidSyntax),
        }
    }
    if !seen_digit {
        return Err(ParseDecimalError::InvalidSyntax);
    }

    let coeff = DecBig::from_ascii_digits(&digits);
    let exp = explicit_exp
        .checked_sub(frac_count)
        .ok_or(ParseDecimalError::ExponentOverflow)?;
    let exp = i32::try_from(exp).map_err(|_| ParseDecimalError::ExponentOverflow)?;
    Ok(Decimal::finite(sign, coeff, exp))
}

fn parse_exponent(bytes: &[u8]) -> Result<i64, ParseDecimalError> {
    if bytes.is_empty() {
        return Err(ParseDecimalError::InvalidSyntax);
    }
    let (neg, ds) = match bytes[0] {
        b'+' => (false, &bytes[1..]),
        b'-' => (true, &bytes[1..]),
        _ => (false, bytes),
    };
    if ds.is_empty() || !ds.iter().all(u8::is_ascii_digit) {
        return Err(ParseDecimalError::InvalidSyntax);
    }
    let mut v = 0i64;
    for &b in ds {
        v = v
            .checked_mul(10)
            .and_then(|x| x.checked_add(i64::from(b - b'0')))
            .ok_or(ParseDecimalError::ExponentOverflow)?;
    }
    Ok(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn from_str_matches_parse_str_and_preserves_cohort() {
        // The trait impl is the exact same parse: cohort (the trailing zero) and
        // value are taken verbatim, so it equals the inherent `parse_str`.
        let viad: Decimal = "1.230".parse().unwrap();
        assert_eq!(viad, Decimal::parse_str("1.230").unwrap());
        assert_eq!(
            Decimal::from_str("-0.0").unwrap(),
            Decimal::parse_str("-0.0").unwrap()
        );
    }

    #[test]
    fn from_str_propagates_the_same_error() {
        // An invalid string yields the identical `ParseDecimalError` either way.
        assert_eq!(
            "1.2.3".parse::<Decimal>().unwrap_err(),
            Decimal::parse_str("1.2.3").unwrap_err()
        );
        assert_eq!("".parse::<Decimal>().unwrap_err(), ParseDecimalError::Empty);
    }
}
