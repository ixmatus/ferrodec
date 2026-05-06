//! Fuzz target: arbitrary `(Decimal128, Decimal128)` triples through
//! `add` / `sub` / `mul` / `div`. Asserts:
//!
//! * No panic on any defined input (including sNaN and overflow paths).
//! * Identity invariants: `a + 0` is numerically equal to `a` for
//!   non-NaN finite `a`; `a * 1` numerically equals `a`; `a - a == 0`
//!   for finite `a`.
//!
//! Kani proves these for the special-case dispatch on bounded operands;
//! libFuzzer covers the un-bounded space.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec::{Decimal128, RoundingMode};

#[derive(arbitrary::Arbitrary, Debug)]
struct Pair {
    a: u128,
    b: u128,
}

fn nums_equal(x: Decimal128, y: Decimal128) -> bool {
    if x.is_nan() {
        return y.is_nan();
    }
    matches!(x.partial_cmp(y).0, Some(core::cmp::Ordering::Equal))
}

fuzz_target!(|p: Pair| {
    let a = Decimal128::from_bits(p.a);
    let b = Decimal128::from_bits(p.b);
    let rm = RoundingMode::NearestEven;

    let _ = a.add(b, rm);
    let _ = a.sub(b, rm);
    let _ = a.mul(b, rm);
    let _ = a.div(b, rm);

    if a.is_finite() && !a.is_nan() {
        let (sum, _) = a.add(Decimal128::ZERO, rm);
        assert!(nums_equal(sum, a), "a + 0 != a (a = {a:?})");

        let (prod, _) = a.mul(Decimal128::ONE, rm);
        assert!(nums_equal(prod, a), "a * 1 != a (a = {a:?})");

        let (diff, _) = a.sub(a, rm);
        assert!(diff.is_zero(), "a - a != 0 (a = {a:?})");
    }
});
