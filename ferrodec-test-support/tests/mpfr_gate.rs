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
use rug::float::Round;
use rug::Float;

/// Decimal value as (negative, significant digits big-endian, power
/// of ten of the last digit). `digits` has no leading zeros; the
/// number is `(-1)^neg · digits · 10^exp`. Zero is `(false, [], 0)`.
struct Dec {
    neg: bool,
    digits: Vec<u8>,
    exp: i64,
}

fn parse_dec(s: &str) -> Dec {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (mant, e) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().expect("exponent")),
        None => (body, 0),
    };
    let (int_part, frac_part) = match mant.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mant, ""),
    };
    let mut digits: Vec<u8> = Vec::with_capacity(int_part.len() + frac_part.len());
    for c in int_part.bytes().chain(frac_part.bytes()) {
        digits.push(c - b'0');
    }
    // exponent of the least-significant digit
    let mut exp = e - frac_part.len() as i64;
    // strip leading zeros
    while digits.len() > 1 && digits[0] == 0 {
        digits.remove(0);
    }
    // strip trailing zeros (raising the exponent), canonical form
    while digits.len() > 1 && *digits.last().unwrap() == 0 {
        digits.pop();
        exp += 1;
    }
    if digits == [0] {
        return Dec {
            neg: false,
            digits: Vec::new(),
            exp: 0,
        };
    }
    Dec { neg, digits, exp }
}

/// Round `d` to at most `prec` significant digits, ties to even,
/// returned in the same canonical (trailing-zero-stripped) form so
/// two equal values compare equal.
fn round_sig(d: &Dec, prec: usize) -> Dec {
    if d.digits.len() <= prec {
        return Dec {
            neg: d.neg,
            digits: d.digits.clone(),
            exp: d.exp,
        };
    }
    let mut kept: Vec<u8> = d.digits[..prec].to_vec();
    let dropped_exp = d.exp + (d.digits.len() - prec) as i64;
    let next = d.digits[prec];
    let sticky = d.digits[prec + 1..].iter().any(|&x| x != 0);
    // Round half to even: up if the next digit exceeds 5, or it is
    // exactly 5 with anything nonzero after it, or it is an exact 5
    // tie and the last kept digit is odd.
    let round_up = next > 5 || (next == 5 && (sticky || kept.last().is_some_and(|&l| l % 2 == 1)));
    let mut exp = dropped_exp;
    if round_up {
        let mut i = kept.len();
        loop {
            if i == 0 {
                // All kept digits were 9: 999..9 + 1 carries into a
                // new leading digit. Inserting a high digit does not
                // move the least-significant digit's exponent; only
                // re-trimming the now-extra low digit raises it by
                // one (10000000 @ e-7  ->  1000000 @ e-6, i.e. 1.0).
                kept.insert(0, 1);
                if kept.len() > prec {
                    kept.pop();
                    exp += 1;
                }
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    while kept.len() > 1 && *kept.last().unwrap() == 0 {
        kept.pop();
        exp += 1;
    }
    if kept.iter().all(|&x| x == 0) {
        return Dec {
            neg: false,
            digits: Vec::new(),
            exp: 0,
        };
    }
    Dec {
        neg: d.neg,
        digits: kept,
        exp,
    }
}

fn same_value(a: &Dec, b: &Dec) -> bool {
    a.neg == b.neg && a.digits == b.digits && a.exp == b.exp
}

fn decimal_magnitude(input: &Dec) -> i64 {
    if input.digits.is_empty() {
        return 0;
    }
    input.exp + input.digits.len() as i64 - 1
}

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
