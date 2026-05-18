//! Python/libmpdec differential for `Decimal64` (Track 3, plan
//! 2026-05-17). Opt-in and local-only: gated on the `differential`
//! feature so the Python subprocess never runs in a default
//! `cargo test` or CI. A nightly job to run it is a deferred
//! follow-up.
//!
//! `CPython`'s stdlib `decimal` is libmpdec, a correctly-rounded
//! arbitrary-precision *decimal* implementation. It is an independent
//! decimal-native reference, distinct from the binary astro-float
//! oracle and the fixed decTest vectors, so it catches a class of
//! spec-interpretation defect the rest of the surface cannot.
//!
//! Comparison contract (see ADR-0021, ADR-0025, and the Track 2
//! fd-61r lesson):
//!
//! * **Value, not cohort.** Both implement GDA, but the
//!   preferred-exponent / §7.4 ideal-exponent policy can legitimately
//!   differ (fd-61r), so equality is the cohort-insensitive IEEE
//!   `compare` (`partial_cmp`), exactly as the D64↔D128 cross-check
//!   settled.
//! * **Exact ops** (`add` `sub` `mul` `div` `fma` `sqrt`): both are
//!   correctly rounded, so values must be equal and the unambiguous
//!   signals (`invalid`, `divbyzero`, `inexact`) must agree. `sqrt`
//!   is cross-checked under `NearestEven` only: libmpdec's
//!   `Decimal.sqrt` ignores the context rounding mode (always
//!   round-half-even), while ferrodec's is correctly rounded per
//!   direction (proven in `property_sqrt.rs`), so the directed modes
//!   are not comparable against this reference.
//! * **Faithful ops** (`exp` `ln` `log10` `pow`): ferrodec is
//!   faithful (≤1 ULP, ADR-0021), libmpdec is correctly rounded
//!   (≤0.5 ULP), so they differ by ≤2 representable steps; the check
//!   is structural membership in that 2-step band. The exp/ln family
//!   is swept across the full decade range, including the decades the
//!   fixed astro-float oracle skips (ADR-0026: scrutinise the
//!   widest-blast-radius primitives first).
//! * **mpmath special-function surface** (ADR-0026, fd-cb6): the
//!   functions `decimal` lacks (`exp2` `log2` `cbrt` `sin` `cos`
//!   `tan` `asin` `acos` `atan` `atan2` `sinh` `cosh` `tanh` `asinh`
//!   `acosh` `atanh`) are cross-checked against mpmath, which is
//!   structurally independent of BOTH the shared Extended kernel and
//!   the astro-float oracle and so breaks the correlated-failure
//!   surface. mpmath returns a high-precision true value; the format
//!   rounding is done on our side (no double rounding).
//!   `NearestEven` only (the per-direction faithful contract is
//!   covered by the
//!   astro-float property suites); sweeps deliberately reach the trig
//!   skip decades. mpmath is optional: when it is not importable
//!   these cases skip with a diagnostic, never fail.
//! * `overflow`/`underflow` are **not** strictly compared: their
//!   interaction with the fd-61r cohort policy under an IEEE context
//!   legitimately differs, the same reason status is not cross-checked
//!   in the cross-precision oracle.
//!
//! Operands are a fixed deterministic sweep (this is a batch-once
//! local check, not a fuzzer); the seed is constant so a failure
//! reproduces verbatim.

#![cfg(feature = "differential")]

use core::cmp::Ordering;

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::differential::{run_batch, Request};

// IEEE 754-2019 decimal64 interchange parameters (the Python side
// builds its Context from these so overflow/underflow/clamp semantics
// line up).
const PREC: u32 = 16;
const EMAX: i32 = 384;
const EMIN: i32 = -383;

const MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn round_name(rm: RoundingMode) -> &'static str {
    match rm {
        RoundingMode::NearestEven => "NearestEven",
        RoundingMode::NearestAway => "NearestAway",
        RoundingMode::TowardZero => "TowardZero",
        RoundingMode::TowardPositive => "TowardPositive",
        RoundingMode::TowardNegative => "TowardNegative",
    }
}

/// Deterministic xorshift64 — a batch-once local check wants a fixed,
/// reproducible sweep, not entropy.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// `coef · 10^exp` with `1 ≤ coef ≤ 999_999` (≤ 6 digits, exact in
    /// every format) and `exp ∈ [-lo, hi]`.
    fn operand(&mut self, lo: i32, hi: i32, allow_neg: bool) -> String {
        let coef = 1 + (self.next() % 999_999);
        let span = (hi + lo) as u64 + 1;
        let exp = (self.next() % span) as i32 - lo;
        let neg = allow_neg && (self.next() & 1 == 1);
        format!("{}{}e{}", if neg { "-" } else { "" }, coef, exp)
    }
}

fn parse(s: &str) -> Option<Decimal64> {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .ok()
        .map(|(v, _)| v)
}

/// Cohort-insensitive numeric equality. NaN↔NaN, infinities by sign,
/// finite by IEEE `compare`.
fn val_eq(got: Decimal64, py: &str) -> bool {
    if py == "NaN" || py == "sNaN" {
        return !got.is_finite() && got.to_string().contains("NaN");
    }
    if py.ends_with("Infinity") {
        return got.to_string() == py;
    }
    match parse(py) {
        Some(p) => got.partial_cmp(p).0 == Some(Ordering::Equal),
        None => false,
    }
}

/// `got` within `k` representable steps of the libmpdec value `py`
/// (the faithful-vs-correctly-rounded band).
fn within_k(got: Decimal64, py: &str, k: u32) -> bool {
    let Some(p) = parse(py) else {
        return py == "NaN" && !got.is_finite();
    };
    if got.partial_cmp(p).0 == Some(Ordering::Equal) {
        return true;
    }
    let (mut up, mut dn) = (p, p);
    for _ in 0..k {
        up = up.next_up().0;
        dn = dn.next_down().0;
        if got.partial_cmp(up).0 == Some(Ordering::Equal)
            || got.partial_cmp(dn).0 == Some(Ordering::Equal)
        {
            return true;
        }
    }
    false
}

struct Case {
    req: Request,
    got: Decimal64,
    /// `true` for the faithful ops (band check); `false` for the exact
    /// ops (value-equal + signal check).
    faithful: bool,
}

fn push_unary(
    cases: &mut Vec<Case>,
    op: &'static str,
    rng: &mut Rng,
    n: usize,
    faithful: bool,
    positive: bool,
    lo: i32,
    hi: i32,
    modes: &[RoundingMode],
) {
    for _ in 0..n {
        let a = rng.operand(lo, hi, !positive);
        let x = parse(&a).expect("generated operand parses");
        for &rm in modes {
            let got = match op {
                "sqrt" => x.sqrt(rm).0,
                "exp" => x.exp(rm).0,
                "ln" => x.ln(rm).0,
                "log10" => x.log10(rm).0,
                _ => unreachable!(),
            };
            cases.push(Case {
                req: Request {
                    op,
                    prec: PREC,
                    emax: EMAX,
                    emin: EMIN,
                    round: round_name(rm),
                    args: vec![format!("{x:e}")],
                },
                got,
                faithful,
            });
        }
    }
}

/// Special functions served by mpmath (the surface `decimal` lacks).
fn is_mpmath(op: &str) -> bool {
    matches!(
        op,
        "exp2"
            | "log2"
            | "cbrt"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
    )
}

/// Decade exponents that deliberately reach the astro-float oracle's
/// skip regions (ADR-0026, fd-cb6). The fixed-256-bit oracle is
/// unsound for trig past magnitude ~10^15 and loses bracketing digits
/// for the exp/ln family at large magnitude (the `property_sincos`
/// `coef.ilog10()+exp > 15` guard; fd-3cd / fd-dfs). libmpdec
/// (decimal-native, correctly rounded, no magnitude limit) and mpmath
/// (working precision scaled by the argument magnitude) are the
/// independent backstops there. Scaled to the format's exponent range
/// so every probe stays representable.
fn skip_decades() -> Vec<i32> {
    let mut v = vec![0, 1, 3, 6, 9, 12, 15, 16, 18, 24, 40];
    let mut k = 80;
    while k < EMAX - 8 {
        v.push(k);
        k *= 2;
    }
    v.push(EMAX - 8);
    v
}

/// exp/ln-first independent backstop in the oracle skip regions
/// (ADR-0026): scrutinise the widest-blast-radius primitives first.
/// libmpdec is decimal-native, correctly rounded, and unbounded in
/// magnitude, so `ln` across the full signed decade range and `exp`
/// up to just under the format overflow are checked exactly where the
/// fixed astro-float oracle skips. Faithful 2-step band (ferrodec ≤1
/// ULP, libmpdec correctly rounded at the requested mode).
fn push_libmpdec_skip(cases: &mut Vec<Case>, rng: &mut Rng, tmodes: &[RoundingMode]) {
    // ln of coef·10^k across the full (signed) decade range: always
    // finite and positive, so libmpdec returns a value, never a
    // special.
    for &k in &skip_decades() {
        for &sgn in &[1_i32, -1] {
            let coef = 1 + (rng.next() % 999_999);
            let x = parse(&format!("{coef}e{}", sgn * k)).expect("ln decade operand parses");
            for &rm in tmodes {
                cases.push(Case {
                    req: Request {
                        op: "ln",
                        prec: PREC,
                        emax: EMAX,
                        emin: EMIN,
                        round: round_name(rm),
                        args: vec![format!("{x:e}")],
                    },
                    got: x.ln(rm).0,
                    faithful: true,
                });
            }
        }
    }
    // exp over [−capv, capv]: exp overflows near x = Emax·ln 10, so
    // 0.85·that keeps every result finite (exercises the kernel, not
    // overflow plumbing). The far-negative end underflows to a finite
    // zero, still a valid value comparison.
    let capv = f64::from(EMAX) * core::f64::consts::LN_10 * 0.85;
    for i in 0..=40 {
        let v = capv * (f64::from(i) / 40.0);
        for &sgn in &["", "-"] {
            let micro = (v * 1.0e6).round() as i64;
            let s = format!("{sgn}{}.{:06}", micro / 1_000_000, micro % 1_000_000);
            let x = parse(&s).expect("exp magnitude operand parses");
            for &rm in tmodes {
                cases.push(Case {
                    req: Request {
                        op: "exp",
                        prec: PREC,
                        emax: EMAX,
                        emin: EMIN,
                        round: round_name(rm),
                        args: vec![format!("{x:e}")],
                    },
                    got: x.exp(rm).0,
                    faithful: true,
                });
            }
        }
    }
}

/// One mpmath unary case. `NearestEven` only: the high-precision
/// true value is rounded to the format on our side by the proven
/// parser (`within_k` → `parse_str`, which is `NearestEven`).
/// Out-of-domain
/// arguments are not generated; a `NaN` / `Infinity` / `Skip`
/// response is treated as skip-with-diagnostic (the differential
/// corroborates, it is never a gate — ADR-0026).
fn push_mp_unary(cases: &mut Vec<Case>, op: &'static str, arg: &str) {
    let rm = RoundingMode::NearestEven;
    let Some(x) = parse(arg) else { return };
    let got = match op {
        "exp2" => x.exp2(rm).0,
        "log2" => x.log2(rm).0,
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
        _ => unreachable!("unary mpmath op {op}"),
    };
    cases.push(Case {
        req: Request {
            op,
            prec: PREC,
            emax: EMAX,
            emin: EMIN,
            round: round_name(rm),
            args: vec![format!("{x:e}")],
        },
        got,
        faithful: true,
    });
}

fn push_mp_atan2(cases: &mut Vec<Case>, y: &str, x: &str) {
    let rm = RoundingMode::NearestEven;
    let (Some(yy), Some(xx)) = (parse(y), parse(x)) else {
        return;
    };
    cases.push(Case {
        req: Request {
            op: "atan2",
            prec: PREC,
            emax: EMAX,
            emin: EMIN,
            round: round_name(rm),
            args: vec![format!("{yy:e}"), format!("{xx:e}")],
        },
        got: yy.atan2(xx, rm).0,
        faithful: true,
    });
}

/// The special functions `decimal` lacks, cross-checked against
/// mpmath over an in-domain operand set that deliberately includes
/// the oracle skip decades (above all the Payne-Hanek trig path).
/// mpmath is structurally independent of BOTH the shared kernel and
/// astro-float, so it breaks the correlated-failure surface
/// (ADR-0026).
fn push_mpmath(cases: &mut Vec<Case>, rng: &mut Rng) {
    let decades = skip_decades();
    let coef = |rng: &mut Rng| 1 + (rng.next() % 999_999);

    // Trig over the skip decades — the key independent backstop for
    // Payne-Hanek argument reduction where the fixed oracle is
    // unsound.
    for &k in &decades {
        for &sgn in &["", "-"] {
            let a = format!("{sgn}{}e{k}", coef(rng));
            for op in ["sin", "cos", "tan"] {
                push_mp_unary(cases, op, &a);
            }
        }
    }
    // log2 (= ln·const) and cbrt (= exp(ln/3)) route through the
    // ln/exp primitives, so the full decade range is the independent
    // ln/exp backstop one level removed. exp2 over a finite-result
    // magnitude.
    for &k in &decades {
        push_mp_unary(cases, "log2", &format!("{}e{k}", coef(rng))); // > 0
        for &sgn in &["", "-"] {
            push_mp_unary(cases, "cbrt", &format!("{sgn}{}e{k}", coef(rng)));
        }
    }
    let e2cap = f64::from(EMAX) * 3.0; // 2^x finite for |x| < Emax/log10 2
    for i in 0..=30 {
        let v = e2cap * (f64::from(i) / 30.0);
        for &sgn in &["", "-"] {
            let m = (v * 1.0e3).round() as i64;
            push_mp_unary(cases, "exp2", &format!("{sgn}{}.{:03}", m / 1000, m % 1000));
        }
    }
    // Inverse trig: asin/acos on [−1, 1]; atan over a wide range;
    // atan2 over sign-varied pairs.
    for _ in 0..60 {
        let c = coef(rng); // |x| = c·10^-6 ≤ 0.999999 < 1
        for &sgn in &["", "-"] {
            let u = format!("{sgn}{c}e-6");
            push_mp_unary(cases, "asin", &u);
            push_mp_unary(cases, "acos", &u);
        }
    }
    for &k in &decades {
        for &sgn in &["", "-"] {
            push_mp_unary(cases, "atan", &format!("{sgn}{}e{}", coef(rng), k - 6));
        }
    }
    for _ in 0..80 {
        let ye = (rng.next() % 13) as i32 - 6;
        let xe = (rng.next() % 13) as i32 - 6;
        let ys = if rng.next() & 1 == 1 { "-" } else { "" };
        let xs = if rng.next() & 1 == 1 { "-" } else { "" };
        push_mp_atan2(
            cases,
            &format!("{ys}{}e{ye}", coef(rng)),
            &format!("{xs}{}e{xe}", coef(rng)),
        );
    }
    // Hyperbolic. sinh/cosh share exp's overflow, so cap |x| like
    // exp; tanh saturates and asinh ~ ln(2x), both always finite;
    // acosh needs x ≥ 1 (the 1+δ end exercises the log1p path); atanh
    // needs |x| < 1.
    let hcap = f64::from(EMAX) * core::f64::consts::LN_10 * 0.85;
    for i in 0..=24 {
        let v = hcap * (f64::from(i) / 24.0);
        let m = (v * 1.0e3).round() as i64;
        for &sgn in &["", "-"] {
            let a = format!("{sgn}{}.{:03}", m / 1000, m % 1000);
            push_mp_unary(cases, "sinh", &a);
            push_mp_unary(cases, "cosh", &a);
            push_mp_unary(cases, "tanh", &a);
            push_mp_unary(cases, "asinh", &a);
        }
    }
    for _ in 0..40 {
        let c = coef(rng);
        push_mp_unary(cases, "acosh", &format!("1.{c:06}")); // 1+δ, log1p path
        push_mp_unary(cases, "acosh", &format!("{c}e{}", rng.next() % 12)); // ≥ 1
        for &sgn in &["", "-"] {
            push_mp_unary(cases, "atanh", &format!("{sgn}{c}e-6")); // |x| < 1
        }
    }
}

#[test]
fn differential_against_libmpdec() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut cases: Vec<Case> = Vec::new();

    // Exact binary arithmetic, all five rounding modes.
    for _ in 0..120 {
        let a = rng.operand(20, 20, true);
        let b = rng.operand(20, 20, true);
        let c = rng.operand(20, 20, true);
        let (xa, xb, xc) = (parse(&a).unwrap(), parse(&b).unwrap(), parse(&c).unwrap());
        for &rm in &MODES {
            for op in ["add", "sub", "mul", "div"] {
                let got = match op {
                    "add" => xa.add(xb, rm).0,
                    "sub" => xa.sub(xb, rm).0,
                    "mul" => xa.mul(xb, rm).0,
                    "div" => xa.div(xb, rm).0,
                    _ => unreachable!(),
                };
                cases.push(Case {
                    req: Request {
                        op,
                        prec: PREC,
                        emax: EMAX,
                        emin: EMIN,
                        round: round_name(rm),
                        args: vec![format!("{xa:e}"), format!("{xb:e}")],
                    },
                    got,
                    faithful: false,
                });
            }
            let got = xa.fma(xb, xc, rm).0;
            cases.push(Case {
                req: Request {
                    op: "fma",
                    prec: PREC,
                    emax: EMAX,
                    emin: EMIN,
                    round: round_name(rm),
                    args: vec![format!("{xa:e}"), format!("{xb:e}"), format!("{xc:e}")],
                },
                got,
                faithful: false,
            });
        }
    }

    // sqrt: positive domain, NearestEven only. libmpdec's
    // `Decimal.sqrt` ignores the context rounding mode (empirically it
    // returns the round-half-even result for every mode), whereas
    // ferrodec's sqrt is correctly rounded per direction (proven by
    // the exact integer oracle in `property_sqrt.rs`). So the two are
    // comparable only under NearestEven; under a directed mode the
    // gap is libmpdec's limitation, not a ferrodec defect (the same
    // kind of cross-implementation semantic mismatch as the
    // `rem_trunc` finding in the D64→D128 cross-check).
    push_unary(
        &mut cases,
        "sqrt",
        &mut rng,
        120,
        false,
        true,
        20,
        20,
        &[RoundingMode::NearestEven],
    );

    // Faithful transcendentals libmpdec supports. Nearest + the two
    // outer directed modes (enough to exercise the directed band).
    let tmodes = [
        RoundingMode::NearestEven,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ];
    // exp/ln-first independent backstop across the oracle skip decades
    // (ADR-0026): pushed ahead of the narrow legacy sweeps so the
    // widest-blast-radius primitives are scrutinised first.
    push_libmpdec_skip(&mut cases, &mut rng, &tmodes);
    push_unary(&mut cases, "ln", &mut rng, 120, true, true, 20, 20, &tmodes);
    push_unary(
        &mut cases, "log10", &mut rng, 120, true, true, 20, 20, &tmodes,
    );
    // exp: |x| ∈ [0.001, 60] (signed) so the result stays finite in
    // every format and the sweep exercises the kernel, not overflow.
    for _ in 0..120 {
        let mag = 1 + (rng.next() % 60_000); // 0.001 .. 60.000
        let neg = rng.next() & 1 == 1;
        let s = format!("{}{}e-3", if neg { "-" } else { "" }, mag);
        let x = parse(&s).expect("exp operand parses");
        for &rm in &tmodes {
            cases.push(Case {
                req: Request {
                    op: "exp",
                    prec: PREC,
                    emax: EMAX,
                    emin: EMIN,
                    round: round_name(rm),
                    args: vec![format!("{x:e}")],
                },
                got: x.exp(rm).0,
                faithful: true,
            });
        }
    }
    // pow: x > 0, small finite y, the general (non-integer) path.
    for _ in 0..150 {
        let a = rng.operand(10, 10, false);
        let xa = parse(&a).unwrap();
        let yc = 1 + (rng.next() % 50);
        let yneg = rng.next() & 1 == 1;
        let yb = format!("{}{}e-1", if yneg { "-" } else { "" }, yc);
        let xy = parse(&yb).unwrap();
        for &rm in &tmodes {
            let got = xa.pow(xy, rm).0;
            cases.push(Case {
                req: Request {
                    op: "pow",
                    prec: PREC,
                    emax: EMAX,
                    emin: EMIN,
                    round: round_name(rm),
                    args: vec![format!("{xa:e}"), format!("{xy:e}")],
                },
                got,
                faithful: true,
            });
        }
    }

    // The special-function surface decimal lacks, against mpmath
    // (structurally independent of kernel + astro-float; ADR-0026).
    push_mpmath(&mut cases, &mut rng);

    let reqs: Vec<Request> = cases
        .iter()
        .map(|c| Request {
            op: c.req.op,
            prec: c.req.prec,
            emax: c.req.emax,
            emin: c.req.emin,
            round: c.req.round,
            args: c.req.args.clone(),
        })
        .collect();

    let Some(resp) = run_batch(&reqs) else {
        eprintln!(
            "differential: no usable python3/libmpdec found; skipping \
             (local-only differential, not a failure)"
        );
        return;
    };
    assert_eq!(resp.len(), cases.len());

    let (mut checked, mut skipped) = (0usize, 0usize);
    for (c, r) in cases.iter().zip(&resp) {
        let value = &r.value;
        let ctx = format!(
            "{}({:?}, {}) -> ferrodec {} | ref {}",
            c.req.op, c.req.args, c.req.round, c.got, value
        );

        if c.faithful {
            // `Skip` ⇒ mpmath not importable; for an mpmath op a
            // `NaN`/`Infinity` reference is an out-of-domain/overflow
            // probe. Either way the differential corroborates and is
            // never a gate (ADR-0026), so count and move on. libmpdec
            // faithful ops (exp/ln/log10/pow) stay strictly asserted:
            // their sweeps are in-domain by construction.
            if value.as_str() == "Skip"
                || (is_mpmath(c.req.op) && (value.as_str() == "NaN" || value.ends_with("Infinity")))
            {
                skipped += 1;
                continue;
            }
            assert!(within_k(c.got, value, 2), "faithful band: {ctx}");
        } else {
            assert!(val_eq(c.got, value), "value: {ctx}");
            // Unambiguous signals: both are correctly rounded GDA, so
            // these must agree. (overflow/underflow excluded — fd-61r
            // cohort × IEEE-context interaction.)
            let g = ferrodec_status(c);
            assert_eq!(g.0, r.invalid(), "INVALID flag: {ctx}");
            assert_eq!(g.1, r.divbyzero(), "DIVBYZERO flag: {ctx}");
            assert_eq!(g.2, r.inexact(), "INEXACT flag: {ctx}");
        }
        checked += 1;
    }
    assert!(
        checked > 2000,
        "expected a substantial sweep, ran {checked} (skipped {skipped})"
    );
    if skipped > 0 {
        eprintln!(
            "differential: {skipped} faithful case(s) skipped (mpmath \
             absent ⇒ run under `nix-shell -p python3Packages.mpmath`, \
             or an out-of-domain probe); corroborating check, not a \
             gate (ADR-0026)"
        );
    }
}

/// Re-run the operation to recover ferrodec's status flags
/// (`Case` stores only the value to keep the batch struct small).
fn ferrodec_status(c: &Case) -> (bool, bool, bool) {
    let a = parse(&c.req.args[0]).unwrap();
    let rm = MODES
        .iter()
        .copied()
        .find(|m| round_name(*m) == c.req.round)
        .unwrap();
    let st = match c.req.op {
        "add" => a.add(parse(&c.req.args[1]).unwrap(), rm).1,
        "sub" => a.sub(parse(&c.req.args[1]).unwrap(), rm).1,
        "mul" => a.mul(parse(&c.req.args[1]).unwrap(), rm).1,
        "div" => a.div(parse(&c.req.args[1]).unwrap(), rm).1,
        "fma" => {
            a.fma(
                parse(&c.req.args[1]).unwrap(),
                parse(&c.req.args[2]).unwrap(),
                rm,
            )
            .1
        }
        "sqrt" => a.sqrt(rm).1,
        _ => unreachable!("status only checked for exact ops"),
    };
    (st.invalid(), st.div_by_zero(), st.inexact())
}
