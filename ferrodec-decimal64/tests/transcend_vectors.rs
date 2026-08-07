//! Arb/FLINT frozen hard-to-round vector gate for `Decimal64`
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
//! Contract. `ferrodec` promises *correctly rounded* §9.2
//! transcendentals (ADR-0032; supersedes ADR-0024's faithful
//! contract). The gate is, per rounding direction: the kernel result
//! equals the proven correctly rounded value (value, not cohort, so
//! the fd-61r preferred-exponent policy can legitimately differ;
//! equality is the cohort insensitive IEEE `compare`). One
//! representable step away or worse is a contract violation and
//! panics. The per mode tally is preserved for diagnostic output;
//! the contract requires every count to come from the exact bucket.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use core::cmp::Ordering;

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 16;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
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

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal64 {
    let x = parse(&v.input);
    match v.func.as_str() {
        // Binary: input1 = x, input2 = y. `pow(x, y)`,
        // `atan2(y, x)` (ferrodec's `atan2` is `self = y`).
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        "rsqrt" => x.rsqrt(rm).0,
        "hypot" => x.hypot(parse(v.input2.as_deref().expect("hypot input2")), rm).0,
        "powi" => {
            let n: i32 = v.input2.as_deref().expect("powi n").parse().expect("powi i32");
            x.powi(n, rm).0
        }
        "rootn" => {
            let n: i32 = v.input2.as_deref().expect("rootn n").parse().expect("rootn i32");
            x.rootn(n, rm).0
        }
        "compound" => {
            let n: i32 = v.input2.as_deref().expect("compound n").parse().expect("compound i32");
            x.compound(n, rm).0
        }
        "powr" => x.powr(parse(v.input2.as_deref().expect("powr input2")), rm).0,
        "atan2" => {
            parse(v.input2.as_deref().expect("atan2 input2"))
                .atan2(x, rm)
                .0
        }
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

/// `0` ⇒ exactly the correctly rounded value (ADR-0032); `1` ⇒ one
/// representable step away (a contract violation under ADR-0032);
/// `None` ⇒ multiple representable steps away (also a violation).
/// Value, not cohort. Mode agnostic: the proven value already
/// encodes the rounding direction.
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
fn frozen_arb_vectors_correctly_rounded() {
    let vectors = frozen::load(PREC);
    // Per-(func,mode) exact pins, not an aggregate `len()` floor: a
    // floor admits silent compensating drift (one bucket gains while
    // another loses). fd-aqs.10.
    frozen::assert_bucket_counts(&vectors, frozen::EXPECTED_BUCKETS_P16);

    // Exact bucket count tallied per rounding mode for diagnostic
    // output. Anything other than `Some(0)` panics; ADR-0032 admits no
    // off by one slot.
    let mut by_mode: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for v in &vectors {
        let rm = mode(&v.mode);
        let cr = parse(&v.output);
        let got = kernel(v, rm);
        match step_distance(got, cr) {
            Some(0) => *by_mode.entry(v.mode.clone()).or_insert(0) += 1,
            Some(d) => panic!(
                "correctly rounded contract violated ({d} step) [{}]: \
                 {}({}{}) -> ferrodec {} | proven correctly rounded {} \
                 (ADR-0032)",
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
            None => panic!(
                "correctly rounded contract violated (multi step) [{}]: \
                 {}({}{}) -> ferrodec {} | proven correctly rounded {} \
                 (ADR-0032)",
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
    let exact: usize = by_mode.values().sum();
    eprintln!(
        "frozen Arb vectors (Decimal64, p{PREC}): {exact} checked, all \
         exactly correctly rounded (ADR-0032). Per mode counts: \
         {by_mode:?}. Proven against Arb certified enclosures \
         (ADR-0026); MPFR cross validates the same corpus in Phase 3."
    );
}
