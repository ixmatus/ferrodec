//! Default-on lockstep meta-test for the proof-tier decimal rounding
//! keystone (fd-tgg; fd-cb6, ADR-0026).
//!
//! `round_sig` (Rust) and `round_half_even_sig` (Python, in
//! `tools/gen_transcend_vectors.py`) independently round an Arb/MPFR
//! high-precision result to `prec` significant digits, ties to even,
//! to freeze a correctly-rounded corpus value. Their agreement was
//! only ever checked indirectly (Arb-decisive-AND-MPFR-agrees). This
//! test exercises the Rust side directly against the shared committed
//! case table `tests/vectors/round_half_even/cases.txt`; the Python
//! `--selftest` runs the same table, so the two stay in lockstep. No
//! `rug`, no C-FFI, no oracle in the path: it parses checked-in text,
//! so it is default-on and runs in standard CI.

use std::fs;
use std::path::PathBuf;

use ferrodec_test_support::round_dec::{
    parse_dec, round_directed_sig, round_sig, same_value, Round,
};

struct Case {
    prec: usize,
    mode: Round,
    input: String,
    expected: String,
    name: String,
}

fn parse_mode(s: &str) -> Round {
    match s {
        "NearestEven" => Round::NearestEven,
        "NearestAway" => Round::NearestAway,
        "TowardZero" => Round::TowardZero,
        "TowardPositive" => Round::TowardPositive,
        "TowardNegative" => Round::TowardNegative,
        other => panic!("unknown rounding mode in case table: {other:?}"),
    }
}

/// `<ws>/tests/vectors/round_half_even/cases.txt`, via the same `../`
/// resolution from this crate's manifest dir that `frozen` uses for
/// the transcend corpus.
fn cases_path() -> PathBuf {
    PathBuf::from(format!(
        "{}/../tests/vectors/round_half_even/cases.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn load_cases() -> Vec<Case> {
    let path = cases_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("shared case table {}: {e}", path.display()));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(prec), Some(mode), Some(input), Some(expected), Some(name), None) = (
            it.next(),
            it.next(),
            it.next(),
            it.next(),
            it.next(),
            it.next(),
        ) else {
            panic!("malformed case line in {}: {line:?}", path.display());
        };
        out.push(Case {
            prec: prec.parse().expect("prec is a usize"),
            mode: parse_mode(mode),
            input: input.to_string(),
            expected: expected.to_string(),
            name: name.to_string(),
        });
    }
    out
}

#[test]
fn round_dec_matches_shared_case_table() {
    let cases = load_cases();
    assert!(
        cases.len() >= 28,
        "expected the full shared case table (NearestEven + directed), loaded {}",
        cases.len()
    );
    for c in &cases {
        let got = round_directed_sig(&parse_dec(&c.input), c.prec, c.mode);
        let want = parse_dec(&c.expected);
        assert!(
            same_value(&got, &want),
            "round_directed_sig lockstep failure [{}]: \
             round_directed_sig(parse_dec({:?}), {}, {:?}) = {:?}, \
             expected value of {:?} = {:?}",
            c.name,
            c.input,
            c.prec,
            c.mode,
            got,
            c.expected,
            want
        );
        // round_sig never returns more than `prec` significant digits
        // (a short input is returned unpadded; a long one is trimmed).
        assert!(
            got.digits.len() <= c.prec,
            "round_sig produced {} significant digits > prec {} for [{}] input {:?}",
            got.digits.len(),
            c.prec,
            c.name,
            c.input
        );
    }
}

/// The all-nines carry-exponent bug fixed during fd-cb6: an explicit,
/// independent guard so the named row cannot be silently dropped from
/// the table without this test failing. `cos(1e-4) = 0.999999995`
/// must round to the value `1` at p7.
#[test]
fn named_guard_cos1e4_allnines_carry_present_and_correct() {
    let cases = load_cases();
    let g = cases
        .iter()
        .find(|c| c.name == "cos1e-4-allnines-carry")
        .expect("named regression guard cos1e-4-allnines-carry missing from the case table");
    assert_eq!(g.input, "0.999999995");
    assert_eq!(g.prec, 7);
    let got = round_sig(&parse_dec(&g.input), g.prec);
    assert!(
        same_value(&got, &parse_dec("1")),
        "all-nines carry regression: round_sig(0.999999995, 7) = {got:?}, must be the value 1"
    );
}
