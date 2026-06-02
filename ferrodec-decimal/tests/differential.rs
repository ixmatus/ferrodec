//! Python/libmpdec differential for `ferrodec-decimal` (opt-in, local only,
//! gated on the `differential` feature so the Python subprocess never runs in
//! a default `cargo test` or CI).
//!
//! `CPython`'s stdlib `decimal` is libmpdec, the General Decimal Arithmetic
//! reference implementation. `ferrodec-decimal` implements the same
//! specification, including the ideal-exponent (cohort) rules, so the
//! comparison is **cohort-exact**: the canonical to-scientific strings must be
//! byte-identical, and the unambiguous condition flags (invalid, division by
//! zero, overflow, underflow, inexact) must agree. Any difference is a defect
//! here, since libmpdec is the reference.
//!
//! Operands are a fixed deterministic sweep (a batch-once local check, not a
//! fuzzer); the seed is constant so a failure reproduces verbatim. The sweep
//! mixes finite values, infinities, and NaNs across tight contexts that
//! actually reach the overflow, subnormal, and clamp boundaries, under all
//! eight rounding modes.

#![cfg(feature = "differential")]

use core::fmt::Write as _;
use ferrodec_decimal::{Context, Decimal, Rounding};
use std::io::Write as _;
use std::process::{Command, Stdio};

const PY: &str = r"
import sys, decimal
ROUND = {
    'half_even': decimal.ROUND_HALF_EVEN, 'half_up': decimal.ROUND_HALF_UP,
    'half_down': decimal.ROUND_HALF_DOWN, 'down': decimal.ROUND_DOWN,
    'up': decimal.ROUND_UP, 'ceiling': decimal.ROUND_CEILING,
    'floor': decimal.ROUND_FLOOR, '05up': decimal.ROUND_05UP,
}
out = []
for line in sys.stdin:
    line = line.rstrip('\n')
    if not line:
        continue
    op, prec, emax, emin, rnd, a, b, c = line.split('\t')
    ctx = decimal.Context(prec=int(prec), Emax=int(emax), Emin=int(emin),
                          rounding=ROUND[rnd], clamp=0, traps=[])
    da, db, dc = decimal.Decimal(a), decimal.Decimal(b), decimal.Decimal(c)
    if op == 'add':
        r = ctx.add(da, db)
    elif op == 'subtract':
        r = ctx.subtract(da, db)
    elif op == 'multiply':
        r = ctx.multiply(da, db)
    elif op == 'divide':
        r = ctx.divide(da, db)
    elif op == 'divide_int':
        r = ctx.divide_int(da, db)
    elif op == 'remainder':
        r = ctx.remainder(da, db)
    elif op == 'remainder_near':
        r = ctx.remainder_near(da, db)
    elif op == 'sqrt':
        r = ctx.sqrt(da)
    elif op == 'quantize':
        r = ctx.quantize(da, db)
    elif op == 'to_integral_value':
        r = ctx.to_integral_value(da)
    elif op == 'to_integral_exact':
        r = ctx.to_integral_exact(da)
    elif op == 'reduce':
        r = ctx.normalize(da)
    elif op == 'plus':
        r = ctx.plus(da)
    elif op == 'minus':
        r = ctx.minus(da)
    elif op == 'abs':
        r = ctx.abs(da)
    elif op == 'compare':
        r = ctx.compare(da, db)
    elif op == 'compare_total':
        r = ctx.compare_total(da, db)
    elif op == 'max':
        r = ctx.max(da, db)
    elif op == 'min':
        r = ctx.min(da, db)
    elif op == 'copy_abs':
        r = da.copy_abs()
    elif op == 'copy_negate':
        r = da.copy_negate()
    elif op == 'copy_sign':
        r = da.copy_sign(db)
    else:
        r = ctx.fma(da, db, dc)
    f = []
    if ctx.flags[decimal.InvalidOperation]: f.append('invalid')
    if ctx.flags[decimal.DivisionByZero]: f.append('divzero')
    if ctx.flags[decimal.Overflow]: f.append('overflow')
    if ctx.flags[decimal.Underflow]: f.append('underflow')
    if ctx.flags[decimal.Inexact]: f.append('inexact')
    out.append(str(r) + '\t' + (','.join(f) if f else '-'))
sys.stdout.write('\n'.join(out))
";

/// Small deterministic linear-congruential generator (constant seed).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

fn gen_operand(g: &mut Lcg) -> String {
    match g.below(24) {
        0 => "Infinity".to_string(),
        1 => "-Infinity".to_string(),
        2 => "NaN".to_string(),
        3 => format!("NaN{}", g.below(1000)),
        4 => "sNaN".to_string(),
        5 => "0".to_string(),
        _ => {
            let sign = if g.below(2) == 0 { "" } else { "-" };
            let ndig = 1 + g.below(12);
            let mut digits = String::new();
            for i in 0..ndig {
                let d = if i == 0 { 1 + g.below(9) } else { g.below(10) };
                digits.push(char::from(b'0' + d as u8));
            }
            let exp = g.below(31) as i32 - 15;
            format!("{sign}{digits}E{exp}")
        }
    }
}

fn ferrodec_flags(s: ferrodec_decimal::Status) -> String {
    let mut f = Vec::new();
    if s.invalid() {
        f.push("invalid");
    }
    if s.div_by_zero() {
        f.push("divzero");
    }
    if s.overflow() {
        f.push("overflow");
    }
    if s.underflow() {
        f.push("underflow");
    }
    if s.inexact() {
        f.push("inexact");
    }
    if f.is_empty() {
        "-".to_string()
    } else {
        f.join(",")
    }
}

const ROUNDINGS: [(&str, Rounding); 8] = [
    ("half_even", Rounding::HalfEven),
    ("half_up", Rounding::HalfUp),
    ("half_down", Rounding::HalfDown),
    ("down", Rounding::Down),
    ("up", Rounding::Up),
    ("ceiling", Rounding::Ceiling),
    ("floor", Rounding::Floor),
    ("05up", Rounding::ZeroFiveUp),
];

// (precision, Emax, Emin) triples, including tight ranges that reach the
// overflow / subnormal boundaries given the operand exponent span.
const CONTEXTS: [(u32, i32, i32); 4] = [(9, 999, -999), (7, 96, -95), (3, 9, -9), (1, 6, -6)];

const OPS: [&str; 23] = [
    "add",
    "subtract",
    "multiply",
    "divide",
    "divide_int",
    "remainder",
    "remainder_near",
    "sqrt",
    "fma",
    "quantize",
    "to_integral_value",
    "to_integral_exact",
    "reduce",
    "plus",
    "minus",
    "abs",
    "compare",
    "compare_total",
    "max",
    "min",
    "copy_abs",
    "copy_negate",
    "copy_sign",
];

#[test]
fn differential_core_arithmetic_vs_libmpdec() {
    let mut g = Lcg(0x1234_5678_9abc_def0);

    struct Case {
        op: &'static str,
        ctx: Context,
        rnd_name: &'static str,
        a: String,
        b: String,
        c: String,
    }
    let mut cases = Vec::new();
    let mut input = String::new();
    for _ in 0..8000 {
        let op = OPS[g.below(OPS.len() as u32) as usize];
        let (prec, emax, emin) = CONTEXTS[g.below(CONTEXTS.len() as u32) as usize];
        let (rnd_name, rounding) = ROUNDINGS[g.below(8) as usize];
        let a = gen_operand(&mut g);
        let b = gen_operand(&mut g);
        let c = gen_operand(&mut g);
        writeln!(
            input,
            "{op}\t{prec}\t{emax}\t{emin}\t{rnd_name}\t{a}\t{b}\t{c}"
        )
        .unwrap();
        cases.push(Case {
            op,
            ctx: Context::new(prec, emax, emin, rounding),
            rnd_name,
            a,
            b,
            c,
        });
    }

    let mut child = match Command::new("python3")
        .arg("-c")
        .arg(PY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIP differential: python3 unavailable ({e})");
            return;
        }
    };
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "python3 differential driver failed"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected: Vec<&str> = stdout.split('\n').collect();
    assert_eq!(expected.len(), cases.len(), "result count mismatch");

    let mut mismatches = 0;
    for (case, exp_line) in cases.iter().zip(&expected) {
        let (exp_str, exp_flags) = exp_line.split_once('\t').expect("py line shape");
        let da = Decimal::parse_str(&case.a).expect("operand a");
        let db = Decimal::parse_str(&case.b).expect("operand b");
        let dc = Decimal::parse_str(&case.c).expect("operand c");
        let (r, status) = match case.op {
            "add" => da.add(&db, &case.ctx),
            "subtract" => da.subtract(&db, &case.ctx),
            "multiply" => da.multiply(&db, &case.ctx),
            "divide" => da.divide(&db, &case.ctx),
            "divide_int" => da.divide_integer(&db, &case.ctx),
            "remainder" => da.remainder(&db, &case.ctx),
            "remainder_near" => da.remainder_near(&db, &case.ctx),
            "sqrt" => da.sqrt(&case.ctx),
            "quantize" => da.quantize(&db, &case.ctx),
            "to_integral_value" => da.round_to_integral_value(&case.ctx),
            "to_integral_exact" => da.round_to_integral_exact(&case.ctx),
            "reduce" => da.reduce(&case.ctx),
            "plus" => da.plus(&case.ctx),
            "minus" => da.minus(&case.ctx),
            "abs" => da.abs(&case.ctx),
            "compare" => da.compare(&db, &case.ctx),
            "compare_total" => (da.compare_total(&db), ferrodec_decimal::Status::OK),
            "max" => da.max(&db, &case.ctx),
            "min" => da.min(&db, &case.ctx),
            "copy_abs" => (da.copy_abs(), ferrodec_decimal::Status::OK),
            "copy_negate" => (da.copy_negate(), ferrodec_decimal::Status::OK),
            "copy_sign" => (da.copy_sign(&db), ferrodec_decimal::Status::OK),
            _ => da.fma(&db, &dc, &case.ctx),
        };
        let got_str = r.to_string();
        let got_flags = ferrodec_flags(status);
        if got_str != exp_str || got_flags != exp_flags {
            mismatches += 1;
            if mismatches <= 30 {
                eprintln!(
                    "MISMATCH {} {} [{}] a={} b={} c={}\n  ferrodec: {got_str:?} [{got_flags}]\n  libmpdec: {exp_str:?} [{exp_flags}]",
                    case.op, case.rnd_name, case.ctx.precision, case.a, case.b, case.c
                );
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "{mismatches} differential mismatches vs libmpdec"
    );
}
