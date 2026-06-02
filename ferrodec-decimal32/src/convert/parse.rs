//! `&str` → [`Decimal32`] parser. No allocation, no `std`.
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
//! payload       := DIGIT+        // diagnostic NaN payload, ≤ 6 digits
//! exponent      := ("e" | "E") sign? digits
//! ```
//!
//! NaN payloads encode in the BID significand's trailing 20 bits
//! (`T_MASK`). Payloads up to `2^20 − 1` (`1_048_575`) round-trip;
//! larger values are rejected with `InvalidCharacter` at the first
//! overflowing digit. Canonical NaN payloads are bounded at `< 10^6 = 1_000_000`
//! per IEEE 754-2019 §3.5.2; the parser accepts the wider raw-field
//! range and `canonicalize()` rewrites the rest.
//!
//! Up to 16 mantissa digits are accumulated exactly in a `u64`;
//! trailing digits beyond that contribute to the rounding sticky bit.
//! The rounding direction comes from the supplied [`RoundingMode`].

use crate::bid::{
    pack_quiet_nan, pack_signaling_nan, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, T_MASK,
};
use crate::decimal::{Decimal32, Decimal32Parts};
use crate::ops::round_and_pack_finite;
use ferrodec_ieee::{RoundingMode, Status};

/// Parse error returned by [`Decimal32::parse_str`].
///
/// Each variant names a distinct failure mode a caller may want to
/// react to differently (calculator UI diagnostics, REPL highlighting,
/// linting of decimal sources). Where a byte position is known it is
/// reported in `position`; sites with no meaningful position (`Empty`,
/// `ExponentOutOfRange`, `CoefficientOverflow`) omit the field.
///
/// The enum is `#[non_exhaustive]`: future revisions may add variants
/// under a minor bump without breaking exhaustive matches. Callers
/// that pattern-match exhaustively must include a wildcard arm.
///
/// Definition is byte-identical to the parent
/// `ferrodec::ParseDecimalError` and the
/// `ferrodec_decimal64::ParseDecimalError`; each crate carries its own
/// copy because the type sits at the crate boundary.
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
    /// `MAX_EXPONENT_MAGNITUDE` (1 000 000).
    ExponentOutOfRange,
    /// The integer-coefficient prefix or the leading-fractional-zero
    /// run would shift the implicit exponent past the representable
    /// range, even before the explicit exponent is considered. This is
    /// the H8 saturation guard recorded in ADR-0018; it fires only on
    /// the sibling parsers (Decimal64, Decimal32), never on the parent.
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

const MAX_PARSED_DIGITS: u32 = 16;
const MAX_EXPONENT_MAGNITUDE: u32 = 1_000_000;

// A5-F4 (Agent 5; the Decimal64 H8 shape): the `extra_int_digits as
// i32` / `digits_after_point as i32` casts in `parse_str` are sound
// only because both counters are capped at `MAX_EXPONENT_MAGNITUDE`
// before the cast. Encode "the cap fits in `i32`" as a compile-time
// invariant so the cast safety is type-checked, not just asserted in
// prose.
const _: () = assert!(MAX_EXPONENT_MAGNITUDE <= i32::MAX as u32);

impl Decimal32 {
    /// Parse a `&str` into a `Decimal32`, rounding per `rm`.
    ///
    /// On success returns `(value, status)`. `status.inexact()` is set
    /// iff the input had more significant digits than the format can
    /// represent at the chosen precision.
    ///
    /// # Threat model
    ///
    /// `parse_str`, and its [`FromStr`](core::str::FromStr) delegate,
    /// is the only attacker controlled surface in this crate. Anything
    /// downstream of `str::parse::<Decimal32>()` may hand it bytes from
    /// any source: file content, user keystrokes, JSON or SMIL feeding
    /// the calculator core. The caller owns deciding whether the source
    /// is trusted; the parser itself assumes it is not.
    ///
    /// The three outcomes worth defending against are a panic on a
    /// malformed or oversized literal (a denial of service on debug
    /// builds), a silent miscompute where an overflowing counter wraps
    /// the unbiased exponent and yields a numerically wrong value with
    /// no `INVALID` flag, and unbounded work on a long input. All three
    /// are closed: the digit and implicit exponent counters saturate
    /// rather than wrapping, an exponent out of range returns
    /// [`ParseDecimalError::ExponentOutOfRange`] instead of producing a
    /// wrong `Decimal32`, and the scan is a single linear pass with no
    /// quadratic blowup. The saturation caps are the private
    /// `MAX_PARSED_DIGITS` and `MAX_EXPONENT_MAGNITUDE` constants.
    ///
    /// Every accumulator is fixed width: a `u64` coefficient, `u32`
    /// digit counters, an `i32` exponent. Parsing allocates nothing and
    /// reads only `core::str`, so the residual risk is integer overflow
    /// inside those counters, addressed by the saturation above, not
    /// memory exhaustion. The cost is bounded by input length, not by
    /// input value.
    pub fn parse_str(s: &str, rm: RoundingMode) -> Result<(Self, Status), ParseDecimalError> {
        parse_str_inner(s.as_bytes(), rm)
    }

    /// Build a `Decimal32` from a decimal literal at compile time.
    ///
    /// Intended for `const` initializers that embed an exact published
    /// constant so the source reads as the decimal itself:
    ///
    /// ```
    /// use ferrodec_decimal32::Decimal32;
    ///
    /// const STANDARD_GRAVITY: Decimal32 = Decimal32::from_str_const("9.80665");
    /// assert!(STANDARD_GRAVITY.is_finite() && !STANDARD_GRAVITY.is_zero());
    ///
    /// // Decimal literals are exact: 0.1 carries no representation error.
    /// assert_eq!(
    ///     Decimal32::from_str_const("0.1").to_bits(),
    ///     Decimal32::try_new(1, -1).unwrap().to_bits(),
    /// );
    /// ```
    ///
    /// The literal is an optional sign, decimal digits with an optional
    /// point, and an optional `e`/`E` exponent. Infinity and NaN are
    /// rejected; use the named [`Decimal32::INFINITY`] / [`Decimal32::NAN`]
    /// constants instead.
    ///
    /// # Panics
    ///
    /// Panics, which is a compile error in `const` context, if the literal
    /// is not a finite decimal exactly representable in Decimal32: a
    /// malformed or empty string, more than 7 significant figures, or an
    /// exponent outside `[-101, 90]`. Exactness is required; an inexact
    /// literal is a compile error, never silent rounding. A value with
    /// many trailing zeros can exceed 7 significant figures as written;
    /// write it in scientific notation instead.
    ///
    /// The threat model is a programmer typo in a source literal, not
    /// untrusted input. For runtime parsing of untrusted input, with
    /// rounding and a recoverable error, use [`Decimal32::parse_str`].
    #[must_use]
    pub const fn from_str_const(s: &str) -> Self {
        match Decimal32::from_parts(parse_literal(s.as_bytes())) {
            Some(d) => d,
            None => panic!("from_str_const: literal not representable in Decimal32"),
        }
    }
}

/// Idiomatic Rust parsing via the `str::parse` extension method.
///
/// Defaults to [`RoundingMode::NearestEven`] and discards the
/// [`Status`] flags from `parse_str`.
impl core::str::FromStr for Decimal32 {
    type Err = ParseDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_str_inner(s.as_bytes(), RoundingMode::NearestEven).map(|(v, _)| v)
    }
}

fn parse_str_inner(
    bytes: &[u8],
    rm: RoundingMode,
) -> Result<(Decimal32, Status), ParseDecimalError> {
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
                let d = u32::from(c - b'0');
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
                        // figures haven't started yet. This branch is
                        // not bounded by `digits_total` (which stays
                        // zero until the first significant digit), so
                        // an adversarial run of leading fractional
                        // zeros would overflow the `u32` counter (a
                        // debug-mode panic / DoS). Saturate, then
                        // reject past `MAX_EXPONENT_MAGNITUDE` — the
                        // identical guard the explicit-exponent path
                        // applies at `1e-1000001`.
                        digits_after_point = digits_after_point.saturating_add(1);
                        if digits_after_point > MAX_EXPONENT_MAGNITUDE {
                            // H8 saturation guard. Distinct from
                            // `ExponentOutOfRange` (an *explicit*
                            // exponent past `MAX_EXPONENT_MAGNITUDE`):
                            // here the input shifts the implicit
                            // exponent past the cap purely through a
                            // run of leading fractional zeros, before
                            // any `e` introducer. ADR-0029 item 2
                            // / fd-7f1 makes the distinction matchable.
                            return Err(ParseDecimalError::CoefficientOverflow);
                        }
                    } else {
                        // L3: this `* 10 + d` cannot overflow. The
                        // branch is gated by `digits_total <
                        // MAX_PARSED_DIGITS` (16), so `coef` holds at
                        // most 16 decimal digits, under 10^16, well
                        // inside `u64`.
                        coef = coef * 10 + u64::from(d);
                        digits_total += 1;
                        if decimal_seen {
                            // Bounded by `digits_total <
                            // MAX_PARSED_DIGITS`; saturating for
                            // uniformity with the unbounded branches.
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
                        // Saturate, then reject past
                        // `MAX_EXPONENT_MAGNITUDE`: without the cap an
                        // adversarial run (`"1" + "0"*3e9`) saturates
                        // to `u32::MAX`, which reinterprets as `-1`
                        // under the later `as i32` cast and silently
                        // miscomputes the exponent. Mirrors the
                        // explicit-exponent guard.
                        extra_int_digits = extra_int_digits.saturating_add(1);
                        if extra_int_digits > MAX_EXPONENT_MAGNITUDE {
                            // ADR-0029 item 2 / fd-7f1 promotes this
                            // implicit-exponent overflow to
                            // `CoefficientOverflow`, distinct from the
                            // explicit-exponent `ExponentOutOfRange`.
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
            let d = u32::from(c - b'0');
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

    // `extra_int_digits` and `digits_after_point` are each capped at
    // MAX_EXPONENT_MAGNITUDE (1_000_000) at their increment sites, so
    // both casts are well below `i32::MAX` and cannot wrap. The
    // saturating add / sub then guards only `exp_explicit`'s
    // contribution.
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

/// Parse a finite decimal literal into its [`Decimal32Parts`] at compile
/// time, panicking on anything not exactly representable.
///
/// The byte loop mirrors [`parse_str_inner`] restricted to the exactly
/// representable, finite subset: the same sign handling, leading-zero
/// rules, and quantum derivation, with the rounding machinery replaced by
/// a hard exactness gate. Because every accepted input has at most 7
/// significant figures the coefficient fits a `u32` directly, so no
/// rounding is needed; on the accepted subset the result agrees with
/// `parse_str` bit for bit (pinned by a property test).
///
/// All `panic!` messages are static strings so they surface in `const`
/// evaluation. Each names a distinct failure so a stalled build points at
/// the offending constraint.
const fn parse_literal(bytes: &[u8]) -> Decimal32Parts {
    assert!(!bytes.is_empty(), "from_str_const: empty decimal literal");

    let len = bytes.len();
    let (negative, mut idx) = match bytes[0] {
        b'+' => (false, 1usize),
        b'-' => (true, 1usize),
        _ => (false, 0usize),
    };
    assert!(idx < len, "from_str_const: sign with no digits");

    let mut coef: u32 = 0;
    let mut digits_after_point: i32 = 0;
    let mut decimal_seen = false;
    let mut has_digit = false;

    while idx < len {
        let c = bytes[idx];
        match c {
            b'0'..=b'9' => {
                has_digit = true;
                let d = (c - b'0') as u32;
                let leading_int_zero = coef == 0 && d == 0 && !decimal_seen;
                let leading_frac_zero = coef == 0 && d == 0 && decimal_seen;
                if leading_int_zero {
                    // Pure leading zero in the integer part: ignore it.
                } else if leading_frac_zero {
                    // Leading zero after the point but before the first
                    // significant digit: shifts the quantum without
                    // spending a significand slot.
                    digits_after_point += 1;
                    assert!(
                        digits_after_point <= MAX_EXPONENT_MAGNITUDE as i32,
                        "from_str_const: exponent out of range"
                    );
                } else {
                    // A significant digit. `coef < COEFFICIENT_LIMIT` holds
                    // on entry, so `coef * 10 + d` cannot overflow a u32;
                    // reaching the limit means more than 7 significant
                    // figures, which is not exactly representable.
                    let next = coef * 10 + d;
                    assert!(
                        next < COEFFICIENT_LIMIT,
                        "from_str_const: more than 7 significant figures, not exactly representable (use scientific notation)"
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
    // silently wrap into the representable range.
    let biased = unbiased_exp + BIAS as i32;
    assert!(
        !(biased < 0 || biased > BIASED_EXP_MAX as i32),
        "from_str_const: exponent out of range"
    );

    Decimal32Parts {
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
///   malformed or overflows the 20-bit field.
fn match_special(
    rest: &[u8],
    start_offset: usize,
    sign: bool,
) -> Option<Result<Decimal32, ParseDecimalError>> {
    if eq_ignore_ascii_case(rest, b"infinity") || eq_ignore_ascii_case(rest, b"inf") {
        return Some(Ok(if sign {
            Decimal32::NEG_INFINITY
        } else {
            Decimal32::INFINITY
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
) -> Result<Decimal32, ParseDecimalError> {
    let mut payload: u32 = 0;
    for (i, &c) in digits.iter().enumerate() {
        if !c.is_ascii_digit() {
            return Err(ParseDecimalError::InvalidCharacter {
                position: offset + i,
            });
        }
        let d = u32::from(c - b'0');
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
    Ok(Decimal32::from_bits(bits))
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

/// Build a [`Decimal32`] from a decimal literal at compile time.
///
/// `dec!("9.80665")` expands to
/// [`Decimal32::from_str_const`]`("9.80665")`, so the same
/// exact-or-compile-error contract applies (see its `# Panics`). Const
/// evaluation bakes the value into the binary; there is no runtime parser.
/// Available only with the `fmt` feature.
///
/// Each format crate exports its own `dec!`; to use more than one, rename
/// on import (`use ferrodec_decimal32::dec as dec32;`).
///
/// ```
/// use ferrodec_decimal32::{dec, Decimal32};
///
/// const STANDARD_GRAVITY: Decimal32 = dec!("9.80665");
/// assert!(STANDARD_GRAVITY.is_finite() && !STANDARD_GRAVITY.is_zero());
/// ```
#[macro_export]
macro_rules! dec {
    ($s:literal) => {
        $crate::Decimal32::from_str_const($s)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BiasedExp, Coefficient, BIAS};

    fn parse(s: &str) -> Decimal32 {
        Decimal32::parse_str(s, RoundingMode::default())
            .expect("parse")
            .0
    }

    #[test]
    fn parse_zero() {
        let (d, _) = Decimal32::parse_str("0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(!d.is_sign_negative());

        let (d, _) = Decimal32::parse_str("-0", RoundingMode::default()).unwrap();
        assert!(d.is_zero());
        assert!(d.is_sign_negative());
    }

    #[test]
    fn parse_integers() {
        assert_eq!(parse("1").to_bits(), Decimal32::ONE.to_bits());
        assert_eq!(parse("-1").to_bits(), Decimal32::NEG_ONE.to_bits());
        assert_eq!(parse("10").to_bits(), Decimal32::TEN.to_bits());
    }

    #[test]
    fn parse_fixed_with_decimal() {
        // "0.5" = 5 × 10^-1
        let d = parse("0.5");
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 1).unwrap(),
            Coefficient::try_new(5).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "-1.25" = -125 × 10^-2
        let d = parse("-1.25");
        let expected = Decimal32::from_bits(pack_finite(
            true,
            BiasedExp::try_from_biased(BIAS - 2).unwrap(),
            Coefficient::try_new(125).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_scientific() {
        // "1e3" = 1 × 10^3
        let d = parse("1e3");
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS + 3).unwrap(),
            Coefficient::try_new(1).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());

        // "1.5E-2" = 15 × 10^-3
        let d = parse("1.5E-2");
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 3).unwrap(),
            Coefficient::try_new(15).unwrap(),
        ));
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

        // Larger payloads up to the 20-bit envelope (~ 10^6 - 1).
        let big = parse("NaN999999"); // 6 nines = 999_999, fits in 20 bits.
        assert!(big.is_nan());
        assert_eq!(big.to_bits() & T_MASK, 999_999);

        // Empty payload behaves the same as bare `NaN` / `sNaN`.
        assert_eq!(parse("NaN").to_bits(), parse("NaN0").to_bits());
        assert_eq!(parse("sNaN").to_bits(), parse("sNaN0").to_bits());
    }

    #[test]
    fn parse_nan_payload_overflow() {
        // 8 nines exceeds the 20-bit field (max 1_048_575).
        let res = Decimal32::parse_str("NaN99999999", RoundingMode::default());
        assert!(matches!(
            res,
            Err(ParseDecimalError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn parse_leading_zeros() {
        let d = parse("0007");
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS).unwrap(),
            Coefficient::try_new(7).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());
    }

    #[test]
    fn parse_rounding_at_precision_boundary() {
        // 8 digits → must round to 7. NearestEven with last digit 8 →
        // round up. 12345678 → 1234568 × 10^1.
        let (d, status) = Decimal32::parse_str("12345678", RoundingMode::NearestEven).unwrap();
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS + 1).unwrap(),
            Coefficient::try_new(1_234_568).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());
        assert!(status.inexact());
    }

    #[test]
    fn parse_invalid() {
        assert!(matches!(
            Decimal32::parse_str("", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal32::parse_str("+", RoundingMode::default()),
            Err(ParseDecimalError::Empty)
        ));
        assert!(matches!(
            Decimal32::parse_str("abc", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter { position: 0 })
        ));
        assert!(matches!(
            Decimal32::parse_str("1.2.3", RoundingMode::default()),
            Err(ParseDecimalError::InvalidCharacter { .. })
        ));
        assert!(matches!(
            Decimal32::parse_str("1e", RoundingMode::default()),
            Err(ParseDecimalError::InvalidExponent { .. })
        ));
    }

    #[test]
    fn parse_misplaced_sign_distinct_from_invalid_character() {
        // ADR-0029 item 2 / fd-7f1: a `+` or `-` in a position the
        // grammar does not permit is reported as MisplacedSign with
        // the offending byte position, distinct from a generic
        // InvalidCharacter at the same byte.
        assert!(matches!(
            Decimal32::parse_str("+-1", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 1 })
        ));
        assert!(matches!(
            Decimal32::parse_str("1+2", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 1 })
        ));
        assert!(matches!(
            Decimal32::parse_str("1e++3", RoundingMode::default()),
            Err(ParseDecimalError::MisplacedSign { position: 3 })
        ));
    }

    #[test]
    fn from_str_default_rounding() {
        let d: Decimal32 = "1.23".parse().unwrap();
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 2).unwrap(),
            Coefficient::try_new(123).unwrap(),
        ));
        assert_eq!(d.to_bits(), expected.to_bits());
    }
}
