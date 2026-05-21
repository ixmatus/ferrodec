//! Exact correctly-rounded oracle for `Decimal32::rem_near` (fd-pvu,
//! ADR-0027).
//!
//! `rem_near` is the IEEE 754-2019 §5.3.1 nearest-even remainder, the
//! sibling analogue of `Decimal128::rem_near` (the parent op renamed
//! from bare `rem` in 2.0 per ADR-0027). The IEEE remainder is
//! *always exact* (`r = a − n·b`, a difference of scaled integers),
//! so the exact integer oracle (`ferrodec_test_support::oracle::rem`,
//! the same one that validates the parent) asserts `rem_near(a, b)`
//! bit-for-bit, cohort included, with an exact IEEE 754 status,
//! across the full finite 32-bit encoding domain (non-zero `b`). This
//! is the `Decimal128` `tests/property_rem.rs` oracle leg ported to
//! the 32-bit format. The parent already pins the truncated
//! `rem_trunc` semantics; here the new nearest-even op is the unit
//! under test.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, decode_decimal32, parse_decimal, Expect, Format};
use proptest::prelude::*;

fn result_matches(got: Decimal32, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = decode_decimal32(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// Every finite 32-bit encoding: normals, subnormals, all cohorts,
/// the extreme exponents.
fn finite() -> impl Strategy<Value = Decimal32> {
    any::<u32>()
        .prop_map(Decimal32::from_bits)
        .prop_filter("finite", |d| d.is_finite())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `rem_near(a, b)` equals the exact IEEE 754 §5.3.1 nearest-even
    /// remainder, bit-for-bit and status-for-status, over the full
    /// finite domain (non-zero `b`). The result is always exact, so
    /// the rounding mode handed to the oracle is immaterial — it only
    /// fixes the cohort, which the GDA `min(exp a, exp b)` ideal
    /// exponent already determines, the same property the parent test
    /// relies on.
    #[test]
    fn rem_near_is_exactly_correctly_rounded(a in finite(), b in finite()) {
        prop_assume!(!b.is_zero());
        let (got, gs) = a.rem_near(b);
        let da = parse_decimal(&format!("{a:e}")).expect("finite operand");
        let db = parse_decimal(&format!("{b:e}")).expect("finite operand");
        let r = oracle::rem(&da, &db, Format::DECIMAL32, RoundingMode::NearestEven);
        prop_assert!(
            result_matches(got, &r.value),
            "value rem_near({a:e}, {b:e}): got {got:e}, oracle {}",
            r.decimal_string()
        );
        prop_assert!(
            status_conformance_eq(gs, r.status),
            "status rem_near({a:e}, {b:e}): got {gs:?}, oracle {:?}",
            r.status
        );
    }
}
