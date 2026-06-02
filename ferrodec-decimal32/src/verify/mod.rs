//! Kani verification harnesses.
//!
//! Compiled only under `cfg(kani)`. To run:
//!
//! ```sh
//! cargo kani --package ferrodec-decimal32 --features=fmt
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
mod canonical;
mod classify;
mod cmp;
mod div;
// DPD interchange codec lives behind the `dpd` feature
// (`src/dpd.rs`); the harness references it, so it is gated
// identically. `decimal64` has no DPD codec, so this port is
// decimal32-only.
#[cfg(feature = "dpd")]
mod dpd;
// `exp` / `ln` live behind the `exp-log` feature (ops/exp.rs); the
// harness references their Kani shims, so it is gated identically.
#[cfg(feature = "exp-log")]
mod exp;
mod fma;
mod from_parts;
// `sinh` … `atanh` live behind the `hyperbolic` feature
// (ops/hyper.rs); the harness references their Kani shims, so it is
// gated identically.
#[cfg(feature = "hyperbolic")]
mod hyper;
mod integral;
mod mul;
// `pow` / `cbrt` live behind the `pow` feature (ops/pow.rs); the
// harness references their Kani shims, so it is gated identically.
#[cfg(feature = "pow")]
mod pow;
// `quantize` … `next_down` are pure decimal (always compiled,
// `ops/quantum.rs` has no feature gate), so the harness is too.
mod quantum;
mod rem;
mod sqrt;
// `trig` lives behind the `trig` feature (ops/trig.rs); the harness
// references its Kani shims, so it is gated identically.
#[cfg(feature = "trig")]
mod trig;

use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

const NUM_OPERANDS: u8 = 10;

/// Map a small selector to one of ten representative `Decimal32`
/// values: NaN, sNaN, ±∞, ±0, ±1, ±MAX, ±MIN_POSITIVE. Together they
/// cover every IEEE 754 class plus the format extremes.
pub(crate) fn operand(idx: u8) -> Decimal32 {
    match idx {
        0 => Decimal32::NAN,
        1 => Decimal32::SIGNALING_NAN,
        2 => Decimal32::INFINITY,
        3 => Decimal32::NEG_INFINITY,
        4 => Decimal32::ZERO,
        5 => Decimal32::NEG_ZERO,
        6 => Decimal32::ONE,
        7 => Decimal32::NEG_ONE,
        8 => Decimal32::MAX,
        _ => Decimal32::MIN_POSITIVE,
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
