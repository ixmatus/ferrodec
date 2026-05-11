//! Kani harnesses for [`Decimal128::try_new`].
//!
//! `try_new(coefficient, exponent)` is a thin wrapper over
//! `bid::pack_finite` plus two range checks. SMT can dispatch the
//! whole `(i128, i32)` domain in seconds.

use crate::bid::{classify_bits, Class, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT};
use crate::{Decimal128, Decimal128BuildError};

/// `try_new(coef, exp)` returns `Ok` exactly when both bounds are
/// satisfied: `|coef| < 10^34` and the biased exponent
/// `exp + BIAS` is in `[0, BIASED_EXP_MAX]`. The decoded result then
/// matches the inputs verbatim.
#[kani::proof]
fn try_new_in_range_succeeds() {
    let coefficient: i128 = kani::any();
    let exponent: i32 = kani::any();

    let mag = (coefficient as i128).unsigned_abs();
    kani::assume(mag < COEFFICIENT_LIMIT);
    let biased = (exponent as i64) + (BIAS as i64);
    kani::assume(biased >= 0);
    kani::assume(biased <= BIASED_EXP_MAX as i64);

    let result = Decimal128::try_new(coefficient, exponent);
    let d = result.expect("in-range inputs must succeed");

    match classify_bits(d.to_bits()) {
        Class::Zero { sign, biased_exp } => {
            assert!(coefficient == 0);
            assert!(biased_exp == biased as u32);
            assert!(sign == (coefficient < 0));
        }
        Class::Finite {
            sign,
            biased_exp,
            coefficient: c,
        } => {
            assert!(c == mag);
            assert!(biased_exp == biased as u32);
            assert!(sign == (coefficient < 0));
        }
        _ => kani::cover!(false, "try_new should never produce NaN/Inf"),
    }
}

/// `try_new` rejects out-of-range coefficients with the
/// `CoefficientOutOfRange` variant, and out-of-range exponents with
/// `ExponentOutOfRange`. The check order matters: coefficient is
/// checked first, so a doubly-bad input reports the coefficient error.
#[kani::proof]
fn try_new_coefficient_out_of_range() {
    let coefficient: i128 = kani::any();
    let exponent: i32 = kani::any();

    let mag = (coefficient as i128).unsigned_abs();
    kani::assume(mag >= COEFFICIENT_LIMIT);

    let result = Decimal128::try_new(coefficient, exponent);
    assert!(matches!(
        result,
        Err(Decimal128BuildError::CoefficientOutOfRange)
    ));
}

/// Out-of-range exponent (with in-range coefficient) reports
/// `ExponentOutOfRange`.
#[kani::proof]
fn try_new_exponent_out_of_range() {
    let coefficient: i128 = kani::any();
    let exponent: i32 = kani::any();

    let mag = (coefficient as i128).unsigned_abs();
    kani::assume(mag < COEFFICIENT_LIMIT);
    let biased = (exponent as i64) + (BIAS as i64);
    kani::assume(biased < 0 || biased > BIASED_EXP_MAX as i64);

    let result = Decimal128::try_new(coefficient, exponent);
    assert!(matches!(
        result,
        Err(Decimal128BuildError::ExponentOutOfRange)
    ));
}
