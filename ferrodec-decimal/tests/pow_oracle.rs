//! An independent oracle for [`Decimal::power`]'s rounding (the ADR-0026
//! pattern): compute the power at a much higher precision, then round that down
//! to the target precision, and assert it matches the directly rounded power.
//!
//! The high-precision result is the true value to far more digits than the
//! target needs, so rounding it down is the correctly rounded answer (a double
//! rounding could disagree only when the true value lies within `10^-50` of a
//! target-precision rounding boundary, which no finite sample reaches). This is
//! structurally independent of the kernel's bounded Ziv loop: it exercises the
//! same operation at a second, much wider precision and a separate rounding
//! step, so a wrong Ziv decision at the target precision is caught. Power is
//! correctly rounded by construction here, stronger than the libmpdec reference
//! (which is only "almost always" correctly rounded), so the differential
//! against it uses a one-ulp band rather than this exact check.

#![cfg(feature = "fmt")]

use ferrodec_decimal::{Context, Decimal, Rounding};

const MODES: [Rounding; 5] = [
    Rounding::HalfEven,
    Rounding::Up,
    Rounding::Down,
    Rounding::Ceiling,
    Rounding::Floor,
];

fn parse(s: &str) -> Decimal {
    Decimal::parse_str(s).expect("valid")
}

#[test]
fn power_matches_high_precision_oracle() {
    // (base, exponent): positive bases, a mix of non-integer and integer
    // exponents, magnitudes that stay well inside the exponent range.
    let sample: &[(&str, &str)] = &[
        ("2", "0.5"),
        ("2", "10"),
        ("2", "-3"),
        ("3", "3.7"),
        ("10", "2.5"),
        ("0.5", "0.5"),
        ("7", "-2.3"),
        ("123.456", "1.8"),
        ("0.001", "0.4"),
        ("99.9", "-1.5"),
        ("1.5", "100"),
        ("5", "0.333"),
        ("48.2", "2.71"),
        ("0.7", "-0.7"),
        ("1000", "0.6"),
        ("2.718281828", "3.14159"),
        ("9999", "1.1"),
        ("0.123", "-2"),
        ("6.022", "4.4"),
        ("88", "-0.25"),
    ];

    for &(b, e) in sample {
        let x = parse(b);
        let y = parse(e);
        for prec in [7u32, 16, 28, 34] {
            for mode in MODES {
                let hi = Context::new(
                    core::num::NonZeroU32::new(prec + 50).unwrap(),
                    99_999,
                    -99_999,
                    mode,
                );
                let lo = Context::new(
                    core::num::NonZeroU32::new(prec).unwrap(),
                    99_999,
                    -99_999,
                    mode,
                );
                // High-precision power, then rounded down to the target.
                let oracle = x.power(&y, &hi).0.plus(&lo).0;
                let direct = x.power(&y, &lo).0;
                assert_eq!(
                    direct.to_string(),
                    oracle.to_string(),
                    "power({b}, {e}) p{prec} {mode:?}: direct vs high-precision oracle"
                );
            }
        }
    }
}
