//! Regression pin for fd-99f — FMA `sub_ulp_eff_sub_c_dominates`
//! missing `UNDERFLOW`.
//!
//! Surfaced by the S4 exact-oracle migration. On the `c_too_wide`
//! effective-subtraction path the true value is `c·10^qc − epsilon`
//! (epsilon a tiny opposite-sign product residue). When that true
//! value is tiny (below `10^Emin`) but rounds back up to exactly the
//! smallest normal, `round_and_pack_finite` only sees the exactly-
//! representable `c` and its after-rounding subnormal test does not
//! fire, so `UNDERFLOW` was dropped. IEEE 754 §7.5 / GDA detect
//! tininess on the value *before* rounding (the convention the exact
//! oracle pins against decTest), so the flag must be raised.
//!
//! Reproducer: `1e-6176 fma -1e-6176 1.00e-6143` (`NearestEven`). The
//! product is `-1e-12352`; `c − epsilon` is just below `1e-6143` and
//! rounds to `1e-6143`, which must carry `Underflow | Inexact`.

#![cfg(feature = "fmt")]

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn matches(got: Decimal128, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = oracle::decode_decimal128(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

#[test]
fn fma_c_dominant_eff_sub_tiny_raises_underflow() {
    let a = Decimal128::from_bits(0x00000000000000000000000000000001); // 1e-6176
    let b = Decimal128::from_bits(0x80000000000000000000000000000001); // -1e-6176
    let c = Decimal128::from_bits(0x0007C000000000000000000000000064); // 1.00e-6143
    for &rm in MODES {
        let (got, gs) = a.fma(b, c, rm);
        let da = parse_decimal(&format!("{a:e}")).unwrap();
        let db = parse_decimal(&format!("{b:e}")).unwrap();
        let dc = parse_decimal(&format!("{c:e}")).unwrap();
        let r = oracle::fma(&da, &db, &dc, Format::DECIMAL128, rm);
        assert!(
            matches(got, &r.value),
            "value fma({a:e}, {b:e}, {c:e}) rm={rm:?}: got {got:e}, oracle {}",
            r.decimal_string()
        );
        assert!(
            status_conformance_eq(gs, r.status),
            "status fma({a:e}, {b:e}, {c:e}) rm={rm:?}: got {gs:?}, oracle {:?}",
            r.status
        );
    }
}
