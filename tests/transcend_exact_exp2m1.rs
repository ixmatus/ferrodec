//! Exact-result, tie, and special-value gate for `Decimal128::exp2_m1`
//! (IEEE 754-2019 §9.2 `exp2m1`; ADR-0059 Track D).
//!
//! The classifier `ferrodec_transcend::exact::exp2m1_exact_or_tie`
//! claims a complete boundary set: `2^x − 1 = r` rational makes
//! `2^x = 1 + r` rational, and unique factorization forces a
//! representable `x = a/b` to have `b = 1`, so every exact result and
//! every nearest mode tie sits at an integer `x = n` with value
//! `2^n − 1`. That splits into the odd integers `2^n − 1` for `n ≥ 1`
//! and the fractions `−(10^m − 5^m)·10^−m` for `n = −m ≤ −1`. This
//! file is the claim's exhaustive witness at 34 digits: every `n` in
//! `1..=112` and every `m` in `1..=34`, in all five rounding
//! directions, delivered exactly with status `OK` and no `INEXACT`
//! (§7.5 forbids it on an exact result).
//!
//! ## The ties, and why they are only witnessed here
//!
//! Unlike the `logp1` family, this one has real nearest mode ties, six
//! across the three formats and two at this one. `2^n − 1` ends in 5
//! exactly when `4 | n` (`2^n mod 10` cycles `2, 4, 8, 6`), and
//! `n = 116` is the single multiple of four whose value spans 35
//! digits; `10^m − 5^m` always ends in 5, so `m = 35` is a midpoint
//! too. The corpus generator excludes both inputs from every scan
//! mode, because a certified Arb ball around an exact midpoint never
//! becomes decisive, so these literal assertions are the ties' only
//! witnesses. Each delivery is asserted twice: against the value the
//! derivation predicts, spelled out in full, and against the format's
//! own decimal parser applied to the exact midpoint string, which
//! rounds it under the same mode through a path that shares no code
//! with the kernel.
//!
//! Each classified exact case is cross-checked against an independent
//! witness, the house two-proofs pattern: the delivered value plus one
//! must reproduce `2^n` exactly through the format's own addition, and
//! the negative side's coefficient `10^m − 5^m` must equal
//! `5^m(2^m − 1)` computed separately in `u128` integer arithmetic.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// The format's exact-family ceilings: `2^112 − 1` is exactly 34
/// digits, and `10^34 − 5^34` is exactly 34 digits.
const N_MAX: u32 = 112;
const M_MAX: u32 = 34;

/// The two tie inputs at this format: `2^116 − 1` is the one 35-digit
/// value with `4 | n`, and `m = PRECISION + 1 = 35` is always one.
const TIE_POS: u32 = 116;
const TIE_NEG: u32 = 35;

/// The exact midpoint `2^116 − 1`, spelled out.
const TIE_POS_MID: &str = "83076749736557242056487941267521535";
/// Its neighbour of larger magnitude (`…2154E+1`, the even one).
const TIE_POS_UP: &str = "83076749736557242056487941267521540";
/// Its neighbour of smaller magnitude (`…2153E+1`).
const TIE_POS_DOWN: &str = "83076749736557242056487941267521530";

/// The exact midpoint `2^−35 − 1`, spelled out.
const TIE_NEG_MID: &str = "-0.99999999997089616954326629638671875";
/// Its neighbour of larger magnitude (the even one).
const TIE_NEG_UP: &str = "-0.9999999999708961695432662963867188";
/// Its neighbour of smaller magnitude.
const TIE_NEG_DOWN: &str = "-0.9999999999708961695432662963867187";

/// The representable value one step toward zero from `−1`.
const MINUS_ONE_NEIGHBOUR: &str = "-0.9999999999999999999999999999999999";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

/// The format's own decimal parser under an explicit rounding mode:
/// the independent oracle for a value spelled out exactly, sharing no
/// code with the transcendental kernel.
fn parse_rm(s: &str, rm: RoundingMode) -> Decimal128 {
    Decimal128::parse_str(s, rm)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The integer input `n`, built through its decimal string.
fn int_input(n: i32) -> Decimal128 {
    parse(&format!("{n}"))
}

/// The positive exact family value `2^n − 1`, from integer arithmetic.
fn pos_value(n: u32) -> Decimal128 {
    parse(&format!("{}", (1u128 << n) - 1))
}

/// The negative exact family value `−(10^m − 5^m)·10^−m`.
fn neg_value(m: u32) -> Decimal128 {
    parse(&format!("-{}e-{m}", 10u128.pow(m) - 5u128.pow(m)))
}

fn assert_exact(got: (Decimal128, Status), want: Decimal128, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
    assert!(!st.inexact(), "{label}: §7.5 forbids INEXACT here");
}

/// Every `exp2m1(n) = 2^n − 1` in reach of 34 digits, all five modes.
#[test]
fn exact_family_positive_exhaustive_every_mode() {
    for n in 1..=N_MAX {
        let x = int_input(i32::try_from(n).unwrap());
        let want = pos_value(n);
        for rm in ALL {
            assert_exact(x.exp2_m1(rm), want, &format!("exp2_m1({n}) [{rm:?}]"));
        }
    }
}

/// Every `exp2m1(−m) = −(10^m − 5^m)·10^−m` in reach of 34 digits, all
/// five modes.
#[test]
fn exact_family_negative_exhaustive_every_mode() {
    for m in 1..=M_MAX {
        let x = int_input(-i32::try_from(m).unwrap());
        let want = neg_value(m);
        for rm in ALL {
            assert_exact(x.exp2_m1(rm), want, &format!("exp2_m1(-{m}) [{rm:?}]"));
        }
    }
}

/// Independent witness for both exact families: the delivered value
/// plus one must reproduce `2^n` exactly through the format's own
/// addition, and the negative side's coefficient must equal
/// `5^m(2^m − 1)` computed separately. Two proofs of the same boundary
/// fact.
#[test]
fn exact_family_matches_the_independent_witness() {
    for n in 1..=N_MAX {
        let (r, _) = int_input(i32::try_from(n).unwrap()).exp2_m1(NE);
        let pow2 = parse(&format!("{}", 1u128 << n));
        let (sum, st) = r.add(Decimal128::ONE, NE);
        assert!(eq(sum, pow2), "(2^{n} - 1) + 1 = {sum}, want 2^{n}");
        assert!(!st.inexact(), "(2^{n} - 1) + 1 is exact, got {st:?}");
    }
    for m in 1..=M_MAX {
        // The coefficient identity `10^m − 5^m = 5^m(2^m − 1)`, which
        // is what makes it odd and exactly `m` digits wide.
        let coef = 10u128.pow(m) - 5u128.pow(m);
        assert_eq!(
            coef,
            5u128.pow(m) * ((1u128 << m) - 1),
            "10^{m} - 5^{m} factors as 5^{m}(2^{m} - 1)"
        );
        assert_eq!(
            coef.to_string().len(),
            m as usize,
            "10^{m} - 5^{m} carries exactly {m} digits"
        );
        assert_eq!(coef % 10, 5, "10^{m} - 5^{m} ends in 5");
        let (r, _) = int_input(-i32::try_from(m).unwrap()).exp2_m1(NE);
        // `2^-m = 5^m · 10^-m` as an exact decimal fraction.
        let pow2 = parse(&format!("{}e-{m}", 5u128.pow(m)));
        let (sum, st) = r.add(Decimal128::ONE, NE);
        assert!(eq(sum, pow2), "(2^-{m} - 1) + 1 = {sum}, want 2^-{m}");
        assert!(!st.inexact(), "(2^-{m} - 1) + 1 is exact, got {st:?}");
    }
}

/// The two `Decimal128` rows of the derivation's tie table, literal,
/// every mode. `2^116 − 1` is a midpoint whose stripped coefficient
/// carries 35 digits ending in 5; `NearestEven` picks the even
/// neighbour (which here is also the away one), `TowardZero` and
/// `TowardNegative` the smaller magnitude, `TowardPositive` the
/// larger. `2^−35 − 1` mirrors it on the negative side, where
/// `TowardPositive` is the toward-zero direction.
///
/// §7.5: a tie delivery drops a nonzero digit in every mode, so
/// `INEXACT` is raised on every one of them.
#[test]
fn tie_table_positive_side_literal() {
    let x = int_input(i32::try_from(TIE_POS).unwrap());
    let mid = parse(TIE_POS_MID);
    let up = parse(TIE_POS_UP);
    let down = parse(TIE_POS_DOWN);
    for (rm, want) in [(NE, up), (NA, up), (TZ, down), (TP, up), (TN, down)] {
        let (r, st) = x.exp2_m1(rm);
        assert!(
            eq(r, want),
            "exp2_m1({TIE_POS}) [{rm:?}]: got {r}, want {want}"
        );
        assert!(
            st.inexact(),
            "exp2_m1({TIE_POS}) [{rm:?}] drops a nonzero digit: {st:?}"
        );
        // The format's own parser on the exact midpoint string is the
        // independent oracle: it rounds the same value under the same
        // mode with no kernel code in the path.
        assert!(
            eq(r, parse_rm(TIE_POS_MID, rm)),
            "exp2_m1({TIE_POS}) [{rm:?}] disagrees with parse_str on the midpoint"
        );
    }
    // The midpoint itself is exactly the exact integer value, and it
    // really does sit halfway between the two delivered neighbours.
    // Checked in `u128` integer arithmetic, the only arena where the
    // statement is exact: a 35-digit midpoint is by definition not
    // representable at 34 digits, so `parse` of `mid` above is already
    // a rounding and cannot carry this proof.
    let mid_i = (1u128 << TIE_POS) - 1;
    let up_i: u128 = TIE_POS_UP.parse().expect("neighbour literal is a u128");
    let down_i: u128 = TIE_POS_DOWN.parse().expect("neighbour literal is a u128");
    assert_eq!(
        mid_i.to_string(),
        TIE_POS_MID,
        "the tie table's midpoint literal is 2^{TIE_POS} - 1"
    );
    assert_eq!(mid_i % 10, 5, "a midpoint's last digit is 5");
    assert_eq!(
        mid_i - down_i,
        up_i - mid_i,
        "the literal sits exactly halfway between the neighbours"
    );
    // `mid` is the parser's own NearestEven rounding of the midpoint,
    // which must land on the same neighbour the kernel picked.
    assert!(eq(mid, up), "parse_str(midpoint) [NearestEven] = {mid}");
}

/// The negative-side `Decimal128` tie row; see
/// [`tie_table_positive_side_literal`].
#[test]
fn tie_table_negative_side_literal() {
    let x = int_input(-i32::try_from(TIE_NEG).unwrap());
    let mid = parse(TIE_NEG_MID);
    let up = parse(TIE_NEG_UP);
    let down = parse(TIE_NEG_DOWN);
    for (rm, want) in [(NE, up), (NA, up), (TZ, down), (TP, down), (TN, up)] {
        let (r, st) = x.exp2_m1(rm);
        assert!(
            eq(r, want),
            "exp2_m1(-{TIE_NEG}) [{rm:?}]: got {r}, want {want}"
        );
        assert!(
            st.inexact(),
            "exp2_m1(-{TIE_NEG}) [{rm:?}] drops a nonzero digit: {st:?}"
        );
        assert!(
            eq(r, parse_rm(TIE_NEG_MID, rm)),
            "exp2_m1(-{TIE_NEG}) [{rm:?}] disagrees with parse_str on the midpoint"
        );
    }
    // Same two proofs on the negative side, on coefficients scaled by
    // `10^35` so the comparison is exact integer arithmetic. The
    // neighbours carry 34 digits, so scaling them up costs one factor
    // of ten each.
    let mid_i = 10u128.pow(TIE_NEG) - 5u128.pow(TIE_NEG);
    let up_i: u128 = 10 * strip_fraction(TIE_NEG_UP);
    let down_i: u128 = 10 * strip_fraction(TIE_NEG_DOWN);
    assert_eq!(
        format!("-0.{mid_i}"),
        TIE_NEG_MID,
        "the tie table's midpoint literal is 2^-{TIE_NEG} - 1"
    );
    assert_eq!(mid_i % 10, 5, "a midpoint's last digit is 5");
    assert_eq!(
        up_i - mid_i,
        mid_i - down_i,
        "the literal sits exactly halfway between the neighbours"
    );
    assert!(eq(mid, up), "parse_str(midpoint) [NearestEven] = {mid}");
}

/// The fractional digits of a `-0.<digits>` literal as a `u128`.
fn strip_fraction(s: &str) -> u128 {
    s.trim_start_matches("-0.")
        .parse()
        .expect("neighbour literal is -0.<digits>")
}

/// Neighbour probes at both window edges and either side of the ties.
/// Inside the exact window the result is exact; one step out it is
/// inexact but still the correctly rounded value of the exact integer,
/// which the format's own parser supplies independently.
#[test]
fn window_edges_and_tie_neighbours() {
    // Positive side: 112 exact, 113 / 115 / 116 / 117 inexact.
    let exact_edge: [(u32, &str); 1] = [(N_MAX, "5192296858534827628530496329220095")];
    for (n, literal) in exact_edge {
        for rm in ALL {
            assert_exact(
                int_input(i32::try_from(n).unwrap()).exp2_m1(rm),
                parse(literal),
                &format!("exp2_m1({n}) [{rm:?}]"),
            );
        }
    }
    let inexact_pos: [(u32, &str); 4] = [
        (113, "10384593717069655257060992658440191"),
        (115, "41538374868278621028243970633760767"),
        (TIE_POS, TIE_POS_MID),
        (117, "166153499473114484112975882535043071"),
    ];
    for (n, literal) in inexact_pos {
        assert_eq!(
            literal,
            format!("{}", (1u128 << n) - 1),
            "probe literal is 2^{n} - 1"
        );
        for rm in ALL {
            let (r, st) = int_input(i32::try_from(n).unwrap()).exp2_m1(rm);
            assert!(
                st.inexact(),
                "exp2_m1({n}) [{rm:?}] must be INEXACT: {st:?}"
            );
            assert!(
                eq(r, parse_rm(literal, rm)),
                "exp2_m1({n}) [{rm:?}] = {r} is not the correctly rounded 2^{n} - 1"
            );
        }
    }
    // Negative side: 34 exact, 35 (the tie) and 36 inexact.
    for rm in ALL {
        assert_exact(
            int_input(-i32::try_from(M_MAX).unwrap()).exp2_m1(rm),
            neg_value(M_MAX),
            &format!("exp2_m1(-{M_MAX}) [{rm:?}]"),
        );
    }
    for m in [TIE_NEG, 36] {
        let literal = format!("-0.{}", 10u128.pow(m) - 5u128.pow(m));
        for rm in ALL {
            let (r, st) = int_input(-i32::try_from(m).unwrap()).exp2_m1(rm);
            assert!(
                st.inexact(),
                "exp2_m1(-{m}) [{rm:?}] must be INEXACT: {st:?}"
            );
            assert!(
                eq(r, parse_rm(&literal, rm)),
                "exp2_m1(-{m}) [{rm:?}] = {r} is not the correctly rounded 2^-{m} - 1"
            );
        }
    }
}

/// The `−1` band: past `x ≈ −173` the true value sits strictly inside
/// `(−1, −1 + 10^−52)`, so the nearest modes and `TowardNegative`
/// deliver `−1` while `TowardZero` and `TowardPositive` deliver its
/// toward-zero neighbour, always `INEXACT` and never an exception.
#[test]
fn minus_one_band_spot_rows() {
    let neighbour = parse(MINUS_ONE_NEIGHBOUR);
    for label in ["-200", "-1000", "-1e6", "-1e100", "-9.999999e6144"] {
        let x = parse(label);
        for (rm, want) in [
            (NE, Decimal128::NEG_ONE),
            (NA, Decimal128::NEG_ONE),
            (TZ, neighbour),
            (TP, neighbour),
            (TN, Decimal128::NEG_ONE),
        ] {
            let (r, st) = x.exp2_m1(rm);
            assert!(
                eq(r, want),
                "exp2_m1({label}) [{rm:?}]: got {r}, want {want}"
            );
            assert!(
                st.inexact(),
                "exp2_m1({label}) [{rm:?}] must be INEXACT: {st:?}"
            );
            assert!(
                !st.invalid() && !st.div_by_zero() && !st.overflow(),
                "exp2_m1({label}) [{rm:?}] raised an exception: {st:?}"
            );
        }
    }
    // Just inside the gate, where the working value collapses onto the
    // `−1` anchor rather than saturating: same deliveries.
    for label in ["-160", "-170"] {
        let x = parse(label);
        let (r, st) = x.exp2_m1(NE);
        assert!(eq(r, Decimal128::NEG_ONE), "exp2_m1({label}) = {r}");
        assert!(st.inexact(), "exp2_m1({label}) status {st:?}");
        let (r, _) = x.exp2_m1(TZ);
        assert!(eq(r, neighbour), "exp2_m1({label}) [TowardZero] = {r}");
    }
}

/// The overflow band: past `x ≈ +20414` the §7.4 disposition applies
/// per direction, `+∞` at the nearest modes and toward `+∞`, the
/// largest finite magnitude toward zero and `−∞`.
#[test]
fn overflow_band_spot_rows() {
    for label in ["30000", "1e6", "1e100", "9.999999e6144"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.exp2_m1(rm);
            assert!(
                st.overflow() && st.inexact(),
                "exp2_m1({label}) [{rm:?}] must raise OVERFLOW + INEXACT: {st:?}"
            );
            let want_inf = matches!(rm, NE | NA | TP);
            if want_inf {
                assert!(
                    r.is_infinite() && !r.is_sign_negative(),
                    "exp2_m1({label}) [{rm:?}] = {r}, want +inf"
                );
            } else {
                assert!(
                    eq(r, Decimal128::MAX),
                    "exp2_m1({label}) [{rm:?}] = {r}, want MAX"
                );
            }
        }
    }
}

/// IEEE 754-2019 §9.2.1 special values, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        // ±0 → ±0, sign preserved, no exception.
        let (r, st) = Decimal128::ZERO.exp2_m1(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "exp2_m1(+0) = {r}");
        assert_eq!(st, Status::OK, "exp2_m1(+0) status {st:?}");
        let (r, st) = Decimal128::NEG_ZERO.exp2_m1(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "exp2_m1(-0) = {r}");
        assert_eq!(st, Status::OK, "exp2_m1(-0) status {st:?}");

        // −∞ → −1 exactly, no exception.
        let (r, st) = Decimal128::NEG_INFINITY.exp2_m1(rm);
        assert!(eq(r, Decimal128::NEG_ONE), "exp2_m1(-inf) = {r}");
        assert_eq!(st, Status::OK, "exp2_m1(-inf) status {st:?}");

        // +∞ → +∞.
        let (r, st) = Decimal128::INFINITY.exp2_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp2_m1(+inf) = {r}"
        );
        assert_eq!(st, Status::OK, "exp2_m1(+inf) status {st:?}");

        // NaN propagation; sNaN raises INVALID.
        let (r, st) = Decimal128::NAN.exp2_m1(rm);
        assert!(r.is_nan() && st.is_ok(), "exp2_m1(NaN) = {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.exp2_m1(rm);
        assert!(r.is_nan() && st.invalid(), "exp2_m1(sNaN) = {r} {st:?}");
    }
}

/// Flag honesty (§7.5): generic finite inputs are inexact in every
/// mode, and every exact classifier delivery is exact in every mode.
#[test]
fn flag_honesty_across_modes() {
    for label in [
        "0.1",
        "2.5",
        "-0.3",
        "-1.5",
        "1e100",
        "1e-100",
        "1e-6100",
        "-1e-6100",
        "12345.6789",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.exp2_m1(rm);
            assert!(st.inexact(), "exp2_m1({label}) [{rm:?}] status {st:?}");
        }
    }
    for n in [1u32, 2, 3, 57, N_MAX] {
        let x = int_input(i32::try_from(n).unwrap());
        for rm in ALL {
            let (_, st) = x.exp2_m1(rm);
            assert!(
                !st.inexact(),
                "exp2_m1({n}) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
    for m in [1u32, 2, 3, 17, M_MAX] {
        let x = int_input(-i32::try_from(m).unwrap());
        for rm in ALL {
            let (_, st) = x.exp2_m1(rm);
            assert!(
                !st.inexact(),
                "exp2_m1(-{m}) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
}

/// The named exact values the public rustdoc quotes, pinned so the
/// documentation and the kernel cannot drift apart.
#[test]
fn documented_exact_values() {
    for rm in ALL {
        assert_exact(parse("1").exp2_m1(rm), parse("1"), "exp2_m1(1)");
        assert_exact(parse("2").exp2_m1(rm), parse("3"), "exp2_m1(2)");
        assert_exact(parse("3").exp2_m1(rm), parse("7"), "exp2_m1(3)");
        assert_exact(parse("10").exp2_m1(rm), parse("1023"), "exp2_m1(10)");
        assert_exact(parse("-1").exp2_m1(rm), parse("-0.5"), "exp2_m1(-1)");
        assert_exact(parse("-2").exp2_m1(rm), parse("-0.75"), "exp2_m1(-2)");
        assert_exact(parse("-3").exp2_m1(rm), parse("-0.875"), "exp2_m1(-3)");
    }
}

/// The classifier decides on the *stripped* form, so an exact family
/// member handed in any cohort of its value must classify the same
/// way. Trailing zeros move the stored exponent, which is exactly the
/// quantity `as_small_int` reads, so this is the cohort witness for
/// that decode.
#[test]
fn cohorts_of_an_exact_input_classify_alike() {
    let cases: [(&str, &str); 9] = [
        ("3", "7"),
        ("3.0", "7"),
        ("3.000000000000000000000000000000000", "7"),
        ("1e1", "1023"),
        ("10.00", "1023"),
        ("100", "1267650600228229401496703205375"),
        ("-2", "-0.75"),
        ("-2.00", "-0.75"),
        ("-2.000000000000000000000000000000000", "-0.75"),
    ];
    for (label, want) in cases {
        for rm in ALL {
            assert_exact(
                parse(label).exp2_m1(rm),
                parse(want),
                &format!("exp2_m1({label}) [{rm:?}]"),
            );
        }
    }
}

/// One step off an exact family member is not exact: the classifier
/// must decline, and the kernel must then raise `INEXACT`. Guards the
/// `as_small_int` bail against a classifier that over-claims on a
/// non-integer.
#[test]
fn near_misses_are_declined() {
    for label in [
        // Non-integers either side of exact integer inputs (34 digits
        // each, so the parse keeps them off the integer grid).
        "0.5",
        "1.5",
        "2.5",
        "3.000000000000000000000000000000001",
        "-0.5",
        "-1.5",
        "-2.5",
        "-1.999999999999999999999999999999999",
        // Integers past the `as_small_int` limit on both sides.
        "131",
        "200",
        "1000",
        "-131",
        "-200",
        "-1000",
        // Integers inside the limit but past the width gate.
        "118",
        "127",
        "128",
        "130",
        "-40",
        "-100",
        "-130",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.exp2_m1(rm);
            assert!(
                st.inexact(),
                "exp2_m1({label}) [{rm:?}] is not exact: {st:?}"
            );
        }
    }
}

/// Tiny inputs, where the `expm1` series collapses to `u = x · ln 2`
/// and only that scaling separates the result from the input's own
/// grid point. `exp2m1` carries no x-anchor seam by design (its slope
/// at zero is `ln 2`, not 1), so this is the executable witness that
/// the ladder still terminates and delivers there, on every rung the
/// test lanes route through, down to the subnormal floor.
#[test]
fn tiny_inputs_deliver_without_an_x_anchor_seam() {
    for label in [
        "1e-40", "-1e-40", "1e-3000", "-1e-3000", "1e-6100", "-1e-6100", "1e-6170", "-1e-6170",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.exp2_m1(rm);
            assert!(
                !r.is_nan() && !r.is_infinite(),
                "exp2_m1({label}) [{rm:?}] left the finite range: {r}"
            );
            assert!(
                st.inexact(),
                "exp2_m1({label}) [{rm:?}] must be INEXACT: {st:?}"
            );
        }
        let (r, _) = x.exp2_m1(NE);
        assert_eq!(
            r.is_sign_negative(),
            x.is_sign_negative(),
            "exp2_m1({label}) flipped the sign: {r}"
        );
        // `|2^x − 1| < |x|` for a tiny `x`: the slope `ln 2 ≈ 0.693`
        // is below 1 and dominates the second order term. This is the
        // property the missing anchor seam rests on, so it is asserted
        // rather than assumed. Away from the subnormal floor only,
        // where the format quantum rather than the slope decides.
        if !r.is_subnormal() {
            assert!(
                r.abs().partial_cmp(x.abs()).0 == Some(Ordering::Less),
                "exp2_m1({label}) = {r} did not fall inside |{label}|"
            );
        }
    }
}
