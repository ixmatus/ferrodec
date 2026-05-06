//! Shared helpers for the property-test files. Lives at
//! `tests/common/mod.rs` so each `tests/property_*.rs` file can pull
//! it in via `mod common;` without Cargo treating it as a separate
//! integration-test binary.
//!
//! A `#[allow(dead_code)]` blanket sits at the top because the helpers
//! here are split across consumers — `bigfloat_to_decimal_string` only
//! makes sense when the test file uses astro-float as an oracle, while
//! `within_ulps` is consumed by every transcendental property test.
//! Without the blanket, files that import only one helper would surface
//! warnings for the others.

#![allow(dead_code)]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec::{Decimal128, RoundingMode};

/// Parse a decimal literal at default round-half-even, panicking on
/// invalid input. The shape every property test wants for hand-curated
/// reference values.
pub fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

/// `true` if `got` matches `want` to within `ulps` units in the last
/// place at 34-digit precision. For results near zero, the tolerance
/// switches to an absolute `ulps · 10^{-30}` bound (since relative
/// error is undefined at zero).
pub fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-30"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        );
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-33"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(
        cmp,
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

/// Render an astro-float `BigFloat` as a Decimal128-parseable string
/// at `digits` significant digits. Used by every oracle cross-check
/// that converts a `BigFloat` back to a Decimal128 for comparison.
pub fn bigfloat_to_decimal_string(v: &BigFloat, cc: &mut Consts, digits: usize) -> String {
    let (sign, mantissa, exp) = v
        .convert_to_radix(Radix::Dec, AfRm::ToEven, cc)
        .expect("convert to decimal");
    if mantissa.is_empty() || mantissa.iter().all(|&d| d == 0) {
        return "0".to_string();
    }
    let take = digits.min(mantissa.len());
    let digit_str: String = mantissa[..take]
        .iter()
        .map(|&d| char::from(b'0' + d))
        .collect();
    let scale = exp - take as i32;
    let sign_str = if matches!(sign, Sign::Neg) { "-" } else { "" };
    format!("{sign_str}{digit_str}e{scale}")
}
