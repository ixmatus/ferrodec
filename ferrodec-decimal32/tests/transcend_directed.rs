//! Directed-mode regression gate for the transcendental special
//! paths at `Decimal32` (fd-aqs.5) — the sibling mirror of the root
//! crate's `tests/transcend_directed.rs`, pinning the per-format
//! saturation thresholds and rounded constants. The kernel is shared
//! (`ferrodec-transcend`), so the root file carries the full case
//! table; this mirror guards the format-specific seams: the
//! `exp_overflow_limit` / `exp_underflow_limit` figures (224 / 235),
//! the §7.4 dispositions at the `Decimal32` range boundaries, and the
//! 7-digit roundings of the π-family constants.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE).unwrap().0
}

fn assert_val(got: Decimal32, want: &str, label: &str) {
    let want_d = parse(want);
    let (cmp, _) = got.partial_cmp(want_d);
    assert_eq!(
        cmp,
        Some(core::cmp::Ordering::Equal),
        "{label}: got {got:?}, want {want}"
    );
}

#[test]
fn exp_gates_directed_modes() {
    // Overflow gate fires past 224.
    let (r, st) = parse("225").exp(TZ);
    assert_val(r, "9.999999E+96", "exp(225) TowardZero");
    assert!(st.overflow() && st.inexact());
    let (r, _) = parse("225").exp(NE);
    assert!(r.is_infinite() && !r.is_sign_negative());

    // Underflow gate fires past 235; TowardPositive delivers the
    // smallest subnormal.
    let (r, st) = parse("-236").exp(TP);
    assert_val(r, "1E-101", "exp(-236) TowardPositive");
    assert!(st.underflow() && st.inexact());
    let (r, st) = parse("-236").exp(NE);
    assert!(r.is_zero() && !r.is_sign_negative());
    assert!(st.underflow() && st.inexact());
}

#[test]
fn asin_neg_one_directed_modes() {
    // π/2 at 7 digits: 1.570796|326….
    let neg_one = parse("-1");
    for (rm, want) in [
        (NE, "-1.570796"),
        (TZ, "-1.570796"),
        (TP, "-1.570796"),
        (TN, "-1.570797"),
    ] {
        let (r, st) = neg_one.asin(rm);
        assert_val(r, want, "asin(-1)");
        assert!(st.inexact());
    }
}

#[test]
fn atan2_negative_y_constant_directed_modes() {
    // atan2(−1, −∞) = −π; π at 7 digits is 3.141592|653….
    let m_one = parse("-1");
    for (rm, want) in [
        (NE, "-3.141593"),
        (TZ, "-3.141592"),
        (TP, "-3.141592"),
        (TN, "-3.141593"),
    ] {
        let (r, _) = m_one.atan2(Decimal32::NEG_INFINITY, rm);
        assert_val(r, want, "atan2(-1, -inf)");
    }
    // Flag fidelity: atan2(−0, −0) = −π is INEXACT.
    let nz = parse("0").neg();
    let (r, st) = nz.atan2(nz, NE);
    assert_val(r, "-3.141593", "atan2(-0, -0)");
    assert!(st.inexact());
    let _ = Status::OK;
}

#[test]
fn tanh_saturation_directed_modes() {
    let (r, st) = parse("100").tanh(TZ);
    assert_val(r, "0.9999999", "tanh(100) TowardZero");
    assert!(st.inexact());
    let (r, _) = parse("100").tanh(NE);
    assert_val(r, "1", "tanh(100) NearestEven");
    let (r, _) = parse("-60").tanh(TP);
    assert_val(r, "-0.9999999", "tanh(-60) TowardPositive");
    let (r, _) = parse("-60").tanh(TN);
    assert_val(r, "-1", "tanh(-60) TowardNegative");
}
