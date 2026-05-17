//! Exact correctly-rounded oracle for `Decimal32::fma` — the
//! sibling-FMA audit (fd-dpg).
//!
//! `Decimal128` carried the `fd-7nf` opposite-sign sub-ULP / over-
//! qmax-clamp FMA defect family; the parent's exact-oracle migration
//! (S3) is what surfaced it, so the siblings need the same instrument
//! to know whether they carry it too. This is the `Decimal128`
//! `tests/property_fma_oracle.rs` ported to the 64-bit format.
//!
//! `fma(a, b, c)` must equal `round(a·b + c)` under a *single*
//! rounding. The oracle forms `a·b + c` exactly (three scaled
//! integers) and the assertion is bit-for-bit, cohort included, with
//! an exact IEEE 754 status, across the full finite encoding domain
//! and every rounding direction. Operands span every encoding —
//! normals, subnormals, every cohort, the extreme exponents — by
//! sampling raw 64-bit patterns, exactly the alignment-window shapes
//! the static-window FMA defect family lived in.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, decode_decimal32, parse_decimal, Expect, Format};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

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

fn assert_exact_fma(
    a: Decimal32,
    b: Decimal32,
    c: Decimal32,
    rm: RoundingMode,
) -> Result<(), TestCaseError> {
    let (got, gs) = a.fma(b, c, rm);
    let da = parse_decimal(&format!("{a:e}")).expect("finite operand");
    let db = parse_decimal(&format!("{b:e}")).expect("finite operand");
    let dc = parse_decimal(&format!("{c:e}")).expect("finite operand");
    let r = oracle::fma(&da, &db, &dc, Format::DECIMAL32, rm);
    prop_assert!(
        result_matches(got, &r.value),
        "value fma({:e}, {:e}, {:e}) rm={:?}: got {:e} ({:#010x}), oracle {}",
        a,
        b,
        c,
        rm,
        got,
        got.to_bits(),
        r.decimal_string()
    );
    prop_assert!(
        status_conformance_eq(gs, r.status),
        "status fma({:e}, {:e}, {:e}) rm={:?}: got {:?}, oracle {:?}",
        a,
        b,
        c,
        rm,
        gs,
        r.status
    );
    Ok(())
}

/// Sanity: the cohort decoder is the exact inverse of this format's
/// `pack_finite`, so a false oracle disagreement cannot be a decoder
/// artifact. `1` decodes to `(+, 1, 0)`; `1.00` to `(+, 100, -2)`.
#[test]
fn decode_decimal32_is_pack_inverse() {
    let one = Decimal32::parse_str("1", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(decode_decimal32(one.to_bits()), (false, 1u32.into(), 0));
    let onehundredths = Decimal32::parse_str("1.00", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(
        decode_decimal32(onehundredths.to_bits()),
        (false, 100u32.into(), -2)
    );
    let neg = Decimal32::parse_str("-42E3", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(decode_decimal32(neg.to_bits()), (true, 42u32.into(), 3));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `fma` is the exact correctly-rounded fused multiply-add,
    /// bit-for-bit, across the full finite domain and every IEEE
    /// rounding direction.
    ///
    /// fd-dpg proved decimal32 carries the parent's `fd-7nf` FMA
    /// defect family too (P1 **fd-9fi**: `fma(-1e-101, 1e-101, -1e+27)`
    /// TowardNegative → `-2e27` instead of `-1.000001e27`, the
    /// dominant operand not re-cohorted before the directed round-up;
    /// and a distinct overlap defect where the early-return discarded
    /// a side that overlapped the dominant precision window). fd-9fi
    /// fixed both (`fma::extend_to_u128_cap` plus the overlap
    /// working-quantum combine); this sweep is now green and stays
    /// active as the regression guard.
    #[test]
    fn fma_is_exactly_correctly_rounded(
        a in finite(),
        b in finite(),
        c in finite(),
        rm_idx in 0u8..5,
    ) {
        assert_exact_fma(a, b, c, MODES[rm_idx as usize])?;
    }
}
