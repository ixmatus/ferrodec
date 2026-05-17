//! astro-float cross-check for the still-`f64`-routed `Decimal64`
//! transcendental cluster (the trig / inverse-trig / hyperbolic /
//! inverse-hyperbolic families, `pow`, `cbrt`).
//!
//! `exp` and `ln` no longer live here. The fd-r0l pilot rewired them
//! onto the shared faithful `ferrodec-transcend` Extended-precision
//! kernel, so they now meet the exact faithful-rounding contract
//! (≤ 1 ULP at 16 digits, every rounding direction) proven in
//! `tests/property_exp.rs` and `tests/property_ln.rs`. This file
//! retains only the operations that still route through `f64` via
//! `libm` (`ops/trig.rs`, `ops/hyper.rs`, `ops/pow.rs`), whose
//! documented accuracy envelope is the f64 round-trip limit: ~10⁻¹⁵
//! relative, with accuracy specified only for `|x| < 2^53` (see the
//! `ops/trig.rs` module doc). These tests therefore hold the
//! implementation to that *documented* envelope, a `1e-13` relative
//! bound with a `1.0` absolute cushion — the same contract the
//! in-crate `approx_equal` unit-test helpers already encode — not the
//! 1-ULP-at-16-digits a pure-decimal kernel would meet. The oracle is
//! `astro-float` (pure Rust arbitrary precision, no MPFR / C FFI;
//! `feedback_oracle_choice`).
//!
//! Range, not precision, is out of scope here: inputs are kept where
//! the true result stays inside `f64`'s finite range so the
//! comparison measures rounding, not OVERFLOW / UNDERFLOW (those are
//! covered by the per-op unit tests and Kani special-case shims).

#![cfg(feature = "transcendentals")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};
use proptest::prelude::*;

/// Documented decimal64 f64-pipeline envelope: relative `1e-13`
/// with a `1.0` absolute cushion for results near zero.
const TOL: f64 = 1e-13;

/// astro-float working precision in bits (~60 decimal digits, far
/// beyond the f64 envelope we are checking against).
const ORACLE_BITS: usize = 220;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .expect("valid decimal literal")
        .0
}

/// Render an astro-float `BigFloat` as an `f64` (via a 40-digit
/// decimal string, well inside `f64` precision).
fn bigfloat_to_f64(v: &BigFloat, cc: &mut Consts) -> f64 {
    let (sign, mantissa, exp) = v
        .convert_to_radix(Radix::Dec, AfRm::ToEven, cc)
        .expect("convert to decimal");
    if mantissa.is_empty() || mantissa.iter().all(|&d| d == 0) {
        return 0.0;
    }
    let take = 40.min(mantissa.len());
    let digit_str: String = mantissa[..take]
        .iter()
        .map(|&d| char::from(b'0' + d))
        .collect();
    let scale = exp - take as i32;
    let sign_str = if matches!(sign, Sign::Neg) { "-" } else { "" };
    format!("{sign_str}{digit_str}e{scale}")
        .parse::<f64>()
        .expect("decimal string parses as f64")
}

/// Apply a unary astro-float function to `x_str` at oracle
/// precision and return the result as `f64`.
fn oracle_unary<F>(x_str: &str, f: F) -> f64
where
    F: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, ORACLE_BITS, AfRm::None, &mut cc);
    let r = f(&x, ORACLE_BITS, AfRm::None, &mut cc);
    bigfloat_to_f64(&r, &mut cc)
}

/// astro-float `cbrt` takes only `(p, rm)` (no `Consts`), so it
/// needs its own oracle wrapper.
fn oracle_cbrt(x_str: &str) -> f64 {
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, ORACLE_BITS, AfRm::None, &mut cc);
    let r = x.cbrt(ORACLE_BITS, AfRm::None);
    bigfloat_to_f64(&r, &mut cc)
}

fn oracle_pow(base_str: &str, exp_str: &str) -> f64 {
    let mut cc = Consts::new().expect("init consts");
    let b = BigFloat::parse(base_str, Radix::Dec, ORACLE_BITS, AfRm::None, &mut cc);
    let e = BigFloat::parse(exp_str, Radix::Dec, ORACLE_BITS, AfRm::None, &mut cc);
    let r = b.pow(&e, ORACLE_BITS, AfRm::None, &mut cc);
    bigfloat_to_f64(&r, &mut cc)
}

/// `true` when `got` is within the documented envelope of `want`.
fn close(got: Decimal64, want: f64) -> bool {
    let gf = got.to_f64(RoundingMode::NearestEven).0;
    (gf - want).abs() <= TOL * (1.0 + want.abs())
}

fn check_unary<G, O>(name: &str, x_str: &str, g: G, o: O)
where
    G: FnOnce(Decimal64, RoundingMode) -> (Decimal64, Status),
    O: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let x = parse(x_str);
    let (got, _) = g(x, RoundingMode::NearestEven);
    let want = oracle_unary(x_str, o);
    assert!(
        close(got, want),
        "{name}({x_str}): got {:?}, want ≈ {want}",
        got.to_f64(RoundingMode::NearestEven).0
    );
}

// exp / ln moved to the faithful suites `tests/property_exp.rs` and
// `tests/property_ln.rs` (fd-r0l P1). What follows is the still-f64
// envelope for the remaining operations.

// trig --------------------------------------------------------------------

#[test]
fn spot_sin_cos_tan() {
    for s in [
        "0",
        "0.5",
        "1",
        "-1",
        "0.7853981633974483",
        "2.5",
        "-2.5",
        "0.0001",
    ] {
        check_unary("sin", s, Decimal64::sin, BigFloat::sin);
        check_unary("cos", s, Decimal64::cos, BigFloat::cos);
        // tan blows up near π/2; the sampled points stay clear.
        check_unary("tan", s, Decimal64::tan, BigFloat::tan);
    }
}

#[test]
fn spot_inverse_trig() {
    for s in ["0", "0.5", "-0.5", "1", "-1", "0.9", "-0.25"] {
        check_unary("asin", s, Decimal64::asin, BigFloat::asin);
        check_unary("acos", s, Decimal64::acos, BigFloat::acos);
    }
    for s in ["0", "1", "-1", "10", "-10", "0.5", "123.4"] {
        check_unary("atan", s, Decimal64::atan, BigFloat::atan);
    }
}

// hyperbolic --------------------------------------------------------------

#[test]
fn spot_hyperbolic() {
    for s in ["0", "1", "-1", "2", "-2", "0.5", "-0.75", "5"] {
        check_unary("sinh", s, Decimal64::sinh, BigFloat::sinh);
        check_unary("cosh", s, Decimal64::cosh, BigFloat::cosh);
        check_unary("tanh", s, Decimal64::tanh, BigFloat::tanh);
        check_unary("asinh", s, Decimal64::asinh, BigFloat::asinh);
    }
    for s in ["1", "1.5", "2", "10", "100"] {
        check_unary("acosh", s, Decimal64::acosh, BigFloat::acosh);
    }
    for s in ["0", "0.5", "-0.5", "0.9", "-0.99"] {
        check_unary("atanh", s, Decimal64::atanh, BigFloat::atanh);
    }
}

// pow / cbrt --------------------------------------------------------------

#[test]
fn spot_pow() {
    for (b, e) in [
        ("2", "3"),
        ("2", "0.5"),
        ("10", "2"),
        ("3", "-2"),
        ("2.5", "1.5"),
        ("7", "0"),
        ("1.1", "10"),
    ] {
        let got = parse(b).pow(parse(e), RoundingMode::NearestEven).0;
        let want = oracle_pow(b, e);
        assert!(
            close(got, want),
            "pow({b}, {e}): got {:?}, want ≈ {want}",
            got.to_f64(RoundingMode::NearestEven).0
        );
    }
}

#[test]
fn spot_cbrt() {
    for s in ["8", "-27", "2", "0.001", "1000", "0", "-1", "1234.5678"] {
        let (got, _) = parse(s).cbrt(RoundingMode::NearestEven);
        let want = oracle_cbrt(s);
        assert!(
            close(got, want),
            "cbrt({s}): got {:?}, want ≈ {want}",
            got.to_f64(RoundingMode::NearestEven).0
        );
    }
}

// Property sweeps ---------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    // `exp` / `ln` sweeps moved to the faithful suites
    // `tests/property_exp.rs` / `tests/property_ln.rs` (fd-r0l P1).

    /// `sin` / `cos` over moderate magnitude (well inside the
    /// `|x| < 2^53` accuracy envelope).
    #[test]
    fn sin_cos_sweep(bits in any::<u64>(), sign in any::<bool>()) {
        let frac = bits as f64 / u64::MAX as f64;
        let mag = frac * 1000.0;
        let s = format!("{}{:.8}", if sign { "-" } else { "" }, mag);
        let x = parse(&s);
        let (gs, _) = x.sin(RoundingMode::NearestEven);
        let (gc, _) = x.cos(RoundingMode::NearestEven);
        let ws = oracle_unary(&s, BigFloat::sin);
        let wc = oracle_unary(&s, BigFloat::cos);
        prop_assert!(close(gs, ws), "sin({s}): got {:?}, want {ws}",
            gs.to_f64(RoundingMode::NearestEven).0);
        prop_assert!(close(gc, wc), "cos({s}): got {:?}, want {wc}",
            gc.to_f64(RoundingMode::NearestEven).0);
    }

    /// `tanh` over the full real line: bounded in `(-1, 1)`, no
    /// range concerns.
    #[test]
    fn tanh_sweep(bits in any::<u64>(), sign in any::<bool>()) {
        let frac = bits as f64 / u64::MAX as f64;
        let mag = frac * 30.0;
        let s = format!("{}{:.9}", if sign { "-" } else { "" }, mag);
        let x = parse(&s);
        let (got, _) = x.tanh(RoundingMode::NearestEven);
        let want = oracle_unary(&s, BigFloat::tanh);
        prop_assert!(close(got, want), "tanh({s}): got {:?}, want {want}",
            got.to_f64(RoundingMode::NearestEven).0);
    }
}
