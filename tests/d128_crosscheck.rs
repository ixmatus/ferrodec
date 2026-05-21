//! `Decimal128` cross-check **value** oracle for the `Decimal64`
//! arithmetic correctness slice. The D64→D128 analogue of
//! `ferrodec-decimal32/tests/d64_crosscheck.rs`, with two boundary
//! differences from that mirror, both forced by the parent format and
//! documented at their call sites: the oracle pins numeric value, not
//! cohort (`same_value`, the fd-61r preferred-exponent policy), and the
//! `rem` oracle is `Decimal128::rem_trunc` (the GDA truncated remainder
//! matching `Decimal64::rem_trunc`; `Decimal128::rem_near` is the
//! distinct IEEE round-half-even-quotient remainder). Track 2 of the
//! testing-surface extension (plan 2026-05-17). The 1.x bare `rem`
//! spelling was retired in 2.0 per ADR-0027.
//!
//! Every finite `Decimal64` value is exactly representable in
//! `Decimal128`: 16 significand digits and an exponent range
//! (`E_MAX = 384`, `E_MIN = −383`) strictly inside Decimal128's 34
//! digits and `±6144` range. Widening through the exact decimal string
//! is therefore lossless. The `Decimal128` operation is the oracle;
//! round its result back to 16 digits and it must equal the `Decimal64`
//! operation's numeric value.
//!
//! This catches a coefficient/alignment defect from an independent
//! angle: `Decimal64` and `Decimal128` arithmetic are independent
//! per-crate implementations (ADR-0011), so a shared bug is unlikely
//! and disagreement is high-signal. Transcendentals are **excluded**:
//! all three formats delegate to the one shared `ferrodec-transcend`
//! kernel (ADR-0024), so a cross-precision transcendental check would
//! be self-referential and carry near-zero independent signal.
//!
//! Oracle exactness, per operation:
//!
//! - `mul`: the exact product of two 16-digit coefficients is at most
//!   32 digits, representable in Decimal128's 34 exactly, so the
//!   Decimal128 product is exact and the round back to Decimal64 is the
//!   correctly rounded answer. No double rounding — exact oracle.
//! - `add` / `sub` / `div` / `fma`: the exact result can exceed 34
//!   significand digits. The Decimal128 step rounds first, so a double
//!   rounding divergence from the directly correctly rounded Decimal64
//!   result is possible, but only when the exact result needs more than
//!   34 digits AND lands within half a Decimal64 ULP of a rounding
//!   boundary after the first rounding. That narrow regime is exactly
//!   where a static-alignment-window defect would live, so a mismatch
//!   here is a signal to investigate, not noise. Strong screen, not an
//!   exact oracle.
//! - `rem`: the GDA truncated remainder is exact and representable, but
//!   `Decimal128::rem_trunc` (the matching op) keys `Division_impossible`
//!   on its own `10^34` budget, while Decimal64 must raise
//!   `Invalid_operation` once `trunc(|a / b|)` exceeds `PRECISION = 16`
//!   digits (GDA `remainder`, IEEE 754-2019 §5.3.1 + §7.2).
//!   `rem_oracle_check` re-derives the spec rule from the widened
//!   operands.
//!
//! Status is intentionally not cross-checked (the two formats have
//! different exponent ranges, so OVERFLOW / UNDERFLOW / SUBNORMAL
//! legitimately differ); results outside Decimal64's range are skipped
//! via the `narrow → None` arm. Cohort is not cross-checked either
//! (see `same_value`): the widen/narrow path traverses the parent's
//! fd-61r preferred-exponent policy, a by-design informational
//! divergence, not a value error.

#![cfg(feature = "fmt")]

use core::cmp::Ordering;

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_decimal64::Decimal64;
use proptest::prelude::*;

/// All five IEEE 754 rounding directions. `RoundingMode` is the one
/// `ferrodec-ieee` type re-exported by every sibling (ADR-0012), so the
/// same value drives both the `Decimal64` and `Decimal128` calls.
const ROUNDING_MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

/// Widen a finite `Decimal64` to `Decimal128` losslessly through its
/// exact decimal string.
fn widen(d: Decimal64) -> Decimal128 {
    Decimal128::parse_str(&d.to_string(), RoundingMode::NearestEven)
        .expect("the exact decimal string of a finite Decimal64 parses into Decimal128")
        .0
}

/// Round a `Decimal128` result back to `Decimal64` at `rm`, through its
/// exact decimal string. `None` when the value lies outside Decimal64's
/// representable range (overflow / underflow domain): a status-range
/// concern, deliberately out of scope for this value oracle.
fn narrow(d: Decimal128, rm: RoundingMode) -> Option<Decimal64> {
    Decimal64::parse_str(&d.to_string(), rm)
        .ok()
        .map(|(v, _)| v)
}

/// Numeric **value** equality (NOT cohort). NaN matches NaN;
/// infinities match by sign; finite values match by the cohort-
/// insensitive IEEE `compare` (`Decimal64::partial_cmp`).
///
/// Cohort is deliberately *not* compared on this boundary, unlike the
/// `Decimal32`→`Decimal64` mirror. `widen` / `narrow` route through
/// `Decimal128::parse_str`, whose preferred-exponent policy (the open
/// fd-61r area, IEEE 754-2019 §7.4 ideal exponent) differs from
/// `Decimal64`'s for zero and quantum-range operands: e.g.
/// `0E+2 + (-1E+1)` is the same value as `-1E+1` and `-10` but a
/// different cohort. That divergence is by-design and informational
/// ("no value error"; conformance masks the §7.4 `Clamped` flag), so
/// cohort-exact here would surface a known non-defect as noise. The
/// `Decimal32`↔`Decimal64` boundary shares the policy, so the mirror
/// can and does keep cohort-exact. A coefficient/alignment defect
/// drops low-order digits, changing the *value*, so the value oracle
/// still catches the target class.
fn same_value(a: Decimal64, b: Decimal64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        let (sa, sb) = (a.to_string(), b.to_string());
        let (nan_a, nan_b) = (sa.contains("NaN"), sb.contains("NaN"));
        if nan_a || nan_b {
            return nan_a && nan_b;
        }
        return sa == sb; // infinities: compare signed string
    }
    a.partial_cmp(b).0 == Some(Ordering::Equal)
}

/// Arbitrary finite `Decimal64`. Non-canonical encodings are fine: the
/// operation and `widen` decode the bits the same way. Specials are out
/// of scope (status differs by format) and filtered out.
fn finite_d64() -> impl Strategy<Value = Decimal64> {
    any::<u64>()
        .prop_map(Decimal64::from_bits)
        .prop_filter("finite operands only", |d| d.is_finite())
}

/// `mul` is the exact oracle (32-digit product fits Decimal128's 34);
/// `add` / `sub` / `div` carry the double-rounding screen documented in
/// the module header. All four are held to the Decimal128 oracle.
macro_rules! crosscheck_active {
    ($name:ident, $op:ident) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(4096))]
            #[test]
            fn $name(
                a in finite_d64(),
                b in finite_d64(),
                rmi in 0usize..ROUNDING_MODES.len(),
            ) {
                let rm = ROUNDING_MODES[rmi];
                let (actual, _) = a.$op(b, rm);
                let oracle = narrow(widen(a).$op(widen(b), rm).0, rm);
                prop_assume!(oracle.is_some());
                let expected = oracle.unwrap();
                prop_assert!(
                    same_value(actual, expected),
                    "{}({a}, {b}, {rm:?}): Decimal64 -> {actual}, Decimal128 \
oracle -> {expected} (a_bits={:#018x} b_bits={:#018x})",
                    stringify!($op), a.to_bits(), b.to_bits()
                );
            }
        }
    };
}

crosscheck_active!(mul_matches_decimal128, mul);
crosscheck_active!(add_matches_decimal128, add);
crosscheck_active!(sub_matches_decimal128, sub);
crosscheck_active!(div_matches_decimal128, div);

// `fma(a, b, c)` single-rounds the exact `a·b + c`. Widen all three,
// `fma` in Decimal128, narrow back. Same double-rounding screen as
// `add` / `div` (the exact `a·b + c` can exceed 34 digits). The D32
// file omits `fma` and is left frozen as a regression guard; it is
// added here rather than backfilled there.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]
    #[test]
    fn fma_matches_decimal128(
        a in finite_d64(),
        b in finite_d64(),
        c in finite_d64(),
        rmi in 0usize..ROUNDING_MODES.len(),
    ) {
        let rm = ROUNDING_MODES[rmi];
        let (actual, _) = a.fma(b, c, rm);
        let oracle = narrow(widen(a).fma(widen(b), widen(c), rm).0, rm);
        prop_assume!(oracle.is_some());
        let expected = oracle.unwrap();
        prop_assert!(
            same_value(actual, expected),
            "fma({a}, {b}, {c}, {rm:?}): Decimal64 -> {actual}, Decimal128 \
oracle -> {expected} (a={:#018x} b={:#018x} c={:#018x})",
            a.to_bits(), b.to_bits(), c.to_bits()
        );
    }
}

/// Decompose a finite `Decimal64`'s exact decimal string into
/// `(negative, coefficient, exp10)` so the value equals
/// `(-1)^negative × coefficient × 10^exp10`. The coefficient never
/// exceeds 16 digits, so a `u128` holds it with headroom.
fn decompose(d: Decimal64) -> (bool, u128, i32) {
    let s = d.to_string();
    let (negative, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.as_str()),
    };
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
    let coefficient: u128 = digits.parse().expect("≤ 16 significant digits fit u128");
    (negative, coefficient, exp10)
}

/// Decimal digit count of `x ≥ 1`.
fn digits10(mut x: u128) -> u32 {
    let mut n = 0;
    while x > 0 {
        x /= 10;
        n += 1;
    }
    n
}

/// `true` when `trunc(|a / b|)` has more than `PRECISION = 16` digits
/// (`>= 10^16`), the GDA `Division_impossible` condition for Decimal64.
///
/// `|a| = ca · 10^ea`, `|b| = cb · 10^eb`, `g = ea − eb`. Since
/// `trunc(x) >= N ⇔ x >= N` for integer `N`, the boundary is exactly
/// `ca · 10^g >= 10^16 · cb`. A direct `u128` product overflows here
/// (`ca < 10^16`, `g` up to ~31 in the boundary band would reach
/// `10^47`), so the decision is made by digit count and only the
/// single-digit-count boundary band falls back to an exact compare,
/// where canceling `10^min(g,16)` keeps both sides below `10^31`.
fn quotient_exceeds_precision(ca: u128, ea: i32, cb: u128, eb: i32) -> bool {
    debug_assert!(ca >= 1 && cb >= 1);
    const P: i64 = 16; // Decimal64 PRECISION
    let g = (ea - eb) as i64;
    if g <= 0 {
        // cb · 10^(16−g) ≥ 10^16 > ca (ca < 10^16): quotient ≤ 16 digits.
        return false;
    }
    let (da, db) = (i64::from(digits10(ca)), i64::from(digits10(cb)));
    // digits(ca·10^g) = da + g; digits(10^16·cb) = P + db.
    let lhs_digits = da + g;
    let rhs_digits = P + db;
    if lhs_digits > rhs_digits {
        return true; // strictly more digits ⇒ strictly larger
    }
    if lhs_digits < rhs_digits {
        return false; // strictly fewer digits ⇒ strictly smaller
    }
    // Equal digit count: exact compare of ca·10^g vs cb·10^16, with
    // 10^min(g,16) canceled so neither side exceeds ~10^31 < u128::MAX.
    if g >= 16 {
        ca * 10u128.pow((g - 16) as u32) >= cb
    } else {
        ca >= cb * 10u128.pow((16 - g) as u32)
    }
}

/// The `rem` cross-check with the GDA-correct oracle: `NaN`/`INVALID`
/// when `trunc(|a / b|)` exceeds 16 digits, otherwise the exact (small,
/// representable) remainder, for which the narrowed Decimal128
/// remainder is the exact oracle.
fn rem_oracle_check(a: Decimal64, b: Decimal64, rm: RoundingMode) -> Result<(), String> {
    let (actual, _) = a.rem_trunc(b);
    let (_, ca, ea) = decompose(a);
    let (_, cb, eb) = decompose(b);

    if ca != 0 && quotient_exceeds_precision(ca, ea, cb, eb) {
        if actual.to_string().contains("NaN") {
            return Ok(());
        }
        return Err(format!(
            "rem({a}, {b}, {rm:?}): integer quotient exceeds PRECISION = 16 \
             digits, spec answer is NaN/INVALID, Decimal64 -> {actual} \
             (a_bits={:#018x} b_bits={:#018x})",
            a.to_bits(),
            b.to_bits()
        ));
    }

    let oracle = narrow(widen(a).rem_trunc(widen(b)).0, rm);
    let Some(expected) = oracle else {
        return Ok(()); // out of Decimal64 range: status-range, skipped
    };
    if same_value(actual, expected) {
        Ok(())
    } else {
        Err(format!(
            "rem({a}, {b}, {rm:?}): Decimal64 -> {actual}, Decimal128 \
             oracle -> {expected} (a_bits={:#018x} b_bits={:#018x})",
            a.to_bits(),
            b.to_bits()
        ))
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]
    #[test]
    fn rem_matches_decimal128(
        a in finite_d64(),
        b in finite_d64(),
        rmi in 0usize..ROUNDING_MODES.len(),
    ) {
        prop_assume!(!b.is_zero());
        let rm = ROUNDING_MODES[rmi];
        if let Err(msg) = rem_oracle_check(a, b, rm) {
            prop_assert!(false, "{}", msg);
        }
    }
}

/// Parse a literal known to be inside Decimal64's range.
fn d64(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .expect("in-range Decimal64 literal")
        .0
}

/// The `add` / `sub` static-alignment-window neighborhood: a small
/// coefficient at a large exponent combined with a normal-magnitude
/// operand, where a fixed alignment window would truncate the lower
/// operand. Each pair is held to the Decimal128 oracle across all five
/// rounding modes. Magnitudes are at the edge of Decimal64's
/// `±384` exponent range.
#[test]
fn addsub_small_coef_large_gap_neighborhood() {
    let probes: &[(&str, &str)] = &[
        ("1E+384", "1E+360"),
        ("1E+384", "1234567890123456E+340"),
        ("9.999999999999999E+384", "1E+370"),
        ("1234567890123456E+10", "7654321098765432E-30"),
        ("1E-380", "1E-398"),
        ("5000000000000001E+200", "1E+185"),
        ("1E+384", "1E-398"),
        ("1000000000000000E+360", "1E-380"),
        ("1E+384", "1E+367"),
        ("1E-383", "1E-398"),
    ];
    for (sa, sb) in probes {
        let (a, b) = (d64(sa), d64(sb));
        for rm in ROUNDING_MODES {
            let (got_add, _) = a.add(b, rm);
            if let Some(exp_add) = narrow(widen(a).add(widen(b), rm).0, rm) {
                assert!(
                    same_value(got_add, exp_add),
                    "add({sa}, {sb}, {rm:?}): Decimal64 -> {got_add}, oracle -> {exp_add}"
                );
            }
            let (got_sub, _) = a.sub(b, rm);
            if let Some(exp_sub) = narrow(widen(a).sub(widen(b), rm).0, rm) {
                assert!(
                    same_value(got_sub, exp_sub),
                    "sub({sa}, {sb}, {rm:?}): Decimal64 -> {got_sub}, oracle -> {exp_sub}"
                );
            }
        }
    }
}

/// The `rem` large-shift neighborhood: pairs whose alignment shift is
/// large, split into sound small-quotient witnesses (finite remainder,
/// integer quotient ≤ 16 digits) and large-quotient pairs (spec
/// `NaN`/`INVALID`). Each is held to the GDA-correct
/// `rem_oracle_check`.
#[test]
fn rem_large_shift_neighborhood() {
    let probes: &[(&str, &str)] = &[
        // Small-quotient witnesses: large shift, quotient ≤ 16 digits.
        ("1E+22", "9999999999999999"), // q = 1_000_…(16 digits)
        ("1E+22", "5000000000000000"), // q = 2 × 10^15 (16 digits)
        // Large-quotient pairs: quotient ≫ 16 digits ⇒ spec NaN/INVALID.
        ("1E+40", "1"),
        ("9999999999999999E+40", "1E+5"),
        ("1E+384", "3E+360"),
        ("1234567890123456E+120", "89E+40"),
        ("9.999999999999999E+384", "7"),
    ];
    for (sa, sb) in probes {
        let (a, b) = (d64(sa), d64(sb));
        for rm in ROUNDING_MODES {
            if let Err(msg) = rem_oracle_check(a, b, rm) {
                panic!("{msg}");
            }
        }
    }
}
