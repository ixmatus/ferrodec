//! Fuzz target: `Decimal128::total_cmp` (the IEEE 754:2019 §5.10
//! totalOrder predicate) and `compare_total_magnitude` over arbitrary
//! bit patterns.
//!
//! Kani proves antisymmetry on the same-cohort same-sign finite-finite
//! domain (`src/verify/cmp.rs::total_cmp_antisymmetric_finite_same_cohort_same_sign`).
//! The remaining surface — different cohorts, NaN payloads, ±∞ pairs —
//! sits beyond CBMC's reach because `magnitude_cmp`'s pow10 scaling
//! blocks SMT termination. libFuzzer covers that long tail.
//!
//! Invariants asserted:
//!
//! 1. **Reflexivity**: `a.total_cmp(a) == Equal` and
//!    `a.compare_total_magnitude(a) == Equal` for every bit pattern,
//!    including NaN payloads.
//! 2. **Antisymmetry**: `a.total_cmp(b)` is the reverse of
//!    `b.total_cmp(a)`. Same for `compare_total_magnitude`.

#![no_main]

use core::cmp::Ordering;

use libfuzzer_sys::fuzz_target;

use ferrodec::Decimal128;

#[derive(arbitrary::Arbitrary, Debug)]
struct Pair {
    a: u128,
    b: u128,
}

fn reverse(o: Ordering) -> Ordering {
    match o {
        Ordering::Less => Ordering::Greater,
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => Ordering::Less,
    }
}

fuzz_target!(|p: Pair| {
    let a = Decimal128::from_bits(p.a);
    let b = Decimal128::from_bits(p.b);

    // Reflexivity.
    assert_eq!(
        a.total_cmp(a),
        Ordering::Equal,
        "total_cmp not reflexive: a bits {:#034x}",
        p.a
    );
    assert_eq!(
        a.compare_total_magnitude(a),
        Ordering::Equal,
        "compare_total_magnitude not reflexive: a bits {:#034x}",
        p.a
    );

    // Antisymmetry.
    assert_eq!(
        a.total_cmp(b),
        reverse(b.total_cmp(a)),
        "total_cmp not antisymmetric: a bits {:#034x}, b bits {:#034x}",
        p.a,
        p.b
    );
    assert_eq!(
        a.compare_total_magnitude(b),
        reverse(b.compare_total_magnitude(a)),
        "compare_total_magnitude not antisymmetric: a bits {:#034x}, b bits {:#034x}",
        p.a,
        p.b
    );
});
