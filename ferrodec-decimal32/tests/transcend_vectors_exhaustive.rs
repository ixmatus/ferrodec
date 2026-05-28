//! ADR-0033 Plan C4 exhaustive worst-case kernel verification gate
//! (fd-ykr.1, Slice C work-in-progress).
//!
//! The campaign in `tools/d32_exhaustive_sweep.py` walked every
//! canonical Decimal32 input through a two-tier certified Arb filter
//! and recorded the worst-case half-ULP margin per function: the
//! tightest case the kernel must round correctly. The worst-case
//! input + proven correctly-rounded output is committed under
//! `tests/vectors/transcend/exhaustive/<fn>.txt`; this test asserts
//! the kernel reproduces the proven value at that input for every
//! function in the 18-function unary §9.2 surface.
//!
//! This is the strongest empirical correctness gate the project
//! currently has: if the kernel rounds the worst case correctly, by
//! the ADR-0033 proof-program argument (kernel working width
//! comfortably exceeds the tightest empirical margin plus the
//! analytic Payne-Hanek error budget) it rounds every other input
//! correctly too. A failure here is direct evidence of a kernel
//! defect on the function's hardest known input.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use core::cmp::Ordering;

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 7;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("exhaustive vector token parses: {s:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("exhaustive vector has unknown rounding mode {other:?}"),
    }
}

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal32 {
    let x = parse(&v.input);
    match v.func.as_str() {
        "exp" => x.exp(rm).0,
        "ln" => x.ln(rm).0,
        "log2" => x.log2(rm).0,
        "log10" => x.log10(rm).0,
        "exp2" => x.exp2(rm).0,
        "cbrt" => x.cbrt(rm).0,
        "sin" => x.sin(rm).0,
        "cos" => x.cos(rm).0,
        "tan" => x.tan(rm).0,
        "asin" => x.asin(rm).0,
        "acos" => x.acos(rm).0,
        "atan" => x.atan(rm).0,
        "sinh" => x.sinh(rm).0,
        "cosh" => x.cosh(rm).0,
        "tanh" => x.tanh(rm).0,
        "asinh" => x.asinh(rm).0,
        "acosh" => x.acosh(rm).0,
        "atanh" => x.atanh(rm).0,
        other => panic!("exhaustive corpus has no kernel mapping for {other:?}"),
    }
}

fn step_distance(got: Decimal32, cr: Decimal32) -> Option<u8> {
    if got.partial_cmp(cr).0 == Some(Ordering::Equal) {
        return Some(0);
    }
    let up = cr.next_up().0;
    let dn = cr.next_down().0;
    if got.partial_cmp(up).0 == Some(Ordering::Equal)
        || got.partial_cmp(dn).0 == Some(Ordering::Equal)
    {
        return Some(1);
    }
    None
}

#[test]
fn exhaustive_worst_case_correctly_rounded() {
    let vectors = frozen::load_exhaustive(PREC);
    assert!(
        vectors.len() >= 18,
        "expected the 18 unary §9.2 exhaustive worst-case rows, got {}",
        vectors.len()
    );

    let mut exact = 0usize;
    for v in &vectors {
        let rm = mode(&v.mode);
        let cr = parse(&v.output);
        let got = kernel(v, rm);
        match step_distance(got, cr) {
            Some(0) => exact += 1,
            Some(d) => panic!(
                "exhaustive worst-case contract violated ({d} step) [{}]: \
                 {}({}) -> ferrodec {} | proven correctly rounded {} \
                 (ADR-0033 Plan C4)",
                v.mode, v.func, v.input, got, cr
            ),
            None => panic!(
                "exhaustive worst-case contract violated (multi step) [{}]: \
                 {}({}) -> ferrodec {} | proven correctly rounded {} \
                 (ADR-0033 Plan C4)",
                v.mode, v.func, v.input, got, cr
            ),
        }
    }
    eprintln!(
        "ADR-0033 Plan C4 exhaustive worst-case gate (Decimal32, p{PREC}): \
         {exact}/{} exactly correctly rounded. Each row is the tightest \
         half-ULP margin input across the function's full canonical \
         Decimal32 input set; passing here is the strongest empirical \
         correctness evidence the campaign produced.",
        vectors.len()
    );
}
