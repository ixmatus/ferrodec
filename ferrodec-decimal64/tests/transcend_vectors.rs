//! Arb/FLINT frozen hard-to-round vector gate for `Decimal64`
//! (Phase 2 of fd-cb6, ADR-0026).
//!
//! The corpus (`tests/vectors/transcend/`) is committed data: the
//! *proven* `NearestEven` correctly-rounded value of each transcendental
//! at a chosen argument, including a decimal Table-Maker's-Dilemma
//! worst-case search. Arb's certified ball enclosure makes each value
//! a proof, not a sample: where the enclosure does not straddle a
//! decimal half-ULP the correctly-rounded result is established. There
//! is no oracle and no C-FFI in this test's path — it parses checked-in
//! text — so it is default-on and runs in standard CI under
//! `--features transcendentals`, unlike the gated astro-float / mpmath
//! / MPFR references.
//!
//! Contract. `ferrodec-decimal64` promises *faithful* rounding (≤1
//! ULP, ADR-0021), not correct rounding (a decimal CRlibm-class
//! research problem; ADR-0024). So the gate is: the kernel result is
//! within one representable step of the proven correctly-rounded value
//! (value, not cohort — the fd-61r preferred-exponent policy can
//! legitimately differ, so equality is the cohort-insensitive IEEE
//! `compare`). The exact-vs-one-step split is reported: it is the
//! honest evidence for how often the faithful kernel happens to be
//! correctly rounded, the quantity ADR-0026's honest-level statement
//! and the Phase 3 ternary-flag probe speak to. `pow`/`atan2` are
//! absent from the corpus (binary; `pow` is already cross-checked
//! correctly-rounded against decimal-native libmpdec in the Phase 1
//! differential).

#![cfg(all(feature = "exp-log", feature = "trig", feature = "hyperbolic"))]

use core::cmp::Ordering;

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 16;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("frozen token parses: {s:?}"))
        .0
}

fn kernel(func: &str, x: Decimal64) -> Decimal64 {
    let rm = RoundingMode::NearestEven;
    match func {
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
        other => panic!("frozen corpus has no kernel mapping for {other:?}"),
    }
}

/// `0` ⇒ exactly the correctly-rounded value; `1` ⇒ one representable
/// step away (still faithful, ADR-0021); `None` ⇒ outside the faithful
/// contract (a real defect). Value, not cohort.
fn step_distance(got: Decimal64, cr: Decimal64) -> Option<u8> {
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
fn frozen_arb_vectors_faithful() {
    let vectors = frozen::load(PREC);
    assert!(
        vectors.len() > 500,
        "expected a substantial frozen corpus, loaded {}",
        vectors.len()
    );

    let (mut exact, mut one_step) = (0usize, 0usize);
    for v in &vectors {
        let x = parse(&v.input);
        let cr = parse(&v.output);
        let got = kernel(&v.func, x);
        match step_distance(got, cr) {
            Some(0) => exact += 1,
            Some(_) => one_step += 1,
            None => panic!(
                "faithful contract violated: {}({}) -> ferrodec {} | \
                 proven correctly-rounded {} (ADR-0021/0026)",
                v.func, v.input, got, cr
            ),
        }
    }
    let total = exact + one_step;
    eprintln!(
        "frozen Arb vectors (Decimal64, p{PREC}): {total} checked, \
         {exact} exactly correctly-rounded, {one_step} faithful at one \
         step. Proven against Arb certified enclosures (ADR-0026); \
         MPFR cross-validates the same corpus in Phase 3."
    );
}
