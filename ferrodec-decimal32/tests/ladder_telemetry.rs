//! Sibling escalation telemetry (fd-4zo.21, ADR-0059 S3): the
//! whole-corpus ZERO pin.
//!
//! The planted-corpus generator's derivation
//! (`tools/gen_planted_hardcases.py`): a rung-1 budget B escalates
//! inputs whose true result lies within B x 10^(P - 50) fractional
//! ULP of a rounding boundary. At P = 7 that threshold is below
//! the resolution the format's own coefficient lattice can express,
//! so NO Decimal32 input can naturally escalate on any operation --
//! rung 1 decides this format's entire input space. This test makes
//! that structural fact a live pin: replay the full sampled corpus,
//! assert zero natural rung-2 entries, and assert the planted
//! corpus is empty at this precision (planting is a Decimal128
//! activity). One `#[test]`: the counters are process-wide.

#![cfg(all(
    feature = "telemetry",
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow",
    feature = "trig-pi",
    not(force_escalate),
    not(force_rung3)
))]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::frozen;
use ferrodec_transcend::telemetry;

const PREC: u32 = 7;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("frozen token parses: {s:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("frozen corpus has an unknown rounding mode {other:?}"),
    }
}

fn drive(v: &frozen::FrozenVec, rm: RoundingMode) {
    let x = parse(&v.input);
    let _ = match v.func.as_str() {
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
        "cbrt" => x.cbrt(rm).0,
        "sqrt" => x.sqrt(rm).0,
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
        "rsqrt" => x.rsqrt(rm).0,
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        "powr" => {
            x.powr(parse(v.input2.as_deref().expect("powr input2")), rm)
                .0
        }
        "hypot" => {
            x.hypot(parse(v.input2.as_deref().expect("hypot input2")), rm)
                .0
        }
        "powi" => {
            let n: i32 = v.input2.as_deref().expect("powi n").parse().expect("i32");
            x.powi(n, rm).0
        }
        "rootn" => {
            let n: i32 = v.input2.as_deref().expect("rootn n").parse().expect("i32");
            x.rootn(n, rm).0
        }
        "compound" => {
            let n: i32 = v
                .input2
                .as_deref()
                .expect("compound n")
                .parse()
                .expect("i32");
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
        other => panic!("no kernel mapping for {other:?}"),
    };
}

#[test]
fn sibling_corpus_never_escalates() {
    assert!(
        frozen::load_planted(PREC).is_empty(),
        "planted corpus grew Decimal32 rows; the generator derivation says \
         that is impossible -- re-derive before accepting"
    );
    telemetry::reset();
    for v in &frozen::load(PREC) {
        drive(v, mode(&v.mode));
    }
    assert_eq!(
        telemetry::rung2_entries(),
        0,
        "a Decimal32 corpus row naturally escalated: the threshold \
         derivation (B x 10^(7 - 50) fractional ULP, below the \
         coefficient lattice's resolution) no longer holds -- a \
         budget or predicate change moved the floor"
    );
}
