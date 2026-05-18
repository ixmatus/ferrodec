//! Metamorphic identity cross-checks for the `Decimal128`
//! transcendentals (ADR-0025).
//!
//! These are algebraic relations that hold for the *exact* functions at
//! any magnitude, so they need no oracle. That is the point: the
//! astro-float oracle has a sound magnitude domain and the
//! `property_*` suites skip out of it (fd-3cd / fd-dfs); in those
//! skipped regions these identities are the only correctness backstop.
//!
//! They are **not** the IEEE faithful-rounding contract. A composed
//! identity accumulates more than one ULP by construction, and an
//! ill-conditioned composition accumulates a *condition-number* multiple
//! of a ULP (`exp(ln 1e300)` round-trips with ≈700 ULP of error, not 4).
//! Each identity's band is therefore `n_ulps` derived from the analytic
//! condition number evaluated at the test point (ADR-0025), enforced by
//! `common::within_n_ulp_band`.
//!
//! The condition factor is sized in `f64` from the *analytically known*
//! argument magnitude (the decade a probe is constructed at, or a
//! generated exponent). `f64` only sizes the tolerance; it never
//! computes or checks the decimal result, so its own imprecision cannot
//! mask a decimal defect. It can only mis-size the band by far less than
//! the `C = 4` safety factor.
//!
//! The identity set is the audited, non-degenerate one (ADR-0025):
//! identities whose two sides both route through the same shared-kernel
//! helper (`log_b·ln(b)≈ln`, `tanh≈sinh/cosh`, `exp2==pow(2,x)`,
//! `asinh`/`atanh` vs their own ln-forms) are tautological against this
//! kernel and are deliberately absent.

#![cfg(feature = "transcendentals")]

use ferrodec::RoundingMode;
use proptest::prelude::*;

mod common;
use common::{parse, within_n_ulp_band};

use ferrodec_test_support::transcend_oracle::Consts;

const NE: RoundingMode = RoundingMode::NearestEven;

/// Safety constant on every derived band (ADR-0025): absorbs
/// higher-order terms in the condition expansion and the identity's own
/// residual rounding.
const C: f64 = 4.0;

/// `ln(10)`, the bits-free decade-to-natural-log scale.
const LN10: f64 = core::f64::consts::LN_10;

/// Band for an `exp`/`ln`-amplified round-trip whose argument has
/// decimal exponent magnitude `exp_abs`. The condition number is
/// `|ln x| + 1`; `|ln x| ≤ exp_abs·ln10 + ln(coefficient) + 1`, and the
/// coefficient of any `Decimal128` is `< 10^34`, so `ln(coef) < 80`.
fn n_explog(exp_abs: u32) -> u32 {
    (C * (f64::from(exp_abs) * LN10 + 80.0 + 1.0)).ceil() as u32
}

/// Band `C·(cond + 1)` from a condition number already expressed in
/// **ULP-of-x units**, capped: a factor so large the band would be
/// vacuous means the point is past the identity's meaningful domain and
/// the caller skips it instead.
///
/// The ULP-of-x scaling matters. An intermediate at magnitude `O(1)`
/// (e.g. `cos x`) carries an absolute error `≈ |cos x|·u`, but `x` may
/// be small, so one ULP *of x* is `|x|·u ≪ |cos x|·u`. The condition
/// number in ULP-of-x therefore carries an extra `1/|x|`-type factor
/// that a naive `1 + |cot x|` misses (this is exactly the
/// `acos(cos 0.05)` underestimate; see ADR-0025).
fn n_cond(cond_in_ulp_of_x: f64) -> Option<u32> {
    let n = (C * (cond_in_ulp_of_x + 1.0)).ceil();
    if !n.is_finite() || n > 5.0e7 {
        None
    } else {
        Some(n as u32)
    }
}

// ---------------------------------------------------------------------
// Category A — independent cross-computation, well-conditioned, tight
// band. Two mutually independent kernels computing the same magnitude;
// full teeth at any magnitude, including the oracle skip regions.
// ---------------------------------------------------------------------

/// `pow(x,2) == x*x`: `pow` is `exp(2·ln x)`; `x*x` is the BID
/// multiplier. Independent kernels, identical exact value.
#[test]
fn pow_sq_equals_mul() {
    let mut cc = Consts::new().expect("consts");
    for s in [
        "2",
        "3.5",
        "1e-50",
        "1e50",
        "9.999999999999999999999999999999999e10",
        "1.234567890123456789e-200",
        "7e2000",
    ] {
        let x = parse(s);
        let (lhs, _) = x.pow(parse("2"), NE);
        let (rhs, _) = x.mul(x, NE);
        assert!(
            within_n_ulp_band(lhs, rhs, 4, &mut cc),
            "pow({s},2) vs {s}*{s}: {lhs:?} vs {rhs:?}"
        );
    }
}

/// `pow(x,0.5) == sqrt(x)` for `x > 0`: `pow` is `exp(0.5·ln x)`;
/// `sqrt` is the independent Newton kernel.
#[test]
fn pow_half_equals_sqrt() {
    let mut cc = Consts::new().expect("consts");
    for s in [
        "2",
        "100",
        "1e-100",
        "1e100",
        "3.0000000000000001",
        "5e3000",
    ] {
        let x = parse(s);
        let (lhs, _) = x.pow(parse("0.5"), NE);
        let (rhs, _) = x.sqrt(NE);
        assert!(
            within_n_ulp_band(lhs, rhs, 4, &mut cc),
            "pow({s},0.5) vs sqrt({s}): {lhs:?} vs {rhs:?}"
        );
    }
}

/// `ln(exp(x)) ≈ x` where `exp(x)` is finite. The `ln∘exp` direction
/// contracts error (unlike the amplifying `exp∘ln`), so a few ULP
/// suffices. `exp` then `ln` are independent kernels.
#[test]
fn ln_exp_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    // |x| < 14149 keeps exp(x) finite in Decimal128.
    for s in [
        "1", "-1", "0.5", "100", "-100", "5000", "-5000", "13000", "-13000",
    ] {
        let x = parse(s);
        let xf: f64 = s.parse().expect("probe is f64");
        let (e, _) = x.exp(NE);
        let (back, _) = e.ln(NE);
        // |ln(exp x) − x| ≈ u·(1 + |x|); in ULP-of-x that is
        // 1/|x| + 1, so error-contracting (a few ULP) for |x| ≳ 1.
        let n = n_cond(1.0 / xf.abs() + 1.0).expect("ln∘exp well-conditioned");
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "ln(exp({s})) = {back:?}, want {x:?} (band {n})"
        );
    }
}

/// `atan2(sin x, cos x) ≈ x` on `(−π, π]`, including the quadrant
/// branches. The sincos kernel vs the independent atan2 kernel;
/// recovering an angle from a unit vector is well-conditioned.
#[test]
fn atan2_sincos_recovers_angle() {
    let mut cc = Consts::new().expect("consts");
    for s in [
        "0",
        "0.5",
        "-0.5",
        "1.5707963267948966",
        "-1.5707963267948966",
        "3.0",
        "-3.0",
        "2.356194490192345",
        "-2.356194490192345",
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

// ---------------------------------------------------------------------
// Category B — independent inverse round-trip, condition-amplified,
// derived band. Non-degenerate (the kernels are independent); the band
// is the analytic condition number times C.
// ---------------------------------------------------------------------

/// `exp(ln(x)) ≈ x`, `x > 0`. The primary large-magnitude backstop:
/// `ln` is independent of `exp`, and the probes deliberately reach
/// `1e±300` / `7e2000`, far past where the astro-float oracle is
/// skipped (`|x| ≳ 1e70`). Band `≈ C·(|ln x| + 1)`.
#[test]
fn exp_ln_roundtrip_decades() {
    let mut cc = Consts::new().expect("consts");
    // (string, decimal-exponent magnitude)
    let probes: &[(&str, u32)] = &[
        ("1.5", 0),
        ("9.25", 0),
        ("1e10", 10),
        ("1e-10", 10),
        ("3.333e70", 70), // first decade past the oracle-skip wall
        ("1e300", 300),
        ("1e-300", 300),
        ("7.123456789e2000", 2000),
        ("1e6000", 6000),
    ];
    for &(s, exp_abs) in probes {
        let x = parse(s);
        let (l, _) = x.ln(NE);
        let (back, _) = l.exp(NE);
        let n = n_explog(exp_abs);
        assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "exp(ln({s})) = {back:?}, want {x:?} (band {n} ulp)"
        );
    }
}

/// `pow(2, log2 x) ≈ x` and `pow(10, log10 x) ≈ x`, `x > 0`. Replaces
/// the dropped `log_b·ln(b)≈ln` tautology: `log_b` routes through `ln`,
/// `pow` through the *independent* `exp`, so the round-trip is genuine.
/// Same `exp∘ln` conditioning as above.
#[test]
fn pow_base_log_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    let probes: &[(&str, u32)] = &[
        ("2", 0),
        ("1e15", 15),
        ("1e-15", 15),
        ("1e200", 200),
        ("4.5e1500", 1500),
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

/// `asin(sin x) ≈ x` on `[−π/2, π/2]`. The sincos kernel vs the
/// independent inverse-trig kernel. Absolute error
/// `≈ u·(|tan x| + |x|)`; in ULP-of-x the condition number is
/// `|tan x|/|x| + 1` (well-conditioned near 0 where `tan x ≈ x`,
/// growing toward `±π/2`). `x = 0` is the exact fixed point.
#[test]
fn asin_sin_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for &xf in &[
        0.0_f64, 0.3, -0.3, 0.7, -0.7, 1.0, -1.0, 1.4, -1.4, 1.5, 1.56,
    ] {
        let s = format!("{xf:.17}");
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

/// `acos(cos x) ≈ x` on `(0, π)`. Absolute error
/// `≈ u·(|cot x| + |x|)`; in ULP-of-x the condition number is
/// `|cot x|/|x| + 1`, large near `0` and `π` (e.g. `≈ 400` at
/// `x = 0.05`, the case that exposed the earlier flat-budget bug).
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
        let s = format!("{xf:.17}");
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

/// `atan(tan x) ≈ x` on `(−π/2, π/2)` away from the poles. Absolute
/// error `≈ u·(|sin x · cos x| + |x|)`; in ULP-of-x the condition
/// number is `|sin x · cos x|/|x| + 1 ≤ ~2` (the `atan` derivative
/// `cos²` cancels the `tan` blow-up), so it is well-conditioned
/// everywhere. `x = 0` is the exact fixed point.
#[test]
fn atan_tan_roundtrip() {
    let mut cc = Consts::new().expect("consts");
    for &xf in &[0.0_f64, 0.3, -0.3, 0.8, -0.8, 1.2, -1.2, 1.5, -1.5] {
        let s = format!("{xf:.17}");
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

/// `acosh(x) ≈ ln(x + sqrt(x²−1))` for `x ≥ 1.5`. Genuine cross-check:
/// `acosh_kernel` near `x = 1` uses an independent `log1p` path, so the
/// naive reconstruction is a different algorithm. Probes stay at
/// `x ≥ 1.5` so the `sqrt(x²−1)` cancellation factor is bounded; the
/// large-`x` band uses the `exp∘ln`-style decade growth of `ln(2x)`.
#[test]
fn acosh_matches_ln_form() {
    let mut cc = Consts::new().expect("consts");
    let probes: &[(&str, u32)] = &[
        ("1.5", 0),
        ("2", 0),
        ("10", 0),
        ("1e10", 10),
        ("1e100", 100),
        ("1e1000", 1000),
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

// ---------------------------------------------------------------------
// Category C — cancellation, weak, small |x| only. Documented as weak:
// shares the exp kernel, so this is a consistency check on e^x·e^-x,
// not an independent oracle.
// ---------------------------------------------------------------------

/// `cosh²(x) − sinh²(x) ≈ 1` for `|x| ≤ 1`. WEAK: `cosh` and `sinh`
/// both derive from `exp`, so a common `exp` defect can survive here.
/// Bounded to small `|x|` (the cancellation is catastrophic otherwise)
/// and kept only as a sanity check, not a strong claim.
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
        // cosh,sinh ≈ e for x≈1; squaring then subtracting loses a few
        // digits even at |x|=1. 64 ULP is a generous weak sanity bound.
        assert!(
            within_n_ulp_band(d, one, 64, &mut cc),
            "cosh²−sinh² at {s} = {d:?}, want 1"
        );
    }
}

// ---------------------------------------------------------------------
// Magnitude-biased proptest for the headline exp∘ln backstop. The
// generator constructs coef·10^exp with a controlled exponent so the
// sweep actually reaches the oracle-skip decades, rather than the
// mid-range bias a raw bit fuzz would give.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// `exp(ln(x)) ≈ x` for `x > 0` across a wide, deliberately
    /// skip-region-reaching exponent range. Band derived per-sample
    /// from the generated decade.
    #[test]
    fn prop_exp_ln_roundtrip(
        digits in "[1-9][0-9]{0,16}",
        exp in -280_i32..=280_i32,
    ) {
        let s = format!("{digits}e{exp}");
        let x = parse(&s);
        prop_assume!(x.is_finite() && !x.is_zero());
        let mut cc = Consts::new().expect("consts");
        let (l, _) = x.ln(NE);
        let (back, _) = l.exp(NE);
        // |ln x| ≤ (|exp| + #digits)·ln10 + 1; #digits ≤ 17.
        let exp_abs = (exp.unsigned_abs()) + 17;
        let n = n_explog(exp_abs);
        prop_assert!(
            within_n_ulp_band(back, x, n, &mut cc),
            "exp(ln({s})) = {back:?}, want {x:?} (band {n})"
        );
    }
}
