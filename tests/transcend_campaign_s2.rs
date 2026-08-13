//! S2 deep-margin campaign corpus replay for `Decimal128`
//! (fd-4zo.19, ADR-0059 S2).
//!
//! The campaign corpus (`tests/vectors/transcend/campaign/`) holds
//! the hardest certified survivors of the 2.7e9-sample local sweep:
//! at most 50 inputs per (function, format) ranked by exact
//! boundary distance, every output re-derived through the Arb proof
//! tier at certification (never taken from the kernel under test).
//! `MANIFEST.json` records the campaign parameters; margins live in
//! the `.prov` twins. This gate replays the Decimal128 rows
//! bit-exact with exact per-bucket pins; the Decimal64 twin lives in
//! `ferrodec-decimal64/tests/transcend_campaign_s2.rs`.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use core::cmp::Ordering;

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 34;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("campaign token parses: {s:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("campaign corpus has an unknown rounding mode {other:?}"),
    }
}

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal128 {
    let x = parse(&v.input);
    match v.func.as_str() {
        "sin" => x.sin(rm).0,
        "cos" => x.cos(rm).0,
        "tan" => x.tan(rm).0,
        "exp" => x.exp(rm).0,
        "ln" => x.ln(rm).0,
        "log10" => x.log10(rm).0,
        "sinh" => x.sinh(rm).0,
        "cosh" => x.cosh(rm).0,
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        other => panic!("campaign corpus has no kernel mapping for {other:?}"),
    }
}

#[test]
fn campaign_vectors_correctly_rounded() {
    let vectors = frozen::load_campaign(PREC);
    frozen::assert_bucket_counts(&vectors, frozen::EXPECTED_BUCKETS_CAMPAIGN_P34);

    let mut failures = Vec::new();
    for v in &vectors {
        let got = kernel(v, mode(&v.mode));
        let want = parse(&v.output);
        if got.partial_cmp(want).0 != Some(Ordering::Equal) {
            failures.push(format!(
                "{} {} {}{}: got {:?}, proven {}",
                v.func,
                v.mode,
                v.input,
                v.input2
                    .as_deref()
                    .map(|s| format!(" {s}"))
                    .unwrap_or_default(),
                got,
                v.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} campaign rows misround:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
