//! decTest transcendental replay against `Decimal128` (fd-4zo.8): the
//! `precision: 34` subset of the vendored `exp` / `ln` / `log10` /
//! `power` files from Mike Cowlishaw's General Decimal Arithmetic
//! Testcases, version 2.62 (`tests/vectors/gda-transcend/`; provenance
//! in that directory's README and the `cowlishaw-dectest` registry
//! entry).
//!
//! These are the only externally authored expected values the §9.2
//! transcendental surface replays: every other transcendental gate in
//! this repository certifies its own vectors (Arb corpus, MPFR gate,
//! exhaustive programs). An independent author's correctly rounded
//! rows are cheap external calibration, which is exactly the lane
//! plan's reason for this bead.
//!
//! ## What is honorable at p = 34
//!
//! The files are precision-parameterized. The GDA crate replays every
//! block; `Decimal128` is fixed at p = 34, so this gate:
//!
//! * replays only rows whose active `precision:` is 34 (all such
//!   blocks run `half_even`, mapped through the shared
//!   [`conformance::map_rounding`]);
//! * inside p = 34 blocks whose `maxExponent`/`minExponent` are
//!   narrower than Decimal128's (the files reuse the ±383 range with
//!   34-digit precision), skips rows whose expected conditions are
//!   range effects — `Overflow`, `Underflow`, `Subnormal`, `Clamped`
//!   assert the narrow range's dispositions, which the wider format
//!   correctly does not reproduce. Rows without range conditions are
//!   range-inert: the same digits come out at any wider range;
//! * skips rows whose *operand* is not a format value — GDA takes
//!   operands unrounded at any width, and seven rows probe with
//!   35-digit operands (one `log10`, six `power`, e.g.
//!   `power(10, 2.99…93)`) that no `Decimal128` caller can supply.
//!   Detected by the operand parse raising `INEXACT`
//!   ([`parse_exact_operand`]).
//!
//! The honored subset is 221 rows (41 exp, 42 ln, 50 log10,
//! 88 power), every one passing bit-exact with matching flags.
//!
//! Skips land in the run summary; the pass counts are pinned exactly
//! per file (ADR-0010's asymmetric guard, via
//! [`conformance::run_suite`]), and the fail ceiling is zero.
//!
//! ## Comparison semantics
//!
//! Bit-exact on the value (cohort included) plus the five IEEE flags,
//! mirroring the `dq*` runner. Two facts make the strict cohort bar
//! sound here rather than aspirational. The inexact rows fill the
//! full precision in both semantics. The exact rows all carry
//! operands of stored quantum 0, where GDA's ideal exponent 0 and
//! IEEE 754-2019 §9.2.2's preferred exponents coincide (the corpus's
//! single exact `power` row is `powx2515`, `power(10, 3.00…0) =
//! 1000`, with `Q(10) = 0`), so the gate is insensitive to the
//! fd-5g6 preferred-exponent repair by inspection, not by luck.
//! `Rounded`/`Lost_digits` are informational in decTest and ignored;
//! `CLAMPED` is masked per the shared
//! [`conformance::status_conformance_eq`].

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::{
    self, decode_conditions, map_rounding, status_conformance_eq, Context, Outcome, TestCase,
};

const VECTORS_DIR: &str = "tests/vectors/gda-transcend";

/// Exact per-file pass pins (ADR-0010): every intentional change to
/// the honored subset edits this table visibly.
const EXPECTED_PER_FILE: &[(&str, usize)] = &[
    ("exp.decTest", 41),
    ("ln.decTest", 42),
    ("log10.decTest", 50),
    ("power.decTest", 88),
];

fn parse(s: &str, rm: RoundingMode) -> Option<Decimal128> {
    Decimal128::parse_str(s.trim(), rm).ok().map(|(d, _)| d)
}

/// Parse an operand that must be exactly representable at p = 34.
/// `Ok(None)` means the token names a value outside the format's
/// input set (an INEXACT parse: the GDA files carry a few 35-digit
/// operands, e.g. `power(10, 2.99…93)` with 34 nines-and-a-tail,
/// which no fixed-format caller can ever supply) — the caller skips
/// the row. `Err` is a genuinely unparseable token: a loud failure.
fn parse_exact_operand(s: &str, rm: RoundingMode) -> Result<Option<Decimal128>, String> {
    match Decimal128::parse_str(s.trim(), rm) {
        Ok((d, st)) if st.inexact() => {
            debug_assert!(!d.is_nan(), "inexact parse of a NaN token");
            Ok(None)
        }
        Ok((d, _)) => Ok(Some(d)),
        Err(_) => Err(format!("operand unparseable: {s:?}")),
    }
}

fn dispatch(case: &TestCase, ctx: &Context) -> Outcome {
    // Only the p = 34 blocks are Decimal128's context; everything
    // else is the GDA crate's territory (its own conformance suite
    // replays the full files).
    if ctx.precision != 34 {
        return Outcome::Skip;
    }
    let Some(rm) = map_rounding(&ctx.rounding) else {
        return Outcome::Skip;
    };
    // Hybrid-range blocks (p = 34 over a narrower exponent range):
    // range-effect rows assert the narrow range's dispositions and
    // are skipped; everything else is range-inert (module doc).
    let d128_range = ctx.max_exponent == 6144 && ctx.min_exponent == -6143;
    if !d128_range
        && case.conditions.iter().any(|c| {
            matches!(
                c.as_str(),
                "overflow" | "underflow" | "subnormal" | "clamped"
            )
        })
    {
        return Outcome::Skip;
    }
    // The `#` operand encodings never appear in these four files;
    // skip defensively rather than misparse if a re-vendor adds one.
    if case.operands.iter().any(|o| o.starts_with('#')) {
        return Outcome::Skip;
    }

    let x = match parse_exact_operand(&case.operands[0], rm) {
        Ok(Some(d)) => d,
        Ok(None) => return Outcome::Skip, // 35-digit operand: not a format input
        Err(e) => return Outcome::Fail(e),
    };
    let (actual, actual_flags) = match case.op.as_str() {
        "exp" => x.exp(rm),
        "ln" => x.ln(rm),
        "log10" => x.log10(rm),
        "power" => {
            let y = match case.operands.get(1).map(|s| parse_exact_operand(s, rm)) {
                Some(Ok(Some(d))) => d,
                Some(Ok(None)) => return Outcome::Skip,
                Some(Err(e)) => return Outcome::Fail(e),
                None => return Outcome::Fail("power without a second operand".to_string()),
            };
            x.pow(y, rm)
        }
        other => return Outcome::Fail(format!("unexpected op {other:?} in a transcendental file")),
    };

    let Some(expected) = parse(&case.expected, rm) else {
        return Outcome::Fail(format!("expected unparseable: {:?}", case.expected));
    };

    // Value: NaN via the Display projection (sign, sNaN, payload —
    // the fields decTest pins), else bit-exact, cohort included.
    if expected.is_nan() {
        if !actual.is_nan() {
            return Outcome::Fail(format!(
                "expected NaN, got {actual} ({:032X})",
                actual.to_bits()
            ));
        }
        let (got, want) = (format!("{actual}"), format!("{expected}"));
        if got != want {
            return Outcome::Fail(format!("NaN mismatch: got {got}, want {want}"));
        }
    } else if actual.to_bits() != expected.to_bits() {
        return Outcome::Fail(format!(
            "value mismatch: got {actual} ({:032X}), want {} ({:032X})",
            actual.to_bits(),
            expected,
            expected.to_bits()
        ));
    }

    let expected_flags = decode_conditions(&case.conditions);
    if !status_conformance_eq(actual_flags, expected_flags) {
        return Outcome::Fail(format!(
            "status mismatch: got {actual_flags:?}, want {expected_flags:?} from {:?}",
            case.conditions
        ));
    }
    Outcome::Pass
}

/// The replay gate: every honored row passes, the honored subset's
/// size is pinned per file, and no line goes silently unparsed.
#[test]
fn dectest_transcendental_p34_replay() {
    let ctx = Context::for_decimal128();
    conformance::run_suite(VECTORS_DIR, EXPECTED_PER_FILE, &ctx, dispatch);
}
