//! Exact correctly-rounded oracle for `Decimal64::fma` — the
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

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, decode_decimal64, parse_decimal, Expect, Format};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn result_matches(got: Decimal64, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = decode_decimal64(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// Every finite 64-bit encoding: normals, subnormals, all cohorts,
/// the extreme exponents.
fn finite() -> impl Strategy<Value = Decimal64> {
    any::<u64>()
        .prop_map(Decimal64::from_bits)
        .prop_filter("finite", |d| d.is_finite())
}

fn assert_exact_fma(
    a: Decimal64,
    b: Decimal64,
    c: Decimal64,
    rm: RoundingMode,
) -> Result<(), TestCaseError> {
    let (got, gs) = a.fma(b, c, rm);
    let da = parse_decimal(&format!("{a:e}")).expect("finite operand");
    let db = parse_decimal(&format!("{b:e}")).expect("finite operand");
    let dc = parse_decimal(&format!("{c:e}")).expect("finite operand");
    let r = oracle::fma(&da, &db, &dc, Format::DECIMAL64, rm);
    prop_assert!(
        result_matches(got, &r.value),
        "value fma({:e}, {:e}, {:e}) rm={:?}: got {:e} ({:#018x}), oracle {}",
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
fn decode_decimal64_is_pack_inverse() {
    let one = Decimal64::parse_str("1", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(decode_decimal64(one.to_bits()), (false, 1u32.into(), 0));
    let onehundredths = Decimal64::parse_str("1.00", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(
        decode_decimal64(onehundredths.to_bits()),
        (false, 100u32.into(), -2)
    );
    let neg = Decimal64::parse_str("-42E5", RoundingMode::NearestEven)
        .unwrap()
        .0;
    assert_eq!(decode_decimal64(neg.to_bits()), (true, 42u32.into(), 5));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `fma` is the exact correctly-rounded fused multiply-add,
    /// bit-for-bit, across the full finite domain and every IEEE
    /// rounding direction.
    ///
    /// `#[ignore]` — fd-dpg's purpose was to find out whether the
    /// siblings carry the parent's `fd-7nf` FMA defect family; this
    /// sweep proved they do (filed P1 **fd-9fi**: tiny opposite-sign
    /// product with a dominant same-sign addend under a directed mode
    /// gives a gross magnitude error, e.g.
    /// `fma(1e-398, -1e-398, -1e+114)` TowardNegative → `-2e114`
    /// instead of `-1.000000000000001e114`). The sweep is correct; the
    /// kernel is not. It is quarantined, not deleted, so the eventual
    /// `fd-9fi` fix lands by *removing this attribute* and watching it
    /// go green. It must NOT be un-ignored without the fix.
    #[ignore = "fd-9fi: sibling fma carries the fd-7nf family; \
                un-ignore with the kernel fix"]
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
