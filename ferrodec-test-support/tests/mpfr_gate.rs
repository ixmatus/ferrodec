//! MPFR correctly-rounded cross-validation of the Arb frozen corpus
//! (Phase 3 of fd-cb6, ADR-0026).
//!
//! Arb already *proved* every frozen value (Phase 2): a certified
//! enclosure that does not straddle a decimal half-ULP establishes
//! the correctly-rounded result. MPFR is the independent industrial
//! gold standard; two independent gold references agreeing is the
//! strongest acceptance criterion available, so the corpus-accept
//! rule is **Arb enclosure decisive AND MPFR agrees**. This test
//! closes that loop: it recomputes every vector with MPFR at high
//! working precision, performs the decimal rounding *on our side*
//! (no double rounding — the contract stays with us, ADR-0026), and
//! asserts the result equals the Arb-proven output. It also reports
//! MPFR's ternary-flag distribution (the sign of MPFR's own binary
//! rounding error), the instrument that confirms the corpus's
//! correctly-rounded value bit for bit: under ADR-0032 the entire
//! §9.2 production contract is correctly rounded across all three
//! formats, and the MPFR gate is the bit-for-bit empirical witness
//! at every committed vector.
//!
//! Local opt-in only: gated on `mpfr-gate`, so `rug`/MPFR (C-FFI,
//! LGPL) is never built by a default `cargo test`, never in CI,
//! never in the `no_std` build. A nightly lane is a deferred
//! follow-up.

#![cfg(feature = "mpfr-gate")]

use std::cmp::Ordering;

use ferrodec_test_support::frozen;
use ferrodec_test_support::round_dec::{
    decimal_magnitude, parse_dec, round_directed_sig, same_value, Round as DecRound,
};
use rug::float::Round;
use rug::ops::{DivAssignRound, PowAssignRound, SubAssignRound};
use rug::Float;

fn dec_round(mode: &str) -> DecRound {
    match mode {
        "NearestEven" => DecRound::NearestEven,
        "NearestAway" => DecRound::NearestAway,
        "TowardZero" => DecRound::TowardZero,
        "TowardPositive" => DecRound::TowardPositive,
        "TowardNegative" => DecRound::TowardNegative,
        other => panic!("frozen corpus has an unknown rounding mode {other:?}"),
    }
}

/// MPFR working precision (bits): generous headroom over the format
/// precision plus, for the trig family, ~3.34 bits per decimal digit
/// of |x| so MPFR's internal argument reduction survives the
/// cancellation in the skip decades the corpus deliberately reaches.
/// The `p1_anchor` class (`logp1`, ADR-0059 Track D) needs depth for
/// the opposite reason: a tiny argument's value hugs the argument
/// itself at relative distance ~x, so deciding a directed rounding
/// needs ~2·|mag| decimal digits (~6.7 bits each) before the residual
/// `x²/2` becomes visible; the base-variant compositions
/// (`log2p1` / `log10p1`) scale the value away from the grid and
/// keep the generic headroom.
/// The functions whose values hug a representable anchor at a depth
/// that scales with the argument (logp1's tiny band; the expm1
/// family's x anchor and −1 approach; exp10m1's all-nines positive
/// side). Their working precision and stringification guard must
/// reach past the agreement run or the directed modes collapse onto
/// the anchor (the D1/D2 review's width-collapse family).
fn is_anchor_hugging(func: &str) -> bool {
    matches!(func, "logp1" | "expm1" | "exp2m1" | "exp10m1" | "exp10")
}

fn work_bits(prec: usize, mag: i64, trig: bool, p1_anchor: bool) -> u32 {
    let base = 64 + 4 * (prec as i64 + 30);
    let extra = if trig {
        (mag.unsigned_abs() as i64 * 34 / 10) + 64
    } else if p1_anchor {
        // Tiny inputs: the residual appears ~2·|mag| digits down
        // (6.7 bits per digit). Moderate negative inputs (the −1
        // approach, |x| up to ~10^4): the agreement run is
        // ~|x|·log10(base) digits, bounded by 10^(mag+1); the cap
        // keeps the pathological corner inside MPFR's 2^20 ceiling.
        let tiny = mag.unsigned_abs() as i64 * 67 / 10;
        let approach = if (0..=4).contains(&mag) {
            10i64.pow(mag as u32 + 1) * 67 / 10
        } else {
            0
        };
        tiny.max(approach) + 64
    } else {
        64
    };
    (base + extra).min(1 << 20) as u32
}

fn eval(fv: &frozen::FrozenVec, p: u32) -> (Float, Ordering) {
    let x = Float::with_val(
        p,
        Float::parse(&fv.input).expect("frozen input parses in MPFR"),
    );
    // Binary: input1 = x, input2 = y. `pow(x, y)`;
    // `atan2(y, x)` (the proven value is atan2 of the second operand
    // over the first, matching ferrodec's `self = y`).
    match fv.func.as_str() {
        "pow" => {
            let y = Float::with_val(
                p,
                Float::parse(fv.input2.as_deref().expect("pow input2"))
                    .expect("frozen input2 parses in MPFR"),
            );
            let mut v = x;
            let ord = v.pow_assign_round(&y, Round::Nearest);
            return (v, ord);
        }
        "atan2" => {
            let mut y = Float::with_val(
                p,
                Float::parse(fv.input2.as_deref().expect("atan2 input2"))
                    .expect("frozen input2 parses in MPFR"),
            );
            let ord = y.atan2_round(&x, Round::Nearest);
            return (y, ord);
        }
        _ => {}
    }
    let mut v = x;
    let func = fv.func.as_str();
    let ord = match func {
        "exp" => v.exp_round(Round::Nearest),
        "ln" => v.ln_round(Round::Nearest),
        "log2" => v.log2_round(Round::Nearest),
        "log10" => v.log10_round(Round::Nearest),
        "exp2" => v.exp2_round(Round::Nearest),
        "cbrt" => v.cbrt_round(Round::Nearest),
        "sqrt" => v.sqrt_round(Round::Nearest),
        "sin" => v.sin_round(Round::Nearest),
        "cos" => v.cos_round(Round::Nearest),
        "tan" => v.tan_round(Round::Nearest),
        "asin" => v.asin_round(Round::Nearest),
        "acos" => v.acos_round(Round::Nearest),
        "atan" => v.atan_round(Round::Nearest),
        "sinh" => v.sinh_round(Round::Nearest),
        "cosh" => v.cosh_round(Round::Nearest),
        "tanh" => v.tanh_round(Round::Nearest),
        "asinh" => v.asinh_round(Round::Nearest),
        "acosh" => v.acosh_round(Round::Nearest),
        "atanh" => v.atanh_round(Round::Nearest),
        // ADR-0059 Track D: logp1 is MPFR-native (mpfr_log1p); the
        // base variants compose `ln_1p / ln(base)` at the same
        // generous working precision. The composition's second
        // rounding stays ~2^-(p-2) relative, far inside the decimal
        // guard-digit slack the whole gate rests on, and the returned
        // ternary (a distribution stat only, never part of the
        // verdict) is the final operation's.
        "logp1" => v.ln_1p_round(Round::Nearest),
        // ADR-0059 Track D D2: expm1 and exp10 are MPFR-native
        // (mpfr_expm1, mpfr_exp10); the base-m1 variants compose the
        // native power with an exact subtraction of 1 at the same
        // generous working precision (the subtraction is a single
        // correctly rounded op; the ternary stays a distribution
        // stat).
        "expm1" => v.exp_m1_round(Round::Nearest),
        "exp10" => v.exp10_round(Round::Nearest),
        "exp2m1" => {
            v.exp2_round(Round::Nearest);
            let one = Float::with_val(p, 1u32);
            v.sub_assign_round(&one, Round::Nearest)
        }
        "exp10m1" => {
            v.exp10_round(Round::Nearest);
            let one = Float::with_val(p, 1u32);
            v.sub_assign_round(&one, Round::Nearest)
        }
        "log2p1" => {
            v.ln_1p_round(Round::Nearest);
            let ln2 = Float::with_val(p, rug::float::Constant::Log2);
            v.div_assign_round(&ln2, Round::Nearest)
        }
        "log10p1" => {
            v.ln_1p_round(Round::Nearest);
            let mut ln10 = Float::with_val(p, 10u32);
            ln10.ln_round(Round::Nearest);
            v.div_assign_round(&ln10, Round::Nearest)
        }
        other => panic!("no MPFR mapping for {other:?}"),
    };
    (v, ord)
}

const TRIG: &[&str] = &["sin", "cos", "tan", "atan2"];

#[test]
fn mpfr_cross_validates_arb_corpus() {
    let (mut checked, mut disagree) = (0usize, 0usize);
    let (mut below, mut exactish, mut above) = (0usize, 0usize, 0usize);
    let mut first_disagreement: Option<String> = None;

    for &prec in &[7u32, 16, 34] {
        for v in frozen::load(prec) {
            let mut mag = decimal_magnitude(&parse_dec(&v.input)).unsigned_abs() as i64;
            if let Some(y2) = v.input2.as_deref() {
                mag += decimal_magnitude(&parse_dec(y2)).unsigned_abs() as i64;
            }
            let p = work_bits(
                prec as usize,
                mag,
                TRIG.contains(&v.func.as_str()),
                is_anchor_hugging(&v.func),
            );

            // MPFR at Round::Nearest with generous precision is the
            // exact reference; the directed decimal rounding for the
            // line's mode is done on our side (no double rounding, and
            // no dependence on rug exposing every MPFR mode).
            let (y, ternary) = eval(&v, p);
            match ternary {
                Ordering::Less => below += 1,
                Ordering::Equal => exactish += 1,
                Ordering::Greater => above += 1,
            }

            let dr = dec_round(&v.mode);
            // The stringification guard must reach past the value's
            // agreement with its decimal anchor: for `logp1`'s tiny
            // rows the value hugs the argument to ~2·|mag| digits
            // (the residual is x²/2), so a flat `prec + 15` truncates
            // the deciding digits and collapses the directed rounding
            // onto the anchor (the same failure shape the D1 review
            // found in its own mpmath oracle).
            let guard = prec as usize
                + 15
                + if is_anchor_hugging(&v.func) {
                    let tiny = 2 * mag.unsigned_abs() as usize;
                    let approach = if (0..=4).contains(&mag) {
                        10usize.pow(mag as u32 + 1)
                    } else {
                        0
                    };
                    tiny.max(approach)
                } else {
                    0
                };
            let mpfr_dec = round_directed_sig(
                &parse_dec(&y.to_string_radix(10, Some(guard))),
                prec as usize,
                dr,
            );
            let arb_dec = round_directed_sig(&parse_dec(&v.output), prec as usize, dr);
            checked += 1;
            if !same_value(&mpfr_dec, &arb_dec) {
                disagree += 1;
                first_disagreement.get_or_insert_with(|| {
                    format!(
                        "Arb/MPFR disagree (corpus-accept rule, ADR-0026): \
                         {} {}({}{}) p{prec} -> Arb {} | MPFR {}",
                        v.mode,
                        v.func,
                        v.input,
                        v.input2
                            .as_deref()
                            .map(|s| format!(", {s}"))
                            .unwrap_or_default(),
                        v.output,
                        y
                    )
                });
            }
        }
    }

    assert!(checked > 2000, "expected the full corpus, ran {checked}");
    assert!(
        disagree == 0,
        "{disagree} Arb/MPFR disagreement(s); first: {}",
        first_disagreement.as_deref().unwrap_or("?")
    );
    eprintln!(
        "MPFR cross-validation (ADR-0026 Phase 3): {checked} Arb frozen \
         vectors independently reproduced by MPFR, 0 disagreements — the \
         Arb-decisive-AND-MPFR-agrees accept rule holds across the whole \
         corpus. MPFR ternary distribution (sign of MPFR's own binary \
         rounding at the working precision): {below} below, {exactish} \
         exact, {above} above."
    );
}

/// ADR-0033 Plan C5: cross-validate the exhaustive-sweep worst-case
/// rows against MPFR. Each row is the tightest half-ULP margin input
/// across the function's full canonical Decimal32 input set — by the
/// proof-program argument the binding constraint for the kernel's
/// correctly-rounded contract at Decimal32. MPFR independently
/// reproducing the Arb-proven value on these rows is the defensive
/// confirmation that our oracle (Arb at variable precision up to
/// `CAP_BITS=65536`) is not silently disagreeing with the industrial
/// gold standard on the hardest known inputs.
#[test]
fn mpfr_cross_validates_exhaustive_worst_cases() {
    let (mut checked, mut disagree) = (0usize, 0usize);
    let mut first_disagreement: Option<String> = None;

    for v in frozen::load_exhaustive(7) {
        let mag = decimal_magnitude(&parse_dec(&v.input)).unsigned_abs() as i64;
        let p = work_bits(7, mag, TRIG.contains(&v.func.as_str()), v.func == "logp1");
        let (y, _ternary) = eval(&v, p);
        let dr = dec_round(&v.mode);
        let guard = 7 + 15;
        let mpfr_dec = round_directed_sig(&parse_dec(&y.to_string_radix(10, Some(guard))), 7, dr);
        let arb_dec = round_directed_sig(&parse_dec(&v.output), 7, dr);
        checked += 1;
        if !same_value(&mpfr_dec, &arb_dec) {
            disagree += 1;
            first_disagreement.get_or_insert_with(|| {
                format!(
                    "ADR-0033 exhaustive worst-case Arb/MPFR disagreement: \
                     {} {}({}) p7 -> Arb {} | MPFR {}",
                    v.mode, v.func, v.input, v.output, y
                )
            });
        }
    }

    assert!(
        checked >= 19,
        "expected 19 exhaustive worst-case rows (18 §9.2 + §5 sqrt), ran {checked}"
    );
    assert!(
        disagree == 0,
        "{disagree} exhaustive worst-case Arb/MPFR disagreement(s); first: {}",
        first_disagreement.as_deref().unwrap_or("?")
    );
    eprintln!(
        "ADR-0033 Plan C5 + ADR-0034 exhaustive worst-case MPFR \
         cross-validation: {checked} rows independently reproduced by \
         MPFR, 0 disagreements (18 §9.2 transcendentals + §5 sqrt). The \
         exhaustive-sweep oracle (Arb up to CAP_BITS=65536) agrees with \
         MPFR bit for bit on every function's tightest known input."
    );
}
