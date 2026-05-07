//! Fuzz target: `Decimal128` round-to-integer family on arbitrary bit
//! patterns.
//!
//! Asserted invariants:
//!
//! 1. **Panic-freedom** across every input.
//! 2. **Idempotence** — rounding twice produces the bit-identical
//!    result of rounding once. Holds even when the input is non-finite
//!    (NaN passes through, ±∞ pass through).
//! 3. **Integer-ness** — for finite, non-zero results, `is_integer()`
//!    must return `true` (the round-to-integer family lands on an
//!    integer cohort by definition).

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec::{Decimal128, RoundingMode};

fuzz_target!(|bits: u128| {
    let x = Decimal128::from_bits(bits);
    let rm = RoundingMode::NearestEven;

    let candidates: [(&str, Decimal128); 7] = [
        ("floor", x.floor()),
        ("ceil", x.ceil()),
        ("trunc", x.trunc()),
        ("round", x.round()),
        ("round_ties_even", x.round_ties_even()),
        ("round_to_integral", x.round_to_integral(rm).0),
        ("round_to_integral_exact", x.round_to_integral_exact(rm).0),
    ];

    for (name, y) in candidates {
        // Idempotence — the second pass must be bit-identical to the
        // first. The fixed-point check covers all six op shapes.
        let y2 = match name {
            "floor" => y.floor(),
            "ceil" => y.ceil(),
            "trunc" => y.trunc(),
            "round" => y.round(),
            "round_ties_even" => y.round_ties_even(),
            "round_to_integral" => y.round_to_integral(rm).0,
            "round_to_integral_exact" => y.round_to_integral_exact(rm).0,
            _ => unreachable!(),
        };
        assert_eq!(
            y.to_bits(),
            y2.to_bits(),
            "{name} not idempotent: input bits {bits:#034x}, first pass {:#034x}, second pass {:#034x}",
            y.to_bits(),
            y2.to_bits()
        );

        // Integer-ness for finite results. NaN / ±∞ propagate; nothing
        // else to assert there.
        if y.is_finite() {
            assert!(
                y.is_integer(),
                "{name} returned non-integer finite value: input bits {bits:#034x}, result bits {:#034x}",
                y.to_bits()
            );
        }
    }
});
