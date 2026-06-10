//! Directed-mode regression gate for the transcendental special
//! paths (fd-aqs.5).
//!
//! The 2026-06-09 review found four families of directed-mode defects
//! in the kernel's special-value and saturation paths, all invisible
//! to the faithful (≤ 1 ULP) astro-float suites by construction and
//! outside the Arb corpus decades:
//!
//! 1. **Mode-blind overflow/underflow gates.** `exp_from_extended`
//!    returned `+∞ + OVERFLOW` / `+0 + UNDERFLOW` at every mode;
//!    IEEE 754-2019 §7.4 requires the largest finite number at
//!    `TowardZero`/`TowardNegative` overflow and the smallest
//!    subnormal at `TowardPositive` underflow-to-zero. Reaches
//!    `exp`, `exp2`, and `pow` extremes.
//! 2. **Negate-after-round.** `asin(±1)`, `atan(±∞)`, the `atan2`
//!    quadrant constants, and `pow` with a negative base and odd
//!    integer exponent rounded the magnitude under the caller's mode
//!    and then negated, swapping `TowardPositive`/`TowardNegative`
//!    (the fd-r5m class; `cbrt` carries the `for_negation()` fix).
//! 3. **Mode-blind `tanh` saturation.** `|x| > 80` returned exactly
//!    `±1` at every mode where `TowardZero` requires the
//!    all-nines neighbour, and the band `~58 < |x| ≤ 80` reproduced
//!    the same defect through the extended quotient rounding to 1.
//! 4. **`atan2(±0, −0)` flag fidelity.** The prescribed result `±π`
//!    is inexact; the path returned `OK` while its finite-x sibling
//!    correctly raised `INEXACT`.
//!
//! Every expected value below is the correctly rounded 34-digit
//! result derived from the exact constant (π family) or exact integer
//! arithmetic (`11^35`), cross-checked against libmpdec/mpmath at
//! review time. Value equality is the cohort-insensitive IEEE
//! compare, matching the frozen-corpus gate.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE).unwrap().0
}

fn assert_val(got: Decimal128, want: &str, label: &str) {
    let want_d = parse(want);
    let (cmp, _) = got.partial_cmp(want_d);
    assert_eq!(
        cmp,
        Some(core::cmp::Ordering::Equal),
        "{label}: got {got:?}, want {want}"
    );
}

// ---------------------------------------------------------------------------
// 1. exp / exp2 / pow overflow and underflow gates (§7.4 disposition).

#[test]
fn exp_overflow_directed_modes() {
    let x = parse("14151");
    let max = "9.999999999999999999999999999999999E+6144";
    for (rm, want_inf) in [(NE, true), (NA, true), (TP, true), (TZ, false), (TN, false)] {
        let (r, st) = x.exp(rm);
        if want_inf {
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "exp(14151) {rm:?}: {r:?}"
            );
        } else {
            assert_val(r, max, "exp(14151) toward-zero side");
        }
        assert!(
            st.overflow() && st.inexact(),
            "exp(14151) {rm:?} flags: {st:?}"
        );
    }
}

#[test]
fn exp_underflow_directed_modes() {
    let x = parse("-14225");
    for rm in [NE, NA, TZ, TN] {
        let (r, st) = x.exp(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp(-14225) {rm:?}: {r:?}"
        );
        assert!(
            st.underflow() && st.inexact(),
            "exp(-14225) {rm:?} flags: {st:?}"
        );
    }
    // TowardPositive must deliver the smallest subnormal, not zero.
    let (r, st) = x.exp(TP);
    assert_val(r, "1E-6176", "exp(-14225) TowardPositive");
    assert!(
        st.underflow() && st.inexact(),
        "exp(-14225) TP flags: {st:?}"
    );
}

#[test]
fn exp2_gate_directed_modes() {
    let (r, st) = parse("47000").exp2(TZ);
    assert_val(
        r,
        "9.999999999999999999999999999999999E+6144",
        "exp2(47000) TowardZero",
    );
    assert!(st.overflow() && st.inexact());
    let (r, _) = parse("47000").exp2(NE);
    assert!(r.is_infinite() && !r.is_sign_negative());

    let (r, st) = parse("-48000").exp2(TP);
    assert_val(r, "1E-6176", "exp2(-48000) TowardPositive");
    assert!(st.underflow() && st.inexact());
    let (r, _) = parse("-48000").exp2(NE);
    assert!(r.is_zero() && !r.is_sign_negative());
}

#[test]
fn pow_gate_directed_modes() {
    let ten = parse("10");
    let (r, st) = ten.pow(parse("7000"), TZ);
    assert_val(
        r,
        "9.999999999999999999999999999999999E+6144",
        "pow(10, 7000) TowardZero",
    );
    assert!(st.overflow() && st.inexact());

    let (r, st) = ten.pow(parse("-7000"), TP);
    assert_val(r, "1E-6176", "pow(10, -7000) TowardPositive");
    assert!(st.underflow() && st.inexact());
}

// ---------------------------------------------------------------------------
// 2. Negate-after-round: asin(±1), atan(±∞), atan2 constants, odd pow.
//
// π/2 to 34 digits is 1.570796326794896619231321691639751|44…, so the
// magnitude rounds down at NE/TZ and up only toward +∞ (and, for a
// negative result, toward −∞).

#[test]
fn asin_one_directed_modes() {
    let one = parse("1");
    let lo = "1.570796326794896619231321691639751";
    let hi = "1.570796326794896619231321691639752";
    for (rm, want) in [(NE, lo), (NA, lo), (TZ, lo), (TN, lo), (TP, hi)] {
        let (r, st) = one.asin(rm);
        assert_val(r, want, "asin(1)");
        assert!(st.inexact(), "asin(1) {rm:?} flags: {st:?}");
    }
}

#[test]
fn asin_neg_one_directed_modes() {
    let neg_one = parse("-1");
    let lo = "-1.570796326794896619231321691639751";
    let hi = "-1.570796326794896619231321691639752";
    for (rm, want) in [(NE, lo), (NA, lo), (TZ, lo), (TP, lo), (TN, hi)] {
        let (r, st) = neg_one.asin(rm);
        assert_val(r, want, "asin(-1)");
        assert!(st.inexact(), "asin(-1) {rm:?} flags: {st:?}");
    }
}

#[test]
fn atan_infinities_directed_modes() {
    let lo = "1.570796326794896619231321691639751";
    let hi = "1.570796326794896619231321691639752";
    for (rm, want) in [(NE, lo), (TZ, lo), (TN, lo), (TP, hi)] {
        let (r, _) = Decimal128::INFINITY.atan(rm);
        assert_val(r, want, "atan(+inf)");
    }
    for (rm, want) in [(NE, lo), (TZ, lo), (TP, lo), (TN, hi)] {
        let (r, _) = Decimal128::NEG_INFINITY.atan(rm);
        assert_val(r, &alloc_neg(want), "atan(-inf)");
    }
}

/// Prefix a `-` onto an expected-value literal.
fn alloc_neg(s: &str) -> String {
    format!("-{s}")
}

#[test]
fn acos_neg_one_directed_modes() {
    // Control: acos(−1) = +π is rounded directly (no negation step)
    // and was already mode-correct; pin it so the fix cannot regress it.
    let neg_one = parse("-1");
    let lo = "3.141592653589793238462643383279502";
    let hi = "3.141592653589793238462643383279503";
    for (rm, want) in [(NE, hi), (TZ, lo), (TN, lo), (TP, hi)] {
        let (r, _) = neg_one.acos(rm);
        assert_val(r, want, "acos(-1)");
    }
}

#[test]
fn atan2_negative_y_constants_directed_modes() {
    let pi_lo = "3.141592653589793238462643383279502";
    let pi_hi = "3.141592653589793238462643383279503";
    // atan2(−1, −∞) = −π: toward −∞ takes the larger magnitude.
    let m_one = parse("-1");
    for (rm, want) in [(NE, pi_hi), (TZ, pi_lo), (TP, pi_lo), (TN, pi_hi)] {
        let (r, _) = m_one.atan2(Decimal128::NEG_INFINITY, rm);
        assert_val(r, &alloc_neg(want), "atan2(-1, -inf)");
    }
    // Control, positive y: atan2(1, −∞) = +π.
    let one = parse("1");
    for (rm, want) in [(NE, pi_hi), (TZ, pi_lo), (TN, pi_lo), (TP, pi_hi)] {
        let (r, _) = one.atan2(Decimal128::NEG_INFINITY, rm);
        assert_val(r, want, "atan2(1, -inf)");
    }

    // atan2(−∞, −∞) = −3π/4 = −2.356194490192344928846982537459627|16…
    let tq_lo = "2.356194490192344928846982537459627";
    let tq_hi = "2.356194490192344928846982537459628";
    for (rm, want) in [(NE, tq_lo), (TZ, tq_lo), (TP, tq_lo), (TN, tq_hi)] {
        let (r, _) = Decimal128::NEG_INFINITY.atan2(Decimal128::NEG_INFINITY, rm);
        assert_val(r, &alloc_neg(want), "atan2(-inf, -inf)");
    }

    // atan2(−∞, +∞) = −π/4 = −0.7853981633974483096156608458198757|21…
    let q_lo = "0.7853981633974483096156608458198757";
    let q_hi = "0.7853981633974483096156608458198758";
    for (rm, want) in [(NE, q_lo), (TZ, q_lo), (TP, q_lo), (TN, q_hi)] {
        let (r, _) = Decimal128::NEG_INFINITY.atan2(Decimal128::INFINITY, rm);
        assert_val(r, &alloc_neg(want), "atan2(-inf, +inf)");
    }

    // atan2(−∞, 5) = −π/2.
    let hp_lo = "1.570796326794896619231321691639751";
    let hp_hi = "1.570796326794896619231321691639752";
    for (rm, want) in [(NE, hp_lo), (TZ, hp_lo), (TP, hp_lo), (TN, hp_hi)] {
        let (r, _) = Decimal128::NEG_INFINITY.atan2(parse("5"), rm);
        assert_val(r, &alloc_neg(want), "atan2(-inf, 5)");
    }
}

#[test]
fn atan2_zero_neg_zero_value_and_flags() {
    // atan2(±0, −0) = ±π, and π is not representable: INEXACT, matching
    // the finite-x<0 path one arm below it.
    let pi_lo = "3.141592653589793238462643383279502";
    let pi_hi = "3.141592653589793238462643383279503";
    let pz = parse("0");
    let nz = pz.neg();
    for (rm, want) in [(NE, pi_hi), (TZ, pi_lo), (TN, pi_lo), (TP, pi_hi)] {
        let (r, st) = pz.atan2(nz, rm);
        assert_val(r, want, "atan2(+0, -0)");
        assert!(st.inexact(), "atan2(+0, -0) {rm:?} flags: {st:?}");
    }
    for (rm, want) in [(NE, pi_hi), (TZ, pi_lo), (TP, pi_lo), (TN, pi_hi)] {
        let (r, st) = nz.atan2(nz, rm);
        assert_val(r, &alloc_neg(want), "atan2(-0, -0)");
        assert!(st.inexact(), "atan2(-0, -0) {rm:?} flags: {st:?}");
    }
    // Control: atan2(±0, +0) = ±0 exactly, OK.
    let (r, st) = pz.atan2(pz, NE);
    assert!(r.is_zero() && !r.is_sign_negative());
    assert_eq!(st, Status::OK);
}

#[test]
fn pow_negative_base_odd_exponent_directed_modes() {
    // (−1.1)^35 = −11^35 / 10^35; 11^35 has 37 digits
    // (…1213903353404|851), so the magnitude is inexact at 34 digits
    // and directed rounding of the negative result is observable.
    let base = parse("-1.1");
    let y = parse("35");
    let mag_dn = "28.10243684806424785061213903353404";
    let mag_up = "28.10243684806424785061213903353405";
    for (rm, want) in [(NE, mag_up), (TZ, mag_dn), (TP, mag_dn), (TN, mag_up)] {
        let (r, st) = base.pow(y, rm);
        assert_val(r, &alloc_neg(want), "pow(-1.1, 35)");
        assert!(st.inexact(), "pow(-1.1, 35) {rm:?} flags: {st:?}");
    }
}

// ---------------------------------------------------------------------------
// 3. tanh saturation per mode (|tanh x| < 1 strictly for finite x).

#[test]
fn tanh_saturation_directed_modes() {
    let nines = "0.9999999999999999999999999999999999";
    for x_str in ["50", "60", "100", "5000"] {
        let x = parse(x_str);
        for (rm, want) in [(NE, "1"), (NA, "1"), (TP, "1"), (TZ, nines), (TN, nines)] {
            let (r, st) = x.tanh(rm);
            assert_val(r, want, &format!("tanh({x_str})"));
            assert!(st.inexact(), "tanh({x_str}) {rm:?} flags: {st:?}");
        }
        let neg_x = x.neg();
        for (rm, want) in [
            (NE, "-1"),
            (TN, "-1"),
            (TZ, &alloc_neg(nines)[..]),
            (TP, &alloc_neg(nines)[..]),
        ] {
            let (r, st) = neg_x.tanh(rm);
            assert_val(r, want, &format!("tanh(-{x_str})"));
            assert!(st.inexact(), "tanh(-{x_str}) {rm:?} flags: {st:?}");
        }
    }
    // Exact special stays exact: tanh(±∞) = ±1 with no flags.
    let (r, st) = Decimal128::INFINITY.tanh(TZ);
    assert_val(r, "1", "tanh(+inf)");
    assert_eq!(st, Status::OK);
}
