//! Kani verification harnesses.
//!
//! Compiled only under `cfg(kani)`. To run:
//!
//! ```sh
//! cargo kani --package ferrodec-decimal64 --features=fmt
//! ```
//!
//! Each harness function is annotated with `#[kani::proof]` and uses
//! `kani::any()` to introduce symbolic inputs, with `kani::assume()` to
//! constrain to a 10-constant operand set so the SAT problem stays
//! tractable. The strategy mirrors the `ferrodec` Decimal128 verify
//! tree's "special-case-only" approach: we prove no-panic and
//! IEEE 754 special-case propagation across NaN / ±∞ / ±0 / ±1 /
//! ±MAX / ±MIN_POSITIVE inputs, leaving correctness on the
//! finite-finite arithmetic path to the property tests.

mod addsub;
mod cmp;
mod div;
mod fma;
mod mul;
mod sqrt;

use crate::decimal::Decimal64;
use ferrodec_ieee::RoundingMode;

const NUM_OPERANDS: u8 = 10;

/// Map a small selector to one of ten representative `Decimal64`
/// values: NaN, sNaN, ±∞, ±0, ±1, ±MAX, ±MIN_POSITIVE. Together they
/// cover every IEEE 754 class plus the format extremes.
pub(crate) fn operand(idx: u8) -> Decimal64 {
    match idx {
        0 => Decimal64::NAN,
        1 => Decimal64::SIGNALING_NAN,
        2 => Decimal64::INFINITY,
        3 => Decimal64::NEG_INFINITY,
        4 => Decimal64::ZERO,
        5 => Decimal64::NEG_ZERO,
        6 => Decimal64::ONE,
        7 => Decimal64::NEG_ONE,
        8 => Decimal64::MAX,
        _ => Decimal64::MIN_POSITIVE,
    }
}

/// Map a small selector to one of the five IEEE 754-2019 rounding
/// modes.
pub(crate) fn rm_from_u8(x: u8) -> RoundingMode {
    match x {
        0 => RoundingMode::NearestEven,
        1 => RoundingMode::NearestAway,
        2 => RoundingMode::TowardZero,
        3 => RoundingMode::TowardPositive,
        _ => RoundingMode::TowardNegative,
    }
}
