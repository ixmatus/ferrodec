//! Pinned escalation-depth counts (fd-4zo.21, ADR-0059 S3): the
//! drift tripwire in both directions.
//!
//! Replays the planted and sampled Decimal128 corpora with the
//! `telemetry` feature's process-wide counters, asserting EXACT
//! rung-2 entry counts per function file. A budget loosening (or a
//! predicate regression) stops planted rows escalating and their pin
//! moves; a tightening raises the sampled counts and those pins
//! move. Exact per-bucket match, never an aggregate floor.
//!
//! Serial by construction: this binary carries ONE `#[test]` because
//! the counters are process-wide and a second concurrent test would
//! blend counts (`ferrodec_transcend::telemetry` module doc).
//!
//! Pin regeneration is mechanical: on any mismatch the failure
//! message prints the complete measured table as a paste-ready Rust
//! literal. Regenerate deliberately, diff against the planted
//! corpus's `.prov` `escalates=` verdicts, and understand a moved
//! pin before accepting it.
//!
//! Runs only in the default lane: under `--cfg force_escalate` (and
//! the other lane cfgs) the natural predicate is bypassed and every
//! count would read zero.

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

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::frozen;
use ferrodec_transcend::telemetry;

const PREC: u32 = 34;

/// Expected natural rung-2 entries per planted function file: 4
/// escalating inputs (entry and deep grades, two boundary families)
/// x 5 rounding modes. The generator's threshold model and the
/// predicate agreeing on every one of the 36 x 6 planted inputs is
/// the point of the pin.
const PLANTED_RUNG2_PER_FUNC: u64 = 20;

/// Measured natural rung-2 entries per sampled-corpus function file
/// (`tests/vectors/transcend/*.txt` at P = 34). These counts are a
/// property of the frozen corpus bytes and the rung-1 budgets
/// together: the corpus keeps the hardest cases its scans found, so
/// the trig files (whose threshold is ~1.5e-2 fractional ULP)
/// escalate a large share, while tight-budget families escalate only
/// their certified worst rows. Authored from a telemetry run at the
/// fd-4zo.21 landing; regeneration prints the measured table.
const SAMPLED_RUNG2: &[(&str, u64)] = &[
    // Landing measurement 2026-08-10 (branch fd-4zo20-s3). The
    // nonzero rows are exactly the operations whose thresholds can
    // reach their corpus's kept-hardest rows: the trig trio's fat
    // reduction budgets (t ~ 1.5e-2 / 3e-2 fractional ULP) catch
    // their TMD-scan keepers and full-range rows, and compound's
    // adjudicable-range Near verdicts ride the same rung-1 predicate.
    // Everything else's threshold (1e-13 .. 5e-9) sits below its
    // corpus's certified margins. Cross-checked against the .prov
    // margin distributions as a lower bound (per-mode margins record
    // one boundary family; the predicate takes the min of both).
    ("acos", 0),
    ("acosh", 0),
    ("acospi", 0),
    ("asin", 0),
    ("asinh", 0),
    ("asinpi", 0),
    ("atan", 0),
    ("atan2", 0),
    ("atan2pi", 0),
    ("atanh", 0),
    ("atanpi", 0),
    ("cbrt", 0),
    ("compound", 36),
    ("cos", 71),
    ("cosh", 0),
    ("cospi", 0),
    ("exp", 0),
    ("exp10", 0),
    ("exp10m1", 0),
    ("exp2", 0),
    ("exp2m1", 0),
    ("expm1", 0),
    ("hypot", 0),
    ("ln", 0),
    ("log10", 0),
    ("log10p1", 0),
    ("log2", 0),
    ("log2p1", 0),
    ("logp1", 0),
    ("pow", 0),
    ("powi", 0),
    ("rootn", 0),
    ("rsqrt", 0),
    ("sin", 75),
    ("sinh", 0),
    ("sinpi", 0),
    ("tan", 23),
    ("tanh", 0),
    ("tanpi", 0),
];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
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

/// Kernel dispatch, mirroring `tests/transcend_vectors.rs` (results
/// discarded: correctness is that file's and `transcend_planted.rs`'s
/// job; only the escalation side effects matter here).
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

/// Replay `vectors` grouped by function file, serially, returning the
/// measured rung-2 entry count per function (sorted by name; the
/// loader pre-sorts, so one linear pass groups correctly).
fn measure(vectors: &[frozen::FrozenVec]) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut current: Option<String> = None;
    for v in vectors {
        if current.as_deref() != Some(v.func.as_str()) {
            if let Some(f) = current.take() {
                out.push((f, telemetry::rung2_entries()));
            }
            telemetry::reset();
            current = Some(v.func.clone());
        }
        drive(v, mode(&v.mode));
    }
    if let Some(f) = current {
        out.push((f, telemetry::rung2_entries()));
    }
    out
}

fn paste_table(measured: &[(String, u64)]) -> String {
    measured
        .iter()
        .map(|(f, n)| format!("    ({f:?}, {n}),"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn pinned_rung2_entry_counts() {
    // Planted corpus: every non-control input escalates, exactly.
    let planted = frozen::load_planted(PREC);
    let measured = measure(&planted);
    let bad: Vec<_> = measured
        .iter()
        .filter(|(_, n)| *n != PLANTED_RUNG2_PER_FUNC)
        .collect();
    assert!(
        bad.is_empty(),
        "planted rung-2 entries off the {PLANTED_RUNG2_PER_FUNC}-per-file \
         contract:\n{}\nfull measured table:\n{}",
        bad.iter()
            .map(|(f, n)| format!("  {f}: {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
        paste_table(&measured)
    );

    // Sampled corpus: exact pins per function file.
    let sampled = frozen::load(PREC);
    let measured = measure(&sampled);
    let expected: Vec<(String, u64)> = SAMPLED_RUNG2
        .iter()
        .map(|(f, n)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(
        measured,
        expected,
        "sampled-corpus rung-2 entry pins moved; measured table \
         (review before pasting):\n{}",
        paste_table(&measured)
    );
}
