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
//! - `rem`: the remainder itself is exact and no larger in magnitude
//!   than the operands, representable in Decimal64 exactly. But the
//!   raw Decimal64 remainder is *not* the Decimal32 oracle: Decimal64
//!   keys its `Division_impossible` predicate on its own 10^16
//!   coefficient budget, while Decimal32 must raise
//!   `Invalid_operation` once the truncated integer quotient
//!   `trunc(|a / b|)` exceeds 7 digits (GDA `remainder`, IEEE
//!   754-2019 §5.3.1 + §7.2). The `rem_oracle_check` helper therefore
//!   computes the quotient digit count from the widened operands and
//!   asserts the spec-correct Decimal32 result: `NaN`/`INVALID` when
//!   that quotient exceeds 7 digits, the narrowed Decimal64 remainder
//!   otherwise.
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

// `add` / `sub` were red on `main` and are now fixed (H1) and held
// here as the permanent regression guard; `rem` (H2) uses the
// dedicated GDA-correct oracle below rather than the generic macro,
// because the Decimal64 remainder is only the Decimal32 oracle when
// the integer quotient stays within 7 digits.

crosscheck_active!(mul_matches_decimal64, mul);
crosscheck_active!(add_matches_decimal64, add);
crosscheck_active!(sub_matches_decimal64, sub);

// `rem` cannot use the generic Decimal64 oracle directly. The
// Decimal64 `rem` keys its `Division_impossible` predicate on its own
// 10^16 coefficient budget, while Decimal32 must, per the General
// Decimal Arithmetic `remainder` operation and IEEE 754-2019 §5.3.1
// plus §7.2, raise `Invalid_operation` once the *truncated integer
// quotient* `trunc(|a / b|)` exceeds `PRECISION = 7` digits. So a
// Decimal64 finite remainder is the exact Decimal32 oracle only when
// that integer quotient has at most 7 digits; when it has 8 to 16
// digits Decimal64 is finite but the spec-correct Decimal32 answer is
// `NaN` with `INVALID`, and when it exceeds 16 digits both formats
// raise `INVALID`. The `rem_oracle` helper below re-derives the GDA
// validity rule from the spec: it computes the integer quotient digit
// count exactly from the widened operands and asserts the
// spec-correct Decimal32 result, not the raw Decimal64 remainder.

/// Decompose a finite `Decimal32`'s exact decimal string into
/// `(negative, coefficient, exp10)` so that the value equals
/// `(-1)^negative × coefficient × 10^exp10`. The coefficient never
/// exceeds 7 digits, so a `u128` holds it with vast headroom.
fn decompose(d: Decimal32) -> (bool, u128, i32) {
    let s = d.to_string();
    let (negative, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.as_str()),
    };
    // Split an optional `E±nn` exponent suffix.
    let (mantissa, mut exp10): (&str, i32) = match rest.split_once(['E', 'e']) {
        Some((m, e)) => (m, e.parse::<i32>().expect("decimal exponent fits i32")),
        None => (rest, 0),
    };
    // Fold a fractional point into the integer coefficient.
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    exp10 -= frac_part.len() as i32;
    let digits: String = int_part.chars().chain(frac_part.chars()).collect();
    let coefficient: u128 = digits.parse().expect("≤ 7 significant digits fit u128");
    (negative, coefficient, exp10)
}

/// Number of decimal digits in the truncated integer quotient
/// `trunc(|a / b|)`, expressed as a comparison against the
/// `PRECISION = 7` budget: returns `true` when that quotient has more
/// than 7 digits (i.e. is `>= 10^7`), the GDA `Division_impossible`
/// condition for Decimal32.
///
/// `|a| = ca · 10^ea`, `|b| = cb · 10^eb`. The quotient is
/// `trunc((ca / cb) · 10^(ea − eb))`. The boundary `q >= 10^7`
/// rearranges to `ca · 10^max(g,0) >= 10^7 · cb · 10^max(-g,0)` with
/// `g = ea − eb`. When `|g|` is large one side dominates by orders of
/// magnitude and the answer is immediate; only a bounded window of
/// `g` needs the exact `u128` product, and there it always fits
/// (`ca, cb < 10^7`, the residual power is `< 10^7`).
fn quotient_exceeds_precision(ca: u128, ea: i32, cb: u128, eb: i32) -> bool {
    debug_assert!(ca >= 1 && cb >= 1);
    const LIMIT: u128 = 10u128.pow(7); // COEFFICIENT_LIMIT
    let g = ea - eb;
    if g >= 14 {
        // ca · 10^(g−7) vs cb. With g − 7 >= 7, ca·10^(g−7) >= 10^7 >
        // cb (cb < 10^7). Quotient definitely exceeds 7 digits.
        return true;
    }
    if g <= 0 {
        // cb · 10^(7−g) vs ca. With 7 − g >= 7, cb·10^(7−g) >= 10^7 >
        // ca. Quotient definitely at most 7 digits.
        return false;
    }
    // Bounded window 1 <= g <= 13. Compare exactly in u128:
    //   q >= 10^7  iff  ca · 10^g >= 10^7 · cb
    // ca < 10^7 and 10^g <= 10^13 so the left side is < 10^20 < 2^128;
    // the right side is < 10^14. No overflow.
    let lhs = ca * 10u128.pow(g as u32);
    let rhs = LIMIT * cb;
    lhs >= rhs
}

/// The `rem` cross-check with the GDA-correct oracle. For each
/// generated operand pair the spec-correct Decimal32 result is:
///
/// * the Decimal32 operands' own special handling (NaN / 0 / ∞) — out
///   of scope here, the generator emits finite operands only;
/// * `NaN` with `INVALID` when `trunc(|a / b|)` has more than 7
///   digits (GDA `Division_impossible`, IEEE 754-2019 §7.2);
/// * otherwise the exact remainder, which is small (strictly less
///   than `|b|`) and exactly representable; the Decimal64 remainder
///   rounded back to Decimal32 is then the exact oracle.
fn rem_oracle_check(a: Decimal32, b: Decimal32, rm: RoundingMode) -> Result<(), String> {
    let (actual, _) = a.rem(b, rm);
    let (_, ca, ea) = decompose(a);
    let (_, cb, eb) = decompose(b);

    // Dividend zero: remainder is ±0; quotient is 0 (≤ 7 digits).
    // Fall through to the finite-oracle branch, which handles it
    // (Decimal64 also yields a zero remainder).
    if ca != 0 && quotient_exceeds_precision(ca, ea, cb, eb) {
        // Spec-correct Decimal32 result: NaN with INVALID.
        if actual.to_string().contains("NaN") {
            return Ok(());
        }
        return Err(format!(
            "rem({a}, {b}, {rm:?}): integer quotient exceeds PRECISION = 7 \
             digits, spec answer is NaN/INVALID, Decimal32 -> {actual} \
             (a_bits={:#010x} b_bits={:#010x})",
            a.to_bits(),
            b.to_bits()
        ));
    }

    // Integer quotient fits 7 digits: the exact remainder is small
    // and representable, so the Decimal64 remainder rounded back to
    // Decimal32 is the exact oracle.
    let oracle = narrow(widen(a).rem(widen(b), rm).0, rm);
    let Some(expected) = oracle else {
        return Ok(()); // out of Decimal32 range: status-range, skipped
    };
    if same_result(actual, expected) {
        Ok(())
    } else {
        Err(format!(
            "rem({a}, {b}, {rm:?}): Decimal32 -> {actual}, Decimal64 \
             oracle -> {expected} (a_bits={:#010x} b_bits={:#010x})",
            a.to_bits(),
            b.to_bits()
        ))
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]
    #[test]
    fn rem_matches_decimal64(
        a in finite_d32(),
        b in finite_d32(),
        rmi in 0usize..ROUNDING_MODES.len(),
    ) {
        // Divisor zero is a special (NaN/INVALID in both formats),
        // out of this value oracle's scope.
        prop_assume!(!b.is_zero());
        let rm = ROUNDING_MODES[rmi];
        if let Err(msg) = rem_oracle_check(a, b, rm) {
            prop_assert!(false, "{}", msg);
        }
    }
}

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
        // Power-of-ten dominant minus a sub-ULP opposite-sign tail:
        // the Decimal64 fd-d47 regime. The `sub` case exercises the
        // addsub borrow-and-extend across a power-of-ten leading
        // digit, where the pre-fd-d47 PRECISION-cohort shape
        // mis-rounds (leaves seven all-nines with no round digit).
        ("1E+90", "1E-101"),
        ("1E+96", "1E-101"),
        ("1000000E+84", "1E-90"),
        ("1E+90", "1E+83"),
        ("1E-89", "1E-101"),
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
/// shift exceeds `rem.rs`'s former fixed `MAX_SAFE_SHIFT = 12`. With
/// the dynamic per-side bound the small-quotient pairs now compute
/// their finite remainder; the large-quotient pairs stay
/// `Division_impossible`. Each pair is held to the GDA-correct
/// `rem_oracle_check`, which asserts `NaN`/`INVALID` exactly when the
/// true integer quotient exceeds 7 digits.
#[test]
fn rem_large_shift_neighborhood() {
    let probes: &[(&str, &str)] = &[
        // Sound small-quotient witnesses (the H2 defect class):
        // shift > 12, integer quotient ≤ 7 digits, finite remainder.
        ("1E+13", "9999999"), // q = 1_000_000 (7 digits), rem 1_000_000
        ("1E+13", "5000000"), // q = 2_000_000 (7 digits), rem 0
        // Large-quotient pairs: quotient ≫ 7 digits, spec NaN/INVALID.
        ("1E+20", "1"),
        ("9999999E+20", "1E+5"),
        ("1E+90", "3E+70"),
        ("1234567E+30", "89E+10"),
        ("9.999999E+96", "7"),
        // The 2026-05-15 pinned KNOWN_ISSUES H3 case: quotient ~16
        // digits, spec-correct NaN/INVALID (oracle was unsound).
        ("4.194304E+33", "-3.145728E+18"),
    ];
    for (sa, sb) in probes {
        let (a, b) = (d32(sa), d32(sb));
        for rm in ROUNDING_MODES {
            if let Err(msg) = rem_oracle_check(a, b, rm) {
                panic!("{msg}");
            }
        }
    }
}
