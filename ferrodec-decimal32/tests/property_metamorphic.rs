//! Metamorphic identity cross-checks for the `Decimal32`
//! transcendentals (ADR-0025). The decimal32 analogue of
//! `ferrodec/tests/property_metamorphic.rs`; see that file's module
//! docs and ADR-0025 for the design (oracle-skip backstop, the
//! tautology audit, condition-number-derived bands).
//!
//! Probe magnitudes are scaled to the `Decimal32` range: 7 significant
//! digits, exponent range to ≈`1e96`, so `exp(x)` overflows past
//! `|x| ≈ 221` (= `96·ln 10`). The `exp∘ln` probes still reach
//! `1e±85`, far past the astro-float oracle's sound domain for this
//! format.

#![cfg(feature = "transcendentals")]

use ferrodec_decimal32::RoundingMode;
use proptest::prelude::*;

mod common;
use common::{parse, within_n_ulp_band};

use ferrodec_test_support::transcend_oracle::Consts;

const NE: RoundingMode = RoundingMode::NearestEven;

/// Safety constant on every derived band (ADR-0025).
const C: f64 = 4.0;

/// `ln(10)`.
const LN10: f64 = core::f64::consts::LN_10;

/// Band for an `exp`/`ln`-amplified round-trip whose argument has
/// decimal exponent magnitude `exp_abs`. A `Decimal32` coefficient is
/// `< 10^7`, so `ln(coef) < 80` with wide margin (the constant is
/// shared across the formats and conservative here).
fn n_explog(exp_abs: u32) -> u32 {
    (C * (f64::from(exp_abs) * LN10 + 80.0 + 1.0)).ceil() as u32
}

/// Band `C·(cond + 1)` from a condition number already expressed in
/// **ULP-of-x units** (see `ferrodec/tests/property_metamorphic.rs` and
/// ADR-0025 for why the magnitude-ratio term matters), capped.
fn n_cond(cond_in_ulp_of_x: f64) -> Option<u32> {
    let n = (C * (cond_in_ulp_of_x + 1.0)).ceil();
    if !n.is_finite() || n > 5.0e7 {
        None
    } else {
        Some(n as u32)
    }
}

// --- Category A: independent cross-computation, well-conditioned. ---

/// `pow(x,2) == x*x`. `x²` kept `< 1e96`.
#[test]
fn pow_sq_equals_mul() {
    let mut cc = Consts::new().expect("consts");
    for s in ["2", "3.5", "1e-20", "1e20", "9.999999e10", "7e40"] {
        let x = parse(s);
        let (lhs, _) = x.pow(parse("2"), NE);
        let (rhs, _) = x.mul(x, NE);
        assert!(
            within_n_ulp_band(lhs, rhs, 4, &mut cc),
            "pow({s},2) vs {s}*{s}: {lhs:?} vs {rhs:?}"
        );
    }
}

/// `pow(x,0.5) == sqrt(x)`, `x > 0`.
#[test]
fn pow_half_equals_sqrt() {
    let mut cc = Consts::new().expect("consts");
    for s in ["2", "100", "1e-40", "1e40", "3.000001", "5e80"] {
        let x = parse(s);
        let (lhs, _) = x.pow(parse("0.5"), NE);
        let (rhs, _) = x.sqrt(NE);
        assert!(
            within_n_ulp_band(lhs, rhs, 4, &mut cc),
            "pow({s},0.5) vs sqrt({s}): {lhs:?} vs {rhs:?}"
        );
    }
}

/// `ln(exp(x)) ≈ x`, `exp(x)` finite (`|x| < 221`).
#[test]
fn ln_exp_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for s in ["1", "-1", "0.5", "50", "-50", "100", "-100", "200", "-200"] {
        let x = parse(s);
        let xf: f64 = s.parse().expect("probe is f64");
        let (e, _) = x.exp(NE);
        let (back, _) = e.ln(NE);
        let n = n_cond(1.0 / xf.abs() + 1.0).expect("ln∘exp well-conditioned");
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "ln(exp({s})) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `atan2(sin x, cos x) ≈ x` on `(−π, π]`.
#[test]
fn atan2_sincos_recovers_angle() {
    let mut cc = Consts::new().expect("consts");
    for s in [
        "0",
        "0.5",
        "-0.5",
        "1.570796",
        "-1.570796",
        "3.0",
        "-3.0",
        "2.356194",
        "-2.356194",
    ] {
        let x = parse(s);
        let (sn, _) = x.sin(NE);
        let (cs, _) = x.cos(NE);
        let (got, _) = sn.atan2(cs, NE);
        assert!(
            within_n_ulp_band(got, x, 8, &mut cc),
            "atan2(sin {s}, cos {s}) = {got:?}, want {x:?}"
        );
    }
}

// --- Category B: independent inverse round-trip, condition-derived. ---

/// `exp(ln(x)) ≈ x`, `x > 0`. Primary large-magnitude backstop.
#[test]
fn exp_ln_roundtrip_decades() {
    let mut cc = Consts::new().expect("consts");
    let probes: &[(&str, u32)] = &[
        ("1.5", 0),
        ("9.25", 0),
        ("1e10", 10),
        ("1e-10", 10),
        ("3.333e40", 40),
        ("1e85", 85),
        ("1e-85", 85),
    ];
    for &(s, exp_abs) in probes {
        let x = parse(s);
        let (l, _) = x.ln(NE);
        let (back, _) = l.exp(NE);
        let n = n_explog(exp_abs);
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "exp(ln({s})) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `pow(2, log2 x) ≈ x` and `pow(10, log10 x) ≈ x`, `x > 0`.
#[test]
fn pow_base_log_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    let probes: &[(&str, u32)] = &[
        ("2", 0),
        ("1e10", 10),
        ("1e-10", 10),
        ("1e50", 50),
        ("4.5e80", 80),
    ];
    let two = parse("2");
    let ten = parse("10");
    for &(s, exp_abs) in probes {
        let x = parse(s);
        let n = n_explog(exp_abs);

        let (l2, _) = x.log2(NE);
        let (b2, _) = two.pow(l2, NE);
        assert!(
            within_n_ulp_band(b2, x, n, &mut cc),
            "pow(2, log2({s})) = {b2:?}, want {x:?} (band {n})"
        );

        let (l10, _) = x.log10(NE);
        let (b10, _) = ten.pow(l10, NE);
        assert!(
            within_n_ulp_band(b10, x, n, &mut cc),
            "pow(10, log10({s})) = {b10:?}, want {x:?} (band {n})"
        );
    }
}

/// `asin(sin x) ≈ x` on `[−π/2, π/2]`; ULP-of-x condition number
/// `|tan x|/|x| + 1`. `x = 0` is the exact fixed point.
#[test]
fn asin_sin_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for &xf in &[
        0.0_f64, 0.3, -0.3, 0.7, -0.7, 1.0, -1.0, 1.4, -1.4, 1.5, 1.56,
    ] {
        let s = format!("{xf:.7}");
        let x = parse(&s);
        let (sn, _) = x.sin(NE);
        let (back, _) = sn.asin(NE);
        let n = if xf == 0.0 {
            4
        } else {
            match n_cond(xf.tan().abs() / xf.abs()) {
                Some(n) => n,
                None => continue,
            }
        };
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "asin(sin {s}) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `acos(cos x) ≈ x` on `(0, π)`; ULP-of-x condition number
/// `|cot x|/|x| + 1`, large near `0` and `π`.
#[test]
fn acos_cos_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for &xf in &[
        0.05_f64,
        0.4,
        0.9,
        core::f64::consts::FRAC_PI_2,
        2.2,
        2.8,
        3.09,
    ] {
        let s = format!("{xf:.7}");
        let x = parse(&s);
        let (cs, _) = x.cos(NE);
        let (back, _) = cs.acos(NE);
        let cot = xf.cos().abs() / xf.sin().abs();
        let Some(n) = n_cond(cot / xf.abs()) else {
            continue;
        };
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "acos(cos {s}) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `atan(tan x) ≈ x` on `(−π/2, π/2)` away from the poles; ULP-of-x
/// condition number `|sin x·cos x|/|x| + 1 ≤ ~2` (well-conditioned).
/// `x = 0` is the exact fixed point.
#[test]
fn atan_tan_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for &xf in &[0.0_f64, 0.3, -0.3, 0.8, -0.8, 1.2, -1.2, 1.5, -1.5] {
        let s = format!("{xf:.7}");
        let x = parse(&s);
        let (tn, _) = x.tan(NE);
        let (back, _) = tn.atan(NE);
        let n = if xf == 0.0 {
            4
        } else {
            n_cond((xf.sin().abs() * xf.cos().abs()) / xf.abs()).expect("atan∘tan well-conditioned")
        };
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "atan(tan {s}) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `acosh(x) ≈ ln(x + sqrt(x²−1))` for `x ≥ 1.5`. Genuine cross-check
/// (kernel uses an independent `log1p` path near 1).
#[test]
fn acosh_matches_ln_form() {
    let mut cc = Consts::new().expect("consts");
    let probes: &[(&str, u32)] = &[
        ("1.5", 0),
        ("2", 0),
        ("10", 0),
        ("1e10", 10),
        ("1e30", 30),
        ("1e45", 45),
    ];
    let one = parse("1");
    for &(s, exp_abs) in probes {
        let x = parse(s);
        let (xx, _) = x.mul(x, NE);
        let (xm1, _) = xx.sub(one, NE);
        let (r, _) = xm1.sqrt(NE);
        let (inner, _) = x.add(r, NE);
        let (rhs, _) = inner.ln(NE);
        let (lhs, _) = x.acosh(NE);
        let n = n_explog(exp_abs) + 16;
        assert!(
            within_n_ulp_band(lhs, rhs, n, &mut cc),
            "acosh({s}) = {lhs:?}, ln-form {rhs:?} (band {n})"
        );
    }
}

// --- Category C: cancellation, weak, small |x| only. ---

/// `cosh²(x) − sinh²(x) ≈ 1` for `|x| ≤ 1`. WEAK (shares the `exp`
/// kernel); a sanity check, not a strong claim.
#[test]
fn cosh_sq_minus_sinh_sq_is_one_small() {
    let mut cc = Consts::new().expect("consts");
    let one = parse("1");
    for s in ["0", "0.1", "-0.1", "0.5", "-0.5", "1", "-1"] {
        let x = parse(s);
        let (ch, _) = x.cosh(NE);
        let (sh, _) = x.sinh(NE);
        let (ch2, _) = ch.mul(ch, NE);
        let (sh2, _) = sh.mul(sh, NE);
        let (d, _) = ch2.sub(sh2, NE);
        assert!(
            within_n_ulp_band(d, one, 64, &mut cc),
            "cosh²−sinh² at {s} = {d:?}, want 1"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// `exp(ln(x)) ≈ x` for `x > 0` across a skip-region-reaching
    /// exponent range; per-sample band from the generated decade.
    #[test]
    fn prop_exp_ln_roundtrip(
        digits in "[1-9][0-9]{0,6}",
        exp in -80_i32..=80_i32,
    ) {
        let s = format!("{digits}e{exp}");
        let x = parse(&s);
        prop_assume!(x.is_finite() && !x.is_zero());
        let mut cc = Consts::new().expect("consts");
        let (l, _) = x.ln(NE);
        let (back, _) = l.exp(NE);
        let exp_abs = exp.unsigned_abs() + 7;
        let n = n_explog(exp_abs);
        prop_assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "exp(ln({s})) = {back:?}, want {x:?} (band {n})"
        );
    }
}
