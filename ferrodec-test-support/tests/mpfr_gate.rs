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
//! rounding error), the instrument ADR-0026 names for distinguishing
//! faithful from correctly rounded.
//!
//! Local opt-in only: gated on `mpfr-gate`, so `rug`/MPFR (C-FFI,
//! LGPL) is never built by a default `cargo test`, never in CI,
//! never in the `no_std` build. A nightly lane is a deferred
//! follow-up.

#![cfg(feature = "mpfr-gate")]

use std::cmp::Ordering;

use ferrodec_test_support::frozen;
use ferrodec_test_support::round_dec::{decimal_magnitude, parse_dec, round_sig, same_value};
use rug::float::Round;
use rug::Float;

/// MPFR working precision (bits): generous headroom over the format
/// precision plus, for the trig family, ~3.34 bits per decimal digit
/// of |x| so MPFR's internal argument reduction survives the
/// cancellation in the skip decades the corpus deliberately reaches.
fn work_bits(prec: usize, mag: i64, trig: bool) -> u32 {
    let base = 64 + 4 * (prec as i64 + 30);
    let extra = if trig {
        (mag.unsigned_abs() as i64 * 34 / 10) + 64
    } else {
        64
    };
    (base + extra).min(1 << 20) as u32
}

fn eval(func: &str, x: Float) -> (Float, Ordering) {
    let mut v = x;
    let ord = match func {
        "exp" => v.exp_round(Round::Nearest),
        "ln" => v.ln_round(Round::Nearest),
        "log2" => v.log2_round(Round::Nearest),
        "log10" => v.log10_round(Round::Nearest),
        "exp2" => v.exp2_round(Round::Nearest),
        "cbrt" => v.cbrt_round(Round::Nearest),
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
        other => panic!("no MPFR mapping for {other:?}"),
    };
    (v, ord)
}

const TRIG: &[&str] = &["sin", "cos", "tan"];

#[test]
fn mpfr_cross_validates_arb_corpus() {
    let (mut checked, mut disagree) = (0usize, 0usize);
    let (mut below, mut exactish, mut above) = (0usize, 0usize, 0usize);
    let mut first_disagreement: Option<String> = None;

    for &prec in &[7u32, 16, 34] {
        for v in frozen::load(prec) {
            let inp = parse_dec(&v.input);
            let mag = decimal_magnitude(&inp);
            let p = work_bits(prec as usize, mag, TRIG.contains(&v.func.as_str()));

            let parsed = Float::parse(&v.input).expect("frozen input parses in MPFR");
            let x = Float::with_val(p, parsed);
            let (y, ternary) = eval(&v.func, x);
            match ternary {
                Ordering::Less => below += 1,
                Ordering::Equal => exactish += 1,
                Ordering::Greater => above += 1,
            }

            // Decimal rounding on our side: take generous guard
            // digits from MPFR, round to `prec` significant digits
            // ourselves, compare to the Arb-proven output.
            let guard = prec as usize + 15;
            let mpfr_dec = round_sig(
                &parse_dec(&y.to_string_radix(10, Some(guard))),
                prec as usize,
            );
            let arb_dec = round_sig(&parse_dec(&v.output), prec as usize);
            checked += 1;
            if !same_value(&mpfr_dec, &arb_dec) {
                disagree += 1;
                first_disagreement.get_or_insert_with(|| {
                    format!(
                        "Arb/MPFR disagree (corpus-accept rule, \
                         ADR-0026): {}({}) p{prec} -> Arb {} | MPFR {}",
                        v.func, v.input, v.output, y
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
