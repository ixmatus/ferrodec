//! General Decimal Arithmetic decNumber extension coverage for
//! `Decimal32`: `reduce`, `divide_integer`, `logical_invert`,
//! `logical_and`, `logical_or`, `logical_xor`, `shift`, `rotate`.
//!
//! ADR-0031 admitted these eight additive operations into the 1.x line
//! and named the Decimal32 verification posture explicitly. No upstream
//! `ds*.decTest` vectors exist (the historical decSingle distribution
//! shipped no GDA-extension vectors), so the Decimal64 conformance pins
//! cannot carry over. The ADR therefore placed Decimal32 coverage on two
//! artifacts: hand-derived property tests over the operand-validity
//! predicates and special-case table, and cross-format property tests
//! that lift a Decimal32 input to Decimal64, apply the
//! conformance-proven Decimal64 operation, and narrow the result back.
//! This file is that coverage; it is the parity counterpart of the
//! Decimal64 `dd*.decTest` conformance dispatch.
//!
//! ## What the cross-format oracle can and cannot check
//!
//! Every finite `Decimal32` widens to `Decimal64` losslessly: 7
//! significand digits and an exponent range strictly inside Decimal64's
//! 16 digits and wider exponent range. The Decimal64 operation, brought
//! to full conformance in its 1.4.0 slice and pinned against the
//! `dd*.decTest` vectors, is the oracle. But two of the eight ops are
//! precision-window dependent and therefore cannot be cross-checked
//! directly across the formats:
//!
//! - `logical_invert` complements every digit of the precision-wide
//!   window. Decimal64 inverts 16 digits, filling the high 9 positions
//!   that Decimal32 never has; the two results are numerically different
//!   by construction. `logical_invert` is covered by hand only.
//! - `shift` and `rotate` move digits within the precision-wide window,
//!   and Decimal64's window is 16 wide where Decimal32's is 7. A left
//!   shift that falls off Decimal32's 7-digit window stays inside
//!   Decimal64's 16-digit window; a rotate wraps over a different
//!   modulus. Both are covered by hand only.
//!
//! The remaining five ops cross-check cleanly within stated domains:
//!
//! - `reduce` is numerically identity-preserving and width-independent:
//!   a reduced value carries the same digits at both precisions, so
//!   `narrow(widen(a).reduce())` equals `a.reduce()` on every finite
//!   operand.
//! - `divide_integer` yields the same integer quotient at both
//!   precisions, but Decimal32 raises `Invalid_operation` once that
//!   quotient exceeds 7 digits while Decimal64 tolerates up to 16. The
//!   oracle holds only when the quotient fits 7 digits; the
//!   helper screens that domain and asserts `NaN`/`INVALID` outside it.
//! - `logical_and` / `logical_or` / `logical_xor` over two Decimal32
//!   logical operands produce a result whose set digits all sit in the
//!   low 7 positions, which narrows back from Decimal64 exactly. The
//!   logical-operand generator emits only valid operands so the oracle
//!   stays in its domain.
//!
//! Status is not cross-checked: none of these ops ever raises a
//! range flag, and the only flag they raise (`INVALID` on bad operands)
//! is asserted directly in the hand-derived tests.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};
use ferrodec_decimal64::Decimal64;
use proptest::prelude::*;

/// Decimal32 precision in digits. The GDA logical / shift / rotate
/// window width and the `divide_integer` quotient budget.
const D32_PRECISION: u32 = 7;

// ----------------------------------------------------------------------
// Cross-format bridge helpers (shared with `d64_crosscheck.rs`'s pattern)
// ----------------------------------------------------------------------

/// Widen a finite `Decimal32` to `Decimal64` losslessly through its
/// exact decimal string.
fn widen(d: Decimal32) -> Decimal64 {
    Decimal64::parse_str(&d.to_string(), RoundingMode::NearestEven)
        .expect("the exact decimal string of a finite Decimal32 parses into Decimal64")
        .0
}

/// Round a `Decimal64` result back to `Decimal32` through its exact
/// decimal string. Returns `None` when the value lies outside
/// Decimal32's representable range (a status-range concern, out of scope
/// for these value oracles). All GDA-extension results checked here are
/// in range by construction, so `None` only ever signals an unexpected
/// out-of-range result, which the callers treat as a skip.
fn narrow(d: Decimal64) -> Option<Decimal32> {
    Decimal32::parse_str(&d.to_string(), RoundingMode::NearestEven)
        .ok()
        .map(|(v, _)| v)
}

/// Value plus cohort equality. NaN matches NaN (payload and sign are not
/// part of the value contract checked here); infinities match by sign;
/// finite values match by their exact canonical string, which carries
/// the cohort exponent.
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

/// Arbitrary finite `Decimal32`, including non-canonical encodings: both
/// the operation and `widen` decode the bits the same way, so the oracle
/// stays consistent. Specials are out of scope and filtered out.
fn finite_d32() -> impl Strategy<Value = Decimal32> {
    any::<u32>()
        .prop_map(Decimal32::from_bits)
        .prop_filter("finite operands only", |d| d.is_finite())
}

/// Arbitrary valid Decimal32 logical operand: a non-negative integer at
/// exponent zero whose decimal digits all lie in `{0, 1}`. There are
/// `2^7 = 128` such values; the strategy draws the 7-bit digit pattern
/// directly so every generated operand satisfies the precondition and
/// the cross-format oracle never leaves its domain.
fn logical_d32() -> impl Strategy<Value = Decimal32> {
    (0u32..128).prop_map(|bits| {
        // Bit i of `bits` is the value of decimal digit position i
        // (position 0 is the units digit), so the coefficient is the
        // base-10 reading of the bit pattern.
        let mut coef = 0u32;
        let mut place = 1u32;
        for i in 0..D32_PRECISION {
            if bits & (1 << i) != 0 {
                coef += place;
            }
            place *= 10;
        }
        Decimal32::try_new_unsigned(coef, 0).expect("logical operand fits Decimal32")
    })
}

// ----------------------------------------------------------------------
// `reduce`: cross-format identity oracle
// ----------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `reduce` strips trailing zeros without changing the value, so the
    /// digits of the reduced form are identical at both precisions and
    /// `narrow(widen(a).reduce())` equals `a.reduce()` on every finite
    /// operand. Decimal64's wider clamp limit never matters here: a
    /// 7-digit coefficient reduces to at most 7 significant digits, well
    /// inside Decimal64's 16, and the preferred exponent after stripping
    /// stays inside Decimal32's range whenever the input was in range.
    #[test]
    fn reduce_matches_decimal64(a in finite_d32()) {
        let (actual, st) = a.reduce();
        prop_assert!(st.is_ok());
        let oracle = narrow(widen(a).reduce().0);
        prop_assume!(oracle.is_some());
        let expected = oracle.unwrap();
        prop_assert!(
            same_result(actual, expected),
            "reduce({a}): Decimal32 -> {actual}, Decimal64 oracle -> {expected} \
             (a_bits={:#010x})",
            a.to_bits()
        );
    }
}

// ----------------------------------------------------------------------
// `divide_integer`: cross-format oracle inside the 7-digit-quotient
// domain, NaN/INVALID outside it
// ----------------------------------------------------------------------

/// Decompose a finite `Decimal32`'s exact decimal string into
/// `(coefficient, exp10)` so the value equals `coefficient × 10^exp10`,
/// sign dropped. The coefficient never exceeds 7 digits, so a `u128`
/// holds it with vast headroom.
fn decompose_magnitude(d: Decimal32) -> (u128, i32) {
    let s = d.to_string();
    let rest = s.strip_prefix('-').unwrap_or(&s);
    let (mantissa, mut exp10): (&str, i32) = match rest.split_once(['E', 'e']) {
        Some((m, e)) => (m, e.parse::<i32>().expect("decimal exponent fits i32")),
        None => (rest, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    exp10 -= frac_part.len() as i32;
    let digits: String = int_part.chars().chain(frac_part.chars()).collect();
    let coefficient: u128 = digits.parse().expect("<= 7 significant digits fit u128");
    (coefficient, exp10)
}

/// Whether `trunc(|a / b|) >= 10^7`, the GDA `Division_impossible`
/// condition for Decimal32. `|a| = ca · 10^ea`, `|b| = cb · 10^eb`; the
/// quotient is `trunc((ca / cb) · 10^(ea − eb))`. The boundary
/// `q >= 10^7` rearranges to `ca · 10^max(g,0) >= 10^7 · cb · 10^max(-g,0)`
/// with `g = ea − eb`; large `|g|` settles it immediately, and the
/// bounded window computes the exact `u128` product (which always fits:
/// `ca < 10^7`, the residual power is `< 10^14`).
fn quotient_exceeds_precision(ca: u128, ea: i32, cb: u128, eb: i32) -> bool {
    debug_assert!(ca >= 1 && cb >= 1);
    const LIMIT: u128 = 10u128.pow(7);
    let g = ea - eb;
    if g >= 14 {
        return true;
    }
    if g <= 0 {
        return false;
    }
    let lhs = ca * 10u128.pow(g as u32);
    let rhs = LIMIT * cb;
    lhs >= rhs
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `divide_integer` yields the same integer quotient at both
    /// precisions, but Decimal32 must raise `Invalid_operation` once that
    /// quotient exceeds 7 digits (GDA `Division_impossible`) while
    /// Decimal64's budget is 16 digits. So the Decimal64 quotient is the
    /// exact Decimal32 oracle only inside the 7-digit-quotient domain;
    /// outside it the spec-correct Decimal32 answer is `NaN`/`INVALID`.
    #[test]
    fn divide_integer_matches_decimal64(a in finite_d32(), b in finite_d32()) {
        prop_assume!(!b.is_zero());
        let (actual, _) = a.divide_integer(b);
        let (ca, ea) = decompose_magnitude(a);
        let (cb, eb) = decompose_magnitude(b);

        if ca != 0 && quotient_exceeds_precision(ca, ea, cb, eb) {
            prop_assert!(
                actual.to_string().contains("NaN"),
                "divide_integer({a}, {b}): integer quotient exceeds 7 digits, \
                 spec answer is NaN/INVALID, Decimal32 -> {actual} \
                 (a_bits={:#010x} b_bits={:#010x})",
                a.to_bits(), b.to_bits()
            );
        } else {
            let oracle = narrow(widen(a).divide_integer(widen(b)).0);
            prop_assume!(oracle.is_some());
            let expected = oracle.unwrap();
            prop_assert!(
                same_result(actual, expected),
                "divide_integer({a}, {b}): Decimal32 -> {actual}, Decimal64 \
                 oracle -> {expected} (a_bits={:#010x} b_bits={:#010x})",
                a.to_bits(), b.to_bits()
            );
        }
    }
}

// ----------------------------------------------------------------------
// `logical_and` / `logical_or` / `logical_xor`: cross-format oracle over
// valid logical operands
// ----------------------------------------------------------------------

macro_rules! logical_binary_crosscheck {
    ($name:ident, $op:ident) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(2048))]

            /// A digit-wise truth-table op over two valid logical
            /// operands sets only digits in the low 7 positions, so the
            /// Decimal64 result (computed over its 16-digit window, with
            /// the high 9 positions zero on both operands) narrows back
            /// to Decimal32 exactly. The result is positive, at exponent
            /// zero, in both formats.
            #[test]
            fn $name(a in logical_d32(), b in logical_d32()) {
                let (actual, st) = a.$op(b);
                prop_assert!(st.is_ok());
                let oracle = narrow(widen(a).$op(widen(b)).0);
                prop_assume!(oracle.is_some());
                let expected = oracle.unwrap();
                let op = stringify!($op);
                prop_assert!(
                    same_result(actual, expected),
                    "{op}({a}, {b}): Decimal32 -> {actual}, oracle -> {expected} (a_bits={:#010x} b_bits={:#010x})",
                    a.to_bits(), b.to_bits()
                );
            }
        }
    };
}

logical_binary_crosscheck!(logical_and_matches_decimal64, logical_and);
logical_binary_crosscheck!(logical_or_matches_decimal64, logical_or);
logical_binary_crosscheck!(logical_xor_matches_decimal64, logical_xor);

// ----------------------------------------------------------------------
// `logical_*` truth-table and operand-validity properties (hand-derived)
// ----------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// The four logical ops obey their defining truth-table identities
    /// digit by digit, checked here over the whole valid-operand space:
    /// `and` is idempotent, `or` is idempotent, `xor` with self is zero,
    /// double `invert` is identity, and De Morgan ties `and`/`or`
    /// through `invert`.
    #[test]
    fn logical_truth_table_identities(a in logical_d32(), b in logical_d32()) {
        // and / or idempotence.
        let (and_self, _) = a.logical_and(a);
        prop_assert!(same_result(and_self, a), "a AND a != a for {a}");
        let (or_self, _) = a.logical_or(a);
        prop_assert!(same_result(or_self, a), "a OR a != a for {a}");

        // xor with self is zero.
        let (xor_self, _) = a.logical_xor(a);
        prop_assert!(xor_self.is_zero(), "a XOR a != 0 for {a}");

        // Double invert is identity (invert pads to all 7 positions, so
        // the second invert restores the original 7-digit pattern).
        let (inv_once, _) = a.logical_invert();
        let (inv_twice, _) = inv_once.logical_invert();
        prop_assert!(
            same_result(inv_twice, a),
            "invert(invert(a)) != a for {a}: got {inv_twice}"
        );

        // De Morgan: NOT(a AND b) == (NOT a) OR (NOT b).
        let (and_ab, _) = a.logical_and(b);
        let (not_and_ab, _) = and_ab.logical_invert();
        let (not_a, _) = a.logical_invert();
        let (not_b, _) = b.logical_invert();
        let (de_morgan, _) = not_a.logical_or(not_b);
        prop_assert!(
            same_result(not_and_ab, de_morgan),
            "De Morgan failed for ({a}, {b}): {not_and_ab} vs {de_morgan}"
        );
    }
}

/// The all-ones 7-digit logical operand `1111111`, the complement of
/// zero under `logical_invert`.
fn all_ones_d32() -> Decimal32 {
    Decimal32::try_new_unsigned((10u32.pow(7) - 1) / 9, 0).expect("1111111 fits Decimal32")
}

#[test]
fn invert_zero_is_seven_ones() {
    let (r, st) = Decimal32::ZERO.logical_invert();
    assert!(st.is_ok());
    assert!(same_result(r, all_ones_d32()));
}

#[test]
fn invert_seven_ones_is_zero() {
    let (r, st) = all_ones_d32().logical_invert();
    assert!(st.is_ok());
    assert!(r.is_zero());
    assert!(!r.is_sign_negative());
}

/// `logical_invert` is the precision-window op that cannot ride the
/// Decimal64 oracle: Decimal64 fills the high 9 of its 16 digits with
/// ones where Decimal32 has only 7 positions. This pins the divergence
/// directly so the cross-format omission is documented by a test, not
/// only by prose.
#[test]
fn invert_window_width_differs_from_decimal64() {
    let (d32_inv, _) = Decimal32::ZERO.logical_invert();
    let d64_inv = widen(Decimal32::ZERO).logical_invert().0;
    // Decimal32 inverts to 7 ones; Decimal64 to 16 ones.
    assert_eq!(d32_inv.to_string(), "1111111");
    assert_eq!(d64_inv.to_string(), "1111111111111111");
}

// ----------------------------------------------------------------------
// NaN-as-INVALID rule for the logical ops (hand-derived)
// ----------------------------------------------------------------------

/// Every logical op rejects every NaN operand as INVALID, unlike the
/// "qNaN passes through OK" rule the other GDA ops follow. A signaling
/// NaN is quieted; a quiet NaN raises INVALID without passing through to
/// an OK result.
#[test]
fn logical_ops_reject_every_nan_as_invalid() {
    let one = Decimal32::ONE;
    let qnan = Decimal32::NAN;
    let snan = Decimal32::SIGNALING_NAN;

    // logical_invert.
    for nan in [qnan, snan] {
        let (r, st) = nan.logical_invert();
        assert_eq!(st, Status::INVALID, "invert NaN not INVALID");
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan(), "invert leaves a signaling NaN");
    }

    // Binary ops, NaN on either side.
    for op in [
        Decimal32::logical_and as fn(Decimal32, Decimal32) -> (Decimal32, Status),
        Decimal32::logical_or,
        Decimal32::logical_xor,
    ] {
        for (a, b) in [(qnan, one), (one, qnan), (snan, one), (one, snan)] {
            let (r, st) = op(a, b);
            assert_eq!(st, Status::INVALID, "binary logical NaN not INVALID");
            assert!(r.is_nan());
            assert!(
                !r.is_signaling_nan(),
                "binary logical leaves a signaling NaN"
            );
        }
    }
}

/// The logical-operand precondition rejects every input that is not a
/// non-negative integer at exponent zero with all digits in `{0, 1}`.
#[test]
fn logical_operand_precondition_rejected_inputs() {
    let bad: &[Decimal32] = &[
        Decimal32::try_new(-1, 0).unwrap(),  // negative sign
        Decimal32::try_new(2, 0).unwrap(),   // digit above one
        Decimal32::try_new(1, 1).unwrap(),   // nonzero exponent
        Decimal32::try_new(10, -1).unwrap(), // numerically 1 but exp != 0
        Decimal32::INFINITY,
        Decimal32::NEG_INFINITY,
    ];
    for &x in bad {
        let (r, st) = x.logical_invert();
        assert_eq!(st, Status::INVALID, "invert accepted bad operand {x}");
        assert!(r.is_nan());
        let (r2, st2) = x.logical_and(Decimal32::ZERO);
        assert_eq!(st2, Status::INVALID, "and accepted bad operand {x}");
        assert!(r2.is_nan());
    }
}

// ----------------------------------------------------------------------
// `shift`: precision-7 boundary and validity (hand-derived)
// ----------------------------------------------------------------------

fn d(c: i32, e: i32) -> Decimal32 {
    Decimal32::try_new(c, e).unwrap()
}

#[test]
fn shift_left_fills_zero_on_the_right() {
    let (r, st) = d(12345, 0).shift(d(2, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(1234500, 0).to_bits());
}

#[test]
fn shift_left_off_the_seven_digit_window_drops_high_digits() {
    // 1234567 shifted left by 3 keeps the low 4 digits (4567) in the
    // 7-digit window: 4567000.
    let (r, st) = d(1234567, 0).shift(d(3, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(4567000, 0).to_bits());
}

#[test]
fn shift_right_drops_low_digits() {
    let (r, st) = d(1234567, 0).shift(d(-3, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(1234, 0).to_bits());
}

#[test]
fn shift_by_full_precision_clears_the_coefficient() {
    // |n| == PRECISION moves every digit out of the window.
    let (left, st_l) = d(1234567, 0).shift(d(7, 0));
    assert!(st_l.is_ok());
    assert!(left.is_zero());
    let (right, st_r) = d(1234567, 0).shift(d(-7, 0));
    assert!(st_r.is_ok());
    assert!(right.is_zero());
}

#[test]
fn shift_preserves_sign_and_exponent() {
    let x = d(-123, 2);
    let (r, st) = x.shift(d(1, 0));
    assert!(st.is_ok());
    assert!(r.is_sign_negative());
    // Sign and the lhs exponent ride through unchanged; only the
    // coefficient digits move.
    assert_eq!(r.to_bits(), d(-1230, 2).to_bits());
}

#[test]
fn shift_rhs_above_precision_is_invalid() {
    let (r, st) = d(1, 0).shift(d(8, 0));
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
    let (r2, st2) = d(1, 0).shift(d(-8, 0));
    assert_eq!(st2, Status::INVALID);
    assert!(r2.is_nan());
}

#[test]
fn shift_rhs_non_integer_is_invalid() {
    // The literal `1.0` (coefficient 10, exponent -1) is numerically an
    // integer but not at exponent zero, so it is rejected per ADR-0031.
    let (r, st) = d(123, 0).shift(d(10, -1));
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
}

#[test]
fn shift_signaling_nan_lhs_quiets_and_raises_invalid() {
    let (r, st) = Decimal32::SIGNALING_NAN.shift(d(2, 0));
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
    assert!(!r.is_signaling_nan());
}

#[test]
fn shift_quiet_nan_lhs_passes_through_ok() {
    // Unlike the logical ops, shift lets a quiet NaN pass through OK.
    let (r, st) = Decimal32::NAN.shift(d(2, 0));
    assert!(st.is_ok());
    assert!(r.is_nan());
}

#[test]
fn shift_infinity_passes_through() {
    let (r, st) = Decimal32::INFINITY.shift(d(3, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), Decimal32::INFINITY.to_bits());
}

// ----------------------------------------------------------------------
// `rotate`: digit wraparound over the 7-digit window (hand-derived)
// ----------------------------------------------------------------------

#[test]
fn rotate_full_precision_is_identity() {
    let x = d(1234567, 0);
    let (r, st) = x.rotate(d(7, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), x.to_bits());
    let (r2, st2) = x.rotate(d(-7, 0));
    assert!(st2.is_ok());
    assert_eq!(r2.to_bits(), x.to_bits());
}

#[test]
fn rotate_left_wraps_high_digits_to_the_bottom() {
    // 1234567 rotated left by 3: 4567 stays, 123 wraps to the bottom ->
    // 4567123.
    let (r, st) = d(1234567, 0).rotate(d(3, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(4567123, 0).to_bits());
}

#[test]
fn rotate_right_wraps_low_digits_to_the_top() {
    // 1234567 rotated right by 2: 67 wraps to the top -> 6712345.
    let (r, st) = d(1234567, 0).rotate(d(-2, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(6712345, 0).to_bits());
}

#[test]
fn rotate_single_unit_right_lands_in_top_position() {
    // 1 rotated right by 1 wraps the units digit to the top of the
    // 7-digit window: 1_000_000.
    let (r, st) = d(1, 0).rotate(d(-1, 0));
    assert!(st.is_ok());
    assert_eq!(r.to_bits(), d(1_000_000, 0).to_bits());
}

#[test]
fn rotate_left_and_right_are_inverse() {
    // Rotating left by k then right by k restores the original window
    // for every k in [0, 7].
    let x = d(1234567, 0);
    for k in 0..=7 {
        let (l, st_l) = x.rotate(d(k, 0));
        assert!(st_l.is_ok());
        let (back, st_b) = l.rotate(d(-k, 0));
        assert!(st_b.is_ok());
        assert_eq!(back.to_bits(), x.to_bits(), "rotate by {k} not invertible");
    }
}

#[test]
fn rotate_rhs_above_precision_is_invalid() {
    let (r, st) = d(1, 0).rotate(d(8, 0));
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
}

#[test]
fn rotate_signaling_nan_lhs_quiets_and_raises_invalid() {
    let (r, st) = Decimal32::SIGNALING_NAN.rotate(d(3, 0));
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
    assert!(!r.is_signaling_nan());
}

// ----------------------------------------------------------------------
// `reduce`: special-case edges not reachable through the property oracle
// (hand-derived)
// ----------------------------------------------------------------------

#[test]
fn reduce_negative_zero_keeps_sign() {
    // -0 at a non-zero exponent normalises to the canonical -0 quantum.
    // `try_new` cannot make a signed zero, so parse the exact literal.
    let neg_zero = Decimal32::parse_str("-0E+5", RoundingMode::NearestEven)
        .unwrap()
        .0;
    let (r, st) = neg_zero.reduce();
    assert!(st.is_ok());
    assert!(r.is_zero());
    assert!(r.is_sign_negative());
}

#[test]
fn reduce_signaling_nan_quiets_and_raises_invalid() {
    let (r, st) = Decimal32::SIGNALING_NAN.reduce();
    assert_eq!(st, Status::INVALID);
    assert!(r.is_nan());
    assert!(!r.is_signaling_nan());
}

#[test]
fn reduce_quiet_nan_passes_through() {
    let (r, st) = Decimal32::NAN.reduce();
    assert!(st.is_ok());
    assert!(r.is_nan());
    assert!(!r.is_signaling_nan());
}

#[test]
fn reduce_infinity_passes_through() {
    for inf in [Decimal32::INFINITY, Decimal32::NEG_INFINITY] {
        let (r, st) = inf.reduce();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), inf.to_bits());
    }
}

// ----------------------------------------------------------------------
// `divide_integer`: special-case edges (hand-derived)
// ----------------------------------------------------------------------

#[test]
fn divide_integer_finite_over_zero_is_div_by_zero() {
    let (q, st) = d(1, 0).divide_integer(Decimal32::ZERO);
    assert_eq!(st, Status::DIV_BY_ZERO);
    assert_eq!(q.to_bits(), Decimal32::INFINITY.to_bits());
}

#[test]
fn divide_integer_zero_over_zero_is_invalid() {
    let (q, st) = Decimal32::ZERO.divide_integer(Decimal32::ZERO);
    assert_eq!(st, Status::INVALID);
    assert!(q.is_nan());
}

#[test]
fn divide_integer_quotient_over_precision_is_invalid() {
    // 10^7 / 1 needs 8 digits, over PRECISION = 7.
    let (q, st) = d(1, 7).divide_integer(d(1, 0));
    assert_eq!(st, Status::INVALID);
    assert!(q.is_nan());
}

#[test]
fn divide_integer_signaling_nan_quiets_and_raises_invalid() {
    let (q, st) = Decimal32::SIGNALING_NAN.divide_integer(d(2, 0));
    assert_eq!(st, Status::INVALID);
    assert!(q.is_nan());
    assert!(!q.is_signaling_nan());
}
