//! Kani harnesses for `Decimal128::fma`.
//!
//! Same strategy as the mul / addsub harnesses (ADR-0016): every
//! assertion routes through the loop-free
//! [`Decimal128::fma_special_only_for_kani`] shim, so CBMC never
//! encodes the U256 product / alignment / round pipeline. The
//! finite-finite-finite path's correctness is carried by the exact
//! oracle in `tests/property_fma_oracle.rs` and the S6 / S7
//! rounding-decision proofs. Mirrors `src/verify/mul.rs` and the
//! sibling crates' `verify/fma.rs`.

use crate::status::RoundingMode;
use crate::Decimal128;

const NUM_OPERANDS: u8 = 10;

fn operand(idx: u8) -> Decimal128 {
    match idx {
        0 => Decimal128::NAN,
        1 => Decimal128::SIGNALING_NAN,
        2 => Decimal128::INFINITY,
        3 => Decimal128::NEG_INFINITY,
        4 => Decimal128::ZERO,
        5 => Decimal128::NEG_ZERO,
        6 => Decimal128::ONE,
        7 => Decimal128::NEG_ONE,
        8 => Decimal128::MAX,
        _ => Decimal128::MIN,
    }
}

// NOTE (ADR-0016 boundary): a `…_resolves_iff…` harness like
// `src/verify/mul.rs` has — covering *every* operand combination — is
// deliberately omitted here. Unlike `mul_special_cases`, the
// `fma_special_cases` zero-product branch returns the addend `c`
// re-quantised to the §6.3 preferred exponent, a step that routes
// through `U256::decimal_digit_count` (the unbounded loop CBMC cannot
// encode — the very reason the shim policy exists). The harnesses
// below each `assume` their way onto a loop-free special outcome
// (NaN / sNaN / 0·∞); the finite-and-zero-product path's correctness
// is carried by the exact oracle in `tests/property_fma_oracle.rs`.

/// `0 × ∞ + c` raises `INVALID` with a NaN result, for any `c` that
/// does not itself force `INVALID` (signaling NaN excluded).
#[kani::proof]
#[kani::unwind(80)]
fn fma_zero_times_infinity_invalid() {
    let zero_neg: bool = kani::any();
    let inf_neg: bool = kani::any();
    let a_is_zero: bool = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ci < NUM_OPERANDS);
    let c = operand(ci);
    kani::assume(!c.is_signaling_nan());

    let zero = if zero_neg {
        Decimal128::NEG_ZERO
    } else {
        Decimal128::ZERO
    };
    let inf = if inf_neg {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };
    let (a, b) = if a_is_zero { (zero, inf) } else { (inf, zero) };

    let (r, s) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("0 × ∞ resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// A signaling NaN anywhere in `(a, b, c)` raises `INVALID`.
#[kani::proof]
#[kani::unwind(80)]
fn fma_snan_anywhere_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(ci < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    let c = operand(ci);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan() || c.is_signaling_nan());

    let (_, s) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("sNaN path resolved by special_cases");
    assert!(s.invalid());
}

/// Whenever any operand is NaN (and `0 × ∞` does not apply), the shim
/// returns `Some` with a NaN result.
#[kani::proof]
#[kani::unwind(80)]
fn fma_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(ci < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    let c = operand(ci);
    kani::assume(a.is_nan() || b.is_nan() || c.is_nan());

    let (r, _) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}
