//! Exact `Decimal64` cross-check oracle for the `Decimal32`
//! arithmetic correctness slice.
//!
//! Every finite `Decimal32` value is exactly representable in
//! `Decimal64`: 7 significand digits and an exponent range strictly
//! inside Decimal64's 16 digits and wider exponent range. Widening is
//! therefore lossless. Render the `Decimal32` to its exact decimal
//! string and parse that into a `Decimal64`. The `Decimal64`
//! operation, brought to full conformance in its 1.4.0 slice, is the
//! oracle: round its result back to 7 digits and it must equal the
//! `Decimal32` operation's result, value and cohort.
//!
//! Oracle exactness, per operation:
//!
//! - `mul`: the exact product of two 7-digit coefficients is at most
//!   14 digits, representable in Decimal64's 16 exactly, so the
//!   Decimal64 product is exact and the round back to Decimal32 is
//!   the correctly rounded answer. No double rounding.
//! - `rem`: the remainder is exact and no larger in magnitude than
//!   the operands, representable in Decimal64 exactly. No double
//!   rounding.
//! - `add` / `sub`: the exact result can exceed 16 significand
//!   digits (a tiny addend many orders below the dominant operand).
//!   The Decimal64 step rounds first, so a double rounding divergence
//!   from the directly correctly rounded Decimal32 result is possible,
//!   but only when the exact result needs more than 16 digits AND
//!   lands within half a Decimal32 ULP of a rounding boundary after
//!   the first rounding. That narrow regime is exactly where the
//!   suspected static-alignment-window defect would live, so a
//!   mismatch here is a signal to investigate (Phase 0b, beads
//!   fd-pab), not noise to suppress.
//!
//! Status is intentionally not cross-checked. Decimal32 and Decimal64
//! have different exponent ranges, so OVERFLOW, UNDERFLOW, and
//! SUBNORMAL legitimately differ between the two formats for the same
//! operands. Results whose magnitude falls outside Decimal32's
//! representable range are skipped here for the same reason: this
//! oracle pins the in-range rounded value and its cohort, which is
//! the contract the alignment and rounding paths must satisfy.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_decimal64::Decimal64;
use proptest::prelude::*;

/// All five IEEE 754 rounding directions.
const ROUNDING_MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

/// Widen a finite `Decimal32` to `Decimal64` losslessly through its
/// exact decimal string.
fn widen(d: Decimal32) -> Decimal64 {
    Decimal64::parse_str(&d.to_string(), RoundingMode::NearestEven)
        .expect("the exact decimal string of a finite Decimal32 parses into Decimal64")
        .0
}

/// Round a `Decimal64` result back to `Decimal32` at `rm`, through
/// its exact decimal string. Returns `None` when the value lies
/// outside Decimal32's representable range (overflow / underflow
/// domain): that is a status-range concern, deliberately out of
/// scope for this value oracle.
fn narrow(d: Decimal64, rm: RoundingMode) -> Option<Decimal32> {
    Decimal32::parse_str(&d.to_string(), rm)
        .ok()
        .map(|(v, _)| v)
}

/// Value plus cohort equality. NaN matches NaN (payload and sign are
/// not part of the arithmetic-value contract checked here);
/// infinities match by sign; finite values match by their exact
/// canonical string, which carries the cohort exponent.
fn same_result(a: Decimal32, b: Decimal32) -> bool {
    if !a.is_finite() || !b.is_finite() {
        let (sa, sb) = (a.to_string(), b.to_string());
        let (nan_a, nan_b) = (sa.contains("NaN"), sb.contains("NaN"));
        if nan_a || nan_b {
            return nan_a && nan_b;
        }
        return sa == sb;
    }
    a.to_string() == b.to_string()
}

/// Arbitrary finite `Decimal32`. Non-canonical encodings are fine:
/// both the operation and `widen` decode the bits the same way, so
/// the oracle stays consistent. Specials are out of scope (status
/// differs by format) and filtered out.
fn finite_d32() -> impl Strategy<Value = Decimal32> {
    any::<u32>()
        .prop_map(Decimal32::from_bits)
        .prop_filter("finite operands only", |d| d.is_finite())
}

/// `mul` is the exact oracle: the product of two 7-digit
/// coefficients fits Decimal64's 16 digits, so there is no double
/// rounding and it is correct on `main`. This active block guards
/// the bridge itself.
macro_rules! crosscheck_active {
    ($name:ident, $op:ident) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(4096))]
            #[test]
            fn $name(
                a in finite_d32(),
                b in finite_d32(),
                rmi in 0usize..ROUNDING_MODES.len(),
            ) {
                let rm = ROUNDING_MODES[rmi];
                let (actual, _) = a.$op(b, rm);
                let oracle = narrow(widen(a).$op(widen(b), rm).0, rm);
                prop_assume!(oracle.is_some());
                let expected = oracle.unwrap();
                prop_assert!(
                    same_result(actual, expected),
                    "{}({a}, {b}, {rm:?}): Decimal32 -> {actual}, Decimal64 \
oracle -> {expected} (a_bits={:#010x} b_bits={:#010x})",
                    stringify!($op), a.to_bits(), b.to_bits()
                );
            }
        }
    };
}

/// `add` / `sub` / `rem` are red on `main`. Phase 0a (beads fd-ac6)
/// lands this harness as clean green infrastructure; Phase 0b
/// (fd-pab) pins the reproducers in `KNOWN_ISSUES`; each H tier fix in
/// Phase 2..N (fd-6tl) removes the matching `#[ignore]` and the block
/// becomes the permanent regression guard for that fix. Do not delete
/// an `#[ignore]` reason without a landed fix.
macro_rules! crosscheck_ignored {
    ($name:ident, $op:ident, $reason:literal) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(4096))]
            #[test]
            #[ignore = $reason]
            fn $name(
                a in finite_d32(),
                b in finite_d32(),
                rmi in 0usize..ROUNDING_MODES.len(),
            ) {
                let rm = ROUNDING_MODES[rmi];
                let (actual, _) = a.$op(b, rm);
                let oracle = narrow(widen(a).$op(widen(b), rm).0, rm);
                prop_assume!(oracle.is_some());
                let expected = oracle.unwrap();
                prop_assert!(
                    same_result(actual, expected),
                    "{}({a}, {b}, {rm:?}): Decimal32 -> {actual}, Decimal64 \
oracle -> {expected} (a_bits={:#010x} b_bits={:#010x})",
                    stringify!($op), a.to_bits(), b.to_bits()
                );
            }
        }
    };
}

crosscheck_active!(mul_matches_decimal64, mul);
crosscheck_active!(add_matches_decimal64, add);
crosscheck_active!(sub_matches_decimal64, sub);
crosscheck_ignored!(
    rem_matches_decimal64,
    rem,
    "red on main: rem static MAX_SAFE_SHIFT raises spurious INVALID \
     (e.g. rem(4.194304E+33, -3.145728E+18) -> NaN, want \
     1.048576E+18). Un-ignore with the H tier rem fix (fd-6tl)."
);

/// Helper for the explicit neighborhood probes: parse a literal that
/// is known to be inside Decimal32's range.
fn d32(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .expect("in-range Decimal32 literal")
        .0
}

/// The `add` / `sub` static-alignment-window neighborhood: a small
/// coefficient at a large exponent combined with a normal-magnitude
/// operand, where `addsub.rs`'s fixed `ALIGN_LIMIT` window would
/// truncate the lower operand. Each pair is held to the Decimal64
/// oracle across all five rounding modes.
#[test]
fn addsub_small_coef_large_gap_neighborhood() {
    let probes: &[(&str, &str)] = &[
        ("1E+90", "1E+84"),
        ("1E+90", "1234567E+70"),
        ("9.999999E+96", "1E+90"),
        ("1234567E+10", "7654321E-20"),
        ("1E-89", "1E-95"),
        ("5000001E+40", "1E+34"),
    ];
    for (sa, sb) in probes {
        let (a, b) = (d32(sa), d32(sb));
        for rm in ROUNDING_MODES {
            let (got_add, _) = a.add(b, rm);
            if let Some(exp_add) = narrow(widen(a).add(widen(b), rm).0, rm) {
                assert!(
                    same_result(got_add, exp_add),
                    "add({sa}, {sb}, {rm:?}): Decimal32 -> {got_add}, oracle -> {exp_add}"
                );
            }
            let (got_sub, _) = a.sub(b, rm);
            if let Some(exp_sub) = narrow(widen(a).sub(widen(b), rm).0, rm) {
                assert!(
                    same_result(got_sub, exp_sub),
                    "sub({sa}, {sb}, {rm:?}): Decimal32 -> {got_sub}, oracle -> {exp_sub}"
                );
            }
        }
    }
}

/// The `rem` large-shift neighborhood: operand pairs whose alignment
/// shift exceeds `rem.rs`'s fixed `MAX_SAFE_SHIFT` while the quotient
/// stays small, the regime where the static window conflates u64
/// overflow with quotient digit count.
#[test]
#[ignore = "red on main: confirmed rem static MAX_SAFE_SHIFT defect; un-ignore with the H tier rem fix (fd-6tl)"]
fn rem_large_shift_neighborhood() {
    let probes: &[(&str, &str)] = &[
        ("1E+20", "1"),
        ("9999999E+20", "1E+5"),
        ("1E+90", "3E+70"),
        ("1234567E+30", "89E+10"),
        ("9.999999E+96", "7"),
    ];
    for (sa, sb) in probes {
        let (a, b) = (d32(sa), d32(sb));
        for rm in ROUNDING_MODES {
            let (got, _) = a.rem(b, rm);
            if let Some(expected) = narrow(widen(a).rem(widen(b), rm).0, rm) {
                assert!(
                    same_result(got, expected),
                    "rem({sa}, {sb}, {rm:?}): Decimal32 -> {got}, oracle -> {expected}"
                );
            }
        }
    }
}
