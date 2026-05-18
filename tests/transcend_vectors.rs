//! Arb/FLINT frozen hard-to-round vector gate for `Decimal128`
//! (Phase 2 of fd-cb6, ADR-0026; directed modes + binary `pow`/`atan2`
//! fd-97a).
//!
//! The corpus (`tests/vectors/transcend/`) is committed data: the
//! *proven* correctly-rounded value of each transcendental at a chosen
//! argument under a rounding mode, including a decimal
//! Table-Maker's-Dilemma worst-case search (the half-ULP tie for
//! `NearestEven`, the grid-point boundary for the directed modes).
//! Arb's certified ball enclosure makes each value a proof, not a
//! sample: where the enclosure does not straddle the mode's rounding
//! boundary the correctly-rounded result is established. There is no
//! oracle and no C-FFI in this test's path — it parses checked-in
//! text — so it is default-on and runs in standard CI under
//! `--features transcendentals`, unlike the gated astro-float /
//! mpmath / MPFR references.
//!
//! Contract. `ferrodec` promises *faithful* rounding (≤1 ULP,
//! ADR-0021), not correct rounding (a decimal CRlibm-class research
//! problem; ADR-0024). So the gate is, per rounding direction: the
//! kernel result is within one representable step of the proven
//! correctly-rounded value (value, not cohort — the fd-61r
//! preferred-exponent policy can legitimately differ, so equality is
//! the cohort-insensitive IEEE `compare`). The exact-vs-one-step split
//! is reported per mode: it is the honest evidence for how often the
//! faithful kernel happens to be correctly rounded, the quantity
//! ADR-0026's honest-level statement and the Phase 3 ternary-flag
//! probe speak to.

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

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal128 {
    let x = parse(&v.input);
    match v.func.as_str() {
        // Binary: input1 = x, input2 = y. `pow(x, y)`,
        // `atan2(y, x)` (ferrodec's `atan2` is `self = y`).
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        "atan2" => {
            parse(v.input2.as_deref().expect("atan2 input2"))
                .atan2(x, rm)
                .0
        }
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
/// contract (a real defect). Value, not cohort. Mode-agnostic: the
/// proven value already encodes the rounding direction.
fn step_distance(got: Decimal128, cr: Decimal128) -> Option<u8> {
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

    // (exact, one_step) tallied per rounding mode for the honest split.
    let mut by_mode: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for v in &vectors {
        let rm = mode(&v.mode);
        let cr = parse(&v.output);
        let got = kernel(v, rm);
        let slot = by_mode.entry(v.mode.clone()).or_insert((0, 0));
        match step_distance(got, cr) {
            Some(0) => slot.0 += 1,
            Some(_) => slot.1 += 1,
            None => panic!(
                "faithful contract violated [{}]: {}({}{}) -> ferrodec {} | \
                 proven correctly-rounded {} (ADR-0021/0026)",
                v.mode,
                v.func,
                v.input,
                v.input2
                    .as_deref()
                    .map(|y| format!(", {y}"))
                    .unwrap_or_default(),
                got,
                cr
            ),
        }
    }
    let (mut exact, mut one_step) = (0usize, 0usize);
    for (e, o) in by_mode.values() {
        exact += e;
        one_step += o;
    }
    eprintln!(
        "frozen Arb vectors (Decimal128, p{PREC}): {} checked, {exact} exactly \
         correctly-rounded, {one_step} faithful at one step. Per mode \
         (exact/one-step): {by_mode:?}. Proven against Arb certified \
         enclosures (ADR-0026); MPFR cross-validates the same corpus in \
         Phase 3.",
        exact + one_step
    );
}
