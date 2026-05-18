//! Python/libmpdec differential for `Decimal128` (Track 3, plan
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
//!   is structural membership in that 2-step band. `sin`/`cos`/`tan`/
//!   hyperbolic/`log2`/`cbrt` are absent because libmpdec does not
//!   provide them.
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

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::differential::{run_batch, Request};

// IEEE 754-2019 decimal128 interchange parameters (the Python side
// builds its Context from these so overflow/underflow/clamp semantics
// line up).
const PREC: u32 = 34;
const EMAX: i32 = 6144;
const EMIN: i32 = -6143;

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

fn parse(s: &str) -> Option<Decimal128> {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .ok()
        .map(|(v, _)| v)
}

/// Cohort-insensitive numeric equality. NaN↔NaN, infinities by sign,
/// finite by IEEE `compare`.
fn val_eq(got: Decimal128, py: &str) -> bool {
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
fn within_k(got: Decimal128, py: &str, k: u32) -> bool {
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
    got: Decimal128,
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

    let mut checked = 0usize;
    for (c, r) in cases.iter().zip(&resp) {
        let value = &r.value;
        let ctx = format!(
            "{}({:?}, {}) -> ferrodec {} | libmpdec {}",
            c.req.op, c.req.args, c.req.round, c.got, value
        );

        if c.faithful {
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
        "expected a substantial sweep, ran {checked}"
    );
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
