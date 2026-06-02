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

use crate::bid::{
    pack_quiet_nan, pack_signaling_nan, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, T_MASK,
};
use crate::decimal::{Decimal128, Decimal128Parts};
use crate::multiword::U256;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

/// Parse error returned by [`Decimal128::parse_str`].
///
/// Each variant names a distinct failure mode a caller may want to
/// react to differently (calculator UI diagnostics, REPL highlighting,
/// linting of decimal sources). Where a byte position is known it is
/// reported in `position`; sites with no meaningful position (`Empty`,
/// `ExponentOutOfRange`, `CoefficientOverflow`) omit the field.
///
/// The enum is `#[non_exhaustive]`: future revisions may add variants
/// under a minor bump without breaking exhaustive matches. Callers that
/// pattern-match exhaustively must include a wildcard arm.
///
/// Implements [`Display`](core::fmt::Display) and
/// [`core::error::Error`] so the type composes with `?`,
/// `Box<dyn Error>`, and `anyhow::Error` chains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseDecimalError {
    /// Empty input, or input consisting only of a sign with no
    /// trailing digits.
    Empty,
    /// A `+` or `-` appeared where the grammar does not permit one
    /// (e.g. `"+-1"`, `"1+2"`, `"1e++3"`, an in-mantissa sign).
    MisplacedSign { position: usize },
    /// A byte outside the decimal grammar (non-digit, non-sign,
    /// non-`.`, non-`e`/`E`), or trailing junk after a valid literal.
    InvalidCharacter { position: usize },
    /// An `e`/`E` introducer was present but the exponent that
    /// followed was malformed (missing digits, sign without digits).
    InvalidExponent { position: usize },
    /// The explicit exponent magnitude exceeds the parser's
    /// `MAX_EXPONENT_MAGNITUDE` (1 000 000 across all three formats).
    ExponentOutOfRange,
    /// The integer-coefficient prefix or the leading-fractional-zero
    /// run would shift the implicit exponent past the representable
    /// range, even before the explicit exponent is considered. The
    /// parent (Decimal128) parser does not produce this variant; the
    /// sibling parsers (Decimal64, Decimal32) do, because their
    /// coefficient digit budgets are narrower.
    CoefficientOverflow,
}

impl core::fmt::Display for ParseDecimalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty decimal literal"),
            Self::MisplacedSign { position } => {
                write!(f, "misplaced sign at byte {position}")
            }
            Self::InvalidCharacter { position } => {
                write!(f, "invalid character at byte {position}")
            }
            Self::InvalidExponent { position } => {
                write!(f, "malformed exponent at byte {position}")
            }
            Self::ExponentOutOfRange => f.write_str("exponent magnitude out of range"),
            Self::CoefficientOverflow => f.write_str("coefficient digit count out of range"),
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

    /// Build a `Decimal128` from a decimal literal at compile time.
    ///
    /// Intended for `const` initializers that embed an exact published
    /// constant so the source reads as the decimal itself:
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// const PLANCK: Decimal128 = Decimal128::from_str_const("6.62607015e-34");
    /// const SPEED_OF_LIGHT: Decimal128 = Decimal128::from_str_const("2.99792458e8");
    /// assert!(PLANCK.is_finite() && !PLANCK.is_zero());
    ///
    /// // Decimal literals are exact: 0.1 carries no representation error.
    /// assert_eq!(
    ///     Decimal128::from_str_const("0.1").to_bits(),
    ///     Decimal128::try_new(1, -1).unwrap().to_bits(),
    /// );
    /// ```
    ///
    /// The literal is an optional sign, decimal digits with an optional
    /// point, and an optional `e`/`E` exponent. Infinity and NaN are
    /// rejected; use the named [`Decimal128::INFINITY`] /
    /// [`Decimal128::NAN`] constants instead.
    ///
    /// # Panics
    ///
    /// Panics, which is a compile error in `const` context, if the literal
    /// is not a finite decimal exactly representable in Decimal128: a
    /// malformed or empty string, more than 34 significant figures, or an
    /// exponent outside `[-6176, 6111]`. Exactness is required; an inexact
    /// literal is a compile error, never silent rounding. A value with
    /// many trailing zeros can exceed 34 significant figures as written
    /// (`"1"` followed by 34 zeros); write it in scientific notation
    /// (`"1e34"`) instead.
    ///
    /// The threat model is a programmer typo in a source literal, not
    /// untrusted input. For runtime parsing of untrusted input, with
    /// rounding and a recoverable error, use [`Decimal128::parse_str`].
    #[must_use]
    pub const fn from_str_const(s: &str) -> Self {
        match Decimal128::from_parts(parse_literal(s.as_bytes())) {
            Some(d) => d,
            None => panic!("from_str_const: literal not representable in Decimal128"),
        }
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
    // Integer digits beyond MAX_PARSED_DIGITS that we couldn't fold into
    // `coef`. Each one shifts the value up by 10× (since it's a digit
    // before the implicit decimal point), so we accumulate them and add
    // to the unbiased exponent at the end.
    let mut extra_int_digits: u32 = 0;
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
                    let leading_int_zero = coef.is_zero() && d == 0 && !decimal_seen;
                    let leading_frac_zero = coef.is_zero() && d == 0 && decimal_seen;
                    if leading_int_zero {
                        // Pure leading zero in the integer part —
                        // ignore.
                    } else if leading_frac_zero {
                        // Leading zero AFTER the decimal point but
                        // BEFORE the first non-zero digit. Shifts the
                        // quantum down by one but does not "spend" a
                        // digit-budget slot; the value's significant
                        // figures haven't started yet. Saturate then
                        // reject past MAX_EXPONENT_MAGNITUDE: an
                        // adversarial run of leading fractional zeros
                        // would otherwise overflow the u32 counter (a
                        // debug-mode panic / DoS) and, via the later
                        // `as i32` cast, silently miscompute the
                        // exponent. Mirrors the explicit-exponent guard
                        // and the siblings' H8 saturation guard.
                        digits_after_point = digits_after_point.saturating_add(1);
                        if digits_after_point > MAX_EXPONENT_MAGNITUDE {
                            return Err(ParseDecimalError::CoefficientOverflow);
                        }
                    } else {
                        coef = coef.mul10().add(U256::from_u128(d as u128));
                        digits_total += 1;
                        if decimal_seen {
                            // Bounded by digits_total < MAX_PARSED_DIGITS;
                            // saturating for uniformity with the
                            // unbounded arm above.
                            digits_after_point = digits_after_point.saturating_add(1);
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
                        // Saturate then reject past
                        // MAX_EXPONENT_MAGNITUDE: without the cap an
                        // adversarial run (`"1" + "0"*3e9`) saturates to
                        // u32::MAX, which the later `as i32` cast reads
                        // as -1 and silently miscomputes the exponent.
                        // Mirrors the siblings' B7 guard.
                        extra_int_digits = extra_int_digits.saturating_add(1);
                        if extra_int_digits > MAX_EXPONENT_MAGNITUDE {
                            return Err(ParseDecimalError::CoefficientOverflow);
                        }
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
                    return Err(ParseDecimalError::InvalidCharacter { position: idx });
                }
                decimal_seen = true;
                idx += 1;
            }
            b'e' | b'E' => break,
            b'+' | b'-' => return Err(ParseDecimalError::MisplacedSign { position: idx }),
            _ => return Err(ParseDecimalError::InvalidCharacter { position: idx }),
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
            return Err(ParseDecimalError::InvalidExponent { position: idx });
        }
        let mut exp_val: u32 = 0;
        let mut exp_has_digit = false;
        while idx < bytes.len() {
            let c = bytes[idx];
            if !c.is_ascii_digit() {
                return Err(match c {
                    b'+' | b'-' => ParseDecimalError::MisplacedSign { position: idx },
                    _ => ParseDecimalError::InvalidCharacter { position: idx },
                });
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
            return Err(ParseDecimalError::InvalidExponent { position: idx });
        }
        exp_explicit = if exp_sign {
            -(exp_val as i32)
        } else {
            exp_val as i32
        };
    }

    if idx != bytes.len() {
        return Err(ParseDecimalError::InvalidCharacter { position: idx });
    }

    // Quantum: each digit after the decimal point shifts the value down
    // by one decimal position; each integer digit beyond MAX_PARSED_DIGITS
    // shifts the value up by one (it stayed in the integer part but we
    // could not fold it into `coef`); plus the explicit exponent. Both
    // counters are capped at MAX_EXPONENT_MAGNITUDE (1_000_000) above,
    // well inside i32, so the `as i32` casts are exact and the
    // saturating adds only guard the explicit-exponent contribution.
    let unbiased_exp = exp_explicit
        .saturating_add(extra_int_digits as i32)
        .saturating_sub(digits_after_point as i32);

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

/// Parse a finite decimal literal into its [`Decimal128Parts`] at compile
/// time, panicking on anything not exactly representable.
///
/// The byte loop mirrors [`parse_str_inner`] restricted to the exactly
/// representable, finite subset: the same sign handling, leading-zero
/// rules, and quantum derivation, with the rounding machinery replaced by
/// a hard exactness gate. Because every accepted input has at most 34
/// significant figures the coefficient fits a `u128` directly, so no
/// `U256` and no rounding are needed; on the accepted subset the result
/// agrees with `parse_str` bit for bit (pinned by a property test).
///
/// All `panic!` messages are static strings so they surface in `const`
/// evaluation. Each names a distinct failure so a stalled build points at
/// the offending constraint.
const fn parse_literal(bytes: &[u8]) -> Decimal128Parts {
    assert!(!bytes.is_empty(), "from_str_const: empty decimal literal");

    let len = bytes.len();
    let (negative, mut idx) = match bytes[0] {
        b'+' => (false, 1usize),
        b'-' => (true, 1usize),
        _ => (false, 0usize),
    };
    assert!(idx < len, "from_str_const: sign with no digits");

    let mut coef: u128 = 0;
    // Decimal positions the coefficient sits to the right of the point.
    // Counts significant fractional digits plus the leading fractional
    // zeros that shift the quantum without starting the significand.
    let mut digits_after_point: i32 = 0;
    let mut decimal_seen = false;
    let mut has_digit = false;

    while idx < len {
        let c = bytes[idx];
        match c {
            b'0'..=b'9' => {
                has_digit = true;
                let d = (c - b'0') as u128;
                let leading_int_zero = coef == 0 && d == 0 && !decimal_seen;
                let leading_frac_zero = coef == 0 && d == 0 && decimal_seen;
                if leading_int_zero {
                    // Pure leading zero in the integer part: ignore it.
                } else if leading_frac_zero {
                    // Leading zero after the point but before the first
                    // significant digit: shifts the quantum down by one
                    // without spending a significand slot.
                    digits_after_point += 1;
                    assert!(
                        digits_after_point <= MAX_EXPONENT_MAGNITUDE as i32,
                        "from_str_const: exponent out of range"
                    );
                } else {
                    // A significant digit. `coef < COEFFICIENT_LIMIT`
                    // holds on entry, so `coef * 10 + d` cannot overflow a
                    // u128; reaching the limit means more than 34
                    // significant figures, which is not exactly
                    // representable.
                    let next = coef * 10 + d;
                    assert!(
                        next < COEFFICIENT_LIMIT,
                        "from_str_const: more than 34 significant figures, not exactly representable (use scientific notation)"
                    );
                    coef = next;
                    if decimal_seen {
                        digits_after_point += 1;
                    }
                }
                idx += 1;
            }
            b'.' => {
                assert!(!decimal_seen, "from_str_const: more than one decimal point");
                decimal_seen = true;
                idx += 1;
            }
            b'e' | b'E' => break,
            b'+' | b'-' => panic!("from_str_const: misplaced sign in mantissa"),
            _ => panic!("from_str_const: invalid character in literal"),
        }
    }

    assert!(has_digit, "from_str_const: no digits in literal");

    // Optional exponent clause.
    let mut exp_explicit: i32 = 0;
    if idx < len && (bytes[idx] == b'e' || bytes[idx] == b'E') {
        idx += 1;
        let exp_negative = if idx < len && bytes[idx] == b'+' {
            idx += 1;
            false
        } else if idx < len && bytes[idx] == b'-' {
            idx += 1;
            true
        } else {
            false
        };
        assert!(idx < len, "from_str_const: malformed exponent");
        let mut exp_val: i32 = 0;
        while idx < len {
            let c = bytes[idx];
            assert!(
                !(c < b'0' || c > b'9'),
                "from_str_const: invalid character in exponent"
            );
            // Capped at MAX_EXPONENT_MAGNITUDE each step, so `exp_val * 10`
            // stays well inside i32.
            exp_val = exp_val * 10 + (c - b'0') as i32;
            assert!(
                exp_val <= MAX_EXPONENT_MAGNITUDE as i32,
                "from_str_const: exponent out of range"
            );
            idx += 1;
        }
        exp_explicit = if exp_negative { -exp_val } else { exp_val };
    }

    assert!(
        idx == len,
        "from_str_const: trailing characters after literal"
    );

    let unbiased_exp = exp_explicit - digits_after_point;
    // Range check in i32 before the i16 cast, so a large exponent cannot
    // silently wrap into the representable range. This is the same biased
    // bound `from_parts` enforces, applied here while the full magnitude
    // is still visible.
    let biased = unbiased_exp + BIAS as i32;
    assert!(
        !(biased < 0 || biased > BIASED_EXP_MAX as i32),
        "from_str_const: exponent out of range"
    );

    Decimal128Parts {
        negative,
        coefficient: coef,
        exponent: unbiased_exp as i16,
    }
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
            return Err(ParseDecimalError::InvalidCharacter {
                position: offset + i,
            });
        }
        let d = (c - b'0') as u128;
        payload = payload
            .checked_mul(10)
            .and_then(|p| p.checked_add(d))
            .ok_or(ParseDecimalError::InvalidCharacter {
                position: offset + i,
            })?;
        if payload > T_MASK {
            return Err(ParseDecimalError::InvalidCharacter {
                position: offset + i,
            });
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
use alloc::format;
#[cfg(test)]
use alloc::string::ToString;

/// Build a [`Decimal128`] from a decimal literal at compile time.
///
/// `dec!("6.62607015e-34")` expands to
/// [`Decimal128::from_str_const`]`("6.62607015e-34")`, so the same
/// exact-or-compile-error contract applies (see its `# Panics`). Const
/// evaluation bakes the value into the binary; there is no runtime parser.
/// Available only with the `fmt` feature.
///
/// Each format crate exports its own `dec!`; to use more than one, rename
/// on import (`use ferrodec::dec as dec128;`).
///
/// ```
/// use ferrodec::{dec, Decimal128};
///
/// const PLANCK: Decimal128 = dec!("6.62607015e-34");
/// assert!(PLANCK.is_finite() && !PLANCK.is_zero());
/// ```
#[macro_export]
macro_rules! dec {
    ($s:literal) => {
        $crate::Decimal128::from_str_const($s)
    };
}

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
        assert!(matches!(
            res,
            Err(ParseDecimalError::InvalidCharacter { .. })
        ));
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
            Err(ParseDecimalError::InvalidCharacter { .. })
        ));
        assert!(matches!(
            Decimal128::parse_str("1e", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent { .. })
        ));
        assert!(matches!(
            Decimal128::parse_str("1e+", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent { .. })
        ));
        assert!(matches!(
            Decimal128::parse_str("1e1000000000", RoundingMode::default()),
            Err(ParseDecimalError::ExponentOutOfRange)
        ));
        assert!(matches!(
            Decimal128::parse_str("abc", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter { position: 0 })
        ));
        assert!(matches!(
            Decimal128::parse_str("1.2x", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn parse_misplaced_sign_distinct_from_invalid_character() {
        // A bare `+` or `-` after a valid mantissa byte is a misplaced
        // sign, not a generic invalid byte. ADR-0029 item 2 / fd-7f1
        // makes the distinction matchable so callers can produce
        // sharper diagnostics. Trailing garbage after a complete literal
        // also routes here when the trailing byte is a sign.
        assert!(matches!(
            Decimal128::parse_str("+-1", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 1 })
        ));
        assert!(matches!(
            Decimal128::parse_str("1+2", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 1 })
        ));
        // `e` introducer is consumed, optional sign `+` is consumed,
        // then a second `+` at byte 3 triggers MisplacedSign inside
        // the exponent loop.
        assert!(matches!(
            Decimal128::parse_str("1e++3", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 3 })
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
    fn parse_integer_beyond_capacity_scales_quantum() {
        // Regression: integer digits beyond MAX_PARSED_DIGITS used to
        // be silently dropped (the saturating_add increment did nothing
        // to the quantum), so "1" × N for N > 76 stayed at the magnitude
        // of "1" × 76 instead of growing 10× per digit.
        //
        // Anchor at 76 digits, then check the next four lengths each
        // multiply the value by exactly 10. We compare against the
        // parsed "1.111…1eK" form so the assertion is independent of
        // round-half-even tie-breaking on the trailing digits.
        for n in 76..=80 {
            let ones = "1".repeat(n);
            let (a, _) = Decimal128::parse_str(&ones, RoundingMode::NearestEven).unwrap();

            // Equivalent scientific form: integer "1"×n equals
            // (10^n − 1) / 9, which is 1.111…1 (34 sig digits after
            // rounding) × 10^(n − 1).
            let canonical = format!("1.111111111111111111111111111111111E{:+}", (n as i32) - 1);
            let (b, _) = Decimal128::parse_str(&canonical, RoundingMode::NearestEven).unwrap();
            let (cmp, _) = a.partial_cmp(b);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "parse(\"1\"×{n}) = {a:?}, expected {b:?}",
            );
        }
    }

    #[test]
    fn parse_one_then_many_zeros() {
        // "1" followed by N zeros parses to 10^N for any N. Before the
        // fix, N > 75 (one leading "1" plus 75 more digits inside the
        // U256 capacity) silently dropped the trailing zeros.
        for n in [75u32, 76, 77, 100, 500] {
            let s = "1".to_string() + &"0".repeat(n as usize);
            let (a, _) = Decimal128::parse_str(&s, RoundingMode::NearestEven).unwrap();
            let canonical = format!("1E{n:+}");
            let (b, _) = Decimal128::parse_str(&canonical, RoundingMode::NearestEven).unwrap();
            let (cmp, _) = a.partial_cmp(b);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "parse(\"1\" + \"0\"×{n}) = {a:?}, expected 10^{n} = {b:?}",
            );
        }
    }

    #[test]
    fn parse_leading_fractional_zeros_past_budget() {
        // Regression: leading fractional zeros after `.` used to spend
        // digit-budget slots even though the coefficient stayed at zero,
        // pushing the first significant digits past the
        // MAX_PARSED_DIGITS = 76 boundary and into the sticky bit. The
        // result deflated by 10× per zero past the budget.
        //
        // Anchor: "0." + 80 zeros + 34 ones must equal 1.111…1E−81 exactly
        // (34 sig digits, all ones).
        let zeros = "0".repeat(80);
        let ones = "1".repeat(34);
        let s = format!("0.{zeros}{ones}");
        let (a, _) = Decimal128::parse_str(&s, RoundingMode::NearestEven).unwrap();
        let canonical = "1.111111111111111111111111111111111E-81";
        let (b, _) = Decimal128::parse_str(canonical, RoundingMode::NearestEven).unwrap();
        let (cmp, _) = a.partial_cmp(b);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "parse(\"0.{{80 zeros}}{{34 ones}}\") = {a:?}, expected {b:?}",
        );
    }

    #[test]
    fn parse_post_budget_fractional_digits_keep_quantum() {
        // Regression: once digits_total reaches MAX_PARSED_DIGITS = 76,
        // subsequent fractional digits used to keep incrementing
        // digits_after_point — which made unbiased_exp = -digits_after_point
        // deflate the value by 10× per extra fractional digit. The fix:
        // post-budget fractional digits feed *only* the sticky bit.
        //
        // "1." + 80 ones must equal 1.111…1 (34 sig digits) within the
        // round-half-even tie-break, not 0.0001111… or similar.
        let s = "1.".to_string() + &"1".repeat(80);
        let (a, _) = Decimal128::parse_str(&s, RoundingMode::NearestEven).unwrap();
        let canonical = "1.111111111111111111111111111111111";
        let (b, _) = Decimal128::parse_str(canonical, RoundingMode::NearestEven).unwrap();
        let (cmp, _) = a.partial_cmp(b);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "parse(\"1.{{80 ones}}\") = {a:?}, expected {b:?}",
        );
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
