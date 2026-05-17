//! Regression pin for fd-42l — `mul` (and the shared
//! `round_and_pack_finite` core) double-rounding on the deep-subnormal
//! path.
//!
//! When a result is both wider than 34 digits *and* underflows below
//! `qmin` (Etiny), the kernel rounded twice: once to 34-digit
//! precision, then again when `finalize_finite` shifted the
//! already-rounded coefficient to the subnormal quantum. A first-stage
//! round-up followed by a second-stage round-up landed one ULP above
//! the correctly-rounded value, even though the true first-dropped
//! digit was below one half. The fix drops to the wider of the
//! precision and subnormal-quantum requirements in a *single* rounding
//! step, with pre-rounding tininess driving UNDERFLOW (fd-99f
//! convention).
//!
//! `mul` reproducer (`NearestAway`):
//! `6.7059081871587041179E-11 × 3.514977514651561340069613573601788E-6134`
//! — exact `2.357111649318065904860662231814760471738…E-6144`; at
//! quantum `10^-6176` the tail is `.4717…` (round digit 4 < 5) so the
//! correctly-rounded result is `…814760E-6144`; the pre-fix kernel
//! returned `…814761E-6144`. Pinned bit-for-bit against the exact
//! oracle across every rounding direction (the defect is shared, so
//! `property_div` / `property_fma_oracle` exercise the same core).

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

const A_BITS: u128 = 0x2200_8000_0000_0003_A2A1_B7AA_FFC8_5F9B;
const B_BITS: u128 = 0x0802_6D4D_40E5_ADFA_1700_1063_20F9_79FC;

fn result_matches(got: Decimal128, want: &Expect) -> bool {
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
fn mul_deep_subnormal_is_exactly_correctly_rounded() {
    let a = Decimal128::from_bits(A_BITS);
    let b = Decimal128::from_bits(B_BITS);
    let da = parse_decimal(&format!("{a:e}")).expect("finite operand");
    let db = parse_decimal(&format!("{b:e}")).expect("finite operand");
    for &rm in MODES {
        let (got, gs) = a.mul(b, rm);
        let r = oracle::mul(&da, &db, Format::DECIMAL128, rm);
        assert!(
            result_matches(got, &r.value),
            "value mul({a:e}, {b:e}) rm={rm:?}: got {got:e}, oracle {}",
            r.decimal_string()
        );
        assert!(
            status_conformance_eq(gs, r.status),
            "status mul({a:e}, {b:e}) rm={rm:?}: got {gs:?}, oracle {:?}",
            r.status
        );
    }
}

#[test]
fn mul_fd_42l_nearest_away_does_not_over_round() {
    let a = Decimal128::from_bits(A_BITS);
    let b = Decimal128::from_bits(B_BITS);
    let (got, _) = a.mul(b, RoundingMode::NearestAway);
    let (_, coeff, _) = oracle::decode_decimal128(got.to_bits());
    assert_eq!(
        coeff.to_string(),
        "235711164931806590486066231814760",
        "mul NearestAway over-rounded: got {got:e}"
    );
}
