//! Fuzz target: arbitrary `(u64, u64)` bit-pattern pairs through the
//! basic arithmetic ops (add, sub, mul, div, rem). Asserts no panic
//! plus a few algebraic identities.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec_decimal64::{Decimal64, RoundingMode};

fuzz_target!(|data: (u64, u64)| {
    let (bits_a, bits_b) = data;
    let a = Decimal64::from_bits(bits_a);
    let b = Decimal64::from_bits(bits_b);
    let rm = RoundingMode::NearestEven;

    // No panic on any of the basic ops.
    let _ = a.add(b, rm);
    let _ = a.sub(b, rm);
    let _ = a.mul(b, rm);
    let _ = a.div(b, rm);
    let _ = a.rem(b, rm);

    // a + 0 = a (numerically) for non-NaN finite a.
    if a.is_finite() && !a.is_nan() {
        let (sum, _) = a.add(Decimal64::ZERO, rm);
        if !sum.is_nan() && !a.is_nan() {
            let (cmp, _) = sum.partial_cmp(a);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "a + 0 != a: a={a:?}, sum={sum:?}"
            );
        }
    }

    // a - a = ±0 (per IEEE 754 §6.3) for non-NaN, non-Inf a.
    if a.is_finite() && !a.is_nan() {
        let (diff, _) = a.sub(a, rm);
        // -∞ - -∞ → NaN+INVALID handled by sub; we already gated with is_finite.
        if !diff.is_nan() {
            assert!(diff.is_zero(), "a - a should be zero, got {diff:?} for a={a:?}");
        }
    }
});
