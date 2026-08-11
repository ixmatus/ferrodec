//! Planted rung-2-forcing corpus replay for `Decimal128` (fd-4zo.20,
//! ADR-0059 S3).
//!
//! The planted corpus (`tests/vectors/transcend/planted/`) is
//! committed data like the sampled corpus, but its inputs are
//! CONSTRUCTED: each sits at a chosen distance from a format rounding
//! boundary (control / entry / deep grades around the rung-1
//! escalation threshold; `tools/gen_planted_hardcases.py` carries the
//! derivation), so the deliveries here exercise the ladder's natural
//! escalation path on Arb-certified answers. This test is the
//! correctness half: every planted row replays to the proven
//! correctly rounded value exactly. The escalation-count half (the
//! pinned rung-2 entry counts, the actual tripwire) lives in
//! `tests/ladder_telemetry.rs` behind the `telemetry` feature.
//!
//! The kernel dispatch mirrors `tests/transcend_vectors.rs` (the
//! sampled-corpus twin); keep the two in sync when the surface grows.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow",
    feature = "trig-pi"
))]

use core::cmp::Ordering;

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 34;

/// Exact per-file row counts: 36 operations x 6 planted inputs x 5
/// rounding modes. An aggregate floor would admit a file silently
/// losing rows while another gains; the per-bucket exact match is the
/// house regression-guard rule.
const PLANTED_ROWS_PER_FUNC: usize = 30;
const PLANTED_FUNCS: [&str; 36] = [
    "acos", "acosh", "acospi", "asin", "asinh", "asinpi", "atan", "atan2", "atan2pi", "atanh",
    "atanpi", "compound", "cos", "cosh", "cospi", "exp", "exp10", "exp10m1", "exp2", "exp2m1",
    "expm1", "ln", "log10", "log10p1", "log2", "log2p1", "logp1", "pow", "powi", "rootn", "sin",
    "sinh", "sinpi", "tan", "tanh", "tanpi",
];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("planted token parses: {s:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("planted corpus has an unknown rounding mode {other:?}"),
    }
}

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal128 {
    let x = parse(&v.input);
    match v.func.as_str() {
        "exp" => x.exp(rm).0,
        "ln" => x.ln(rm).0,
        "log2" => x.log2(rm).0,
        "log2p1" => x.log2_1p(rm).0,
        "log10" => x.log10(rm).0,
        "logp1" => x.ln_1p(rm).0,
        "log10p1" => x.log10_1p(rm).0,
        "exp2" => x.exp2(rm).0,
        "expm1" => x.exp_m1(rm).0,
        "exp2m1" => x.exp2_m1(rm).0,
        "exp10" => x.exp10(rm).0,
        "exp10m1" => x.exp10_m1(rm).0,
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
        "sinpi" => x.sin_pi(rm).0,
        "cospi" => x.cos_pi(rm).0,
        "tanpi" => x.tan_pi(rm).0,
        "asinpi" => x.asin_pi(rm).0,
        "acospi" => x.acos_pi(rm).0,
        "atanpi" => x.atan_pi(rm).0,
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        "powi" => {
            let n: i32 = v
                .input2
                .as_deref()
                .expect("powi n")
                .parse()
                .expect("powi i32");
            x.powi(n, rm).0
        }
        "rootn" => {
            let n: i32 = v
                .input2
                .as_deref()
                .expect("rootn n")
                .parse()
                .expect("rootn i32");
            x.rootn(n, rm).0
        }
        "compound" => {
            let n: i32 = v
                .input2
                .as_deref()
                .expect("compound n")
                .parse()
                .expect("compound i32");
            x.compound(n, rm).0
        }
        "atan2" => {
            parse(v.input2.as_deref().expect("atan2 input2"))
                .atan2(x, rm)
                .0
        }
        "atan2pi" => {
            parse(v.input2.as_deref().expect("atan2pi input2"))
                .atan2_pi(x, rm)
                .0
        }
        other => panic!("planted corpus has no kernel mapping for {other:?}"),
    }
}

#[test]
fn planted_vectors_correctly_rounded() {
    let vectors = frozen::load_planted(PREC);
    for func in PLANTED_FUNCS {
        let n = vectors.iter().filter(|v| v.func == func).count();
        assert_eq!(
            n, PLANTED_ROWS_PER_FUNC,
            "planted corpus row count drifted for {func}"
        );
    }
    assert_eq!(
        vectors.len(),
        PLANTED_FUNCS.len() * PLANTED_ROWS_PER_FUNC,
        "planted corpus carries an unpinned function file"
    );

    let mut failures = Vec::new();
    for v in &vectors {
        let rm = mode(&v.mode);
        let got = kernel(v, rm);
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
        "{} planted rows misround:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
