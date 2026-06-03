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
//! here, since libmpdec is the reference. The one exception is `power`, which is
//! compared within a one-ulp band: this crate's `power` is correctly rounded by
//! construction while libmpdec's is only "almost always" correctly rounded, so
//! on the rare table-maker's-dilemma input this crate is the stronger of the
//! two and a one-ulp disagreement is expected, not a defect.
//!
//! Operands are a fixed deterministic sweep (a batch-once local check, not a
//! fuzzer); the seed is constant so a failure reproduces verbatim. The sweep
//! mixes finite values, infinities, and NaNs across tight contexts that
//! actually reach the overflow, subnormal, and clamp boundaries, under all
//! eight rounding modes.

#![cfg(feature = "differential")]

use core::fmt::Write as _;
use ferrodec_decimal::{Context, DecBig, Decimal, Rounding, Status};
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
    elif op == 'exp':
        r = ctx.exp(da)
    elif op == 'ln':
        r = ctx.ln(da)
    elif op == 'log10':
        r = ctx.log10(da)
    elif op == 'power':
        r = ctx.power(da, db)
    elif op == 'logical_and':
        r = ctx.logical_and(da, db)
    elif op == 'logical_or':
        r = ctx.logical_or(da, db)
    elif op == 'logical_xor':
        r = ctx.logical_xor(da, db)
    elif op == 'logical_invert':
        r = ctx.logical_invert(da)
    elif op == 'shift':
        r = ctx.shift(da, db)
    elif op == 'rotate':
        r = ctx.rotate(da, db)
    elif op == 'compare_signal':
        r = ctx.compare_signal(da, db)
    elif op == 'compare_total_mag':
        r = da.compare_total_mag(db)
    elif op == 'max_mag':
        r = ctx.max_mag(da, db)
    elif op == 'min_mag':
        r = ctx.min_mag(da, db)
    elif op == 'same_quantum':
        r = decimal.Decimal(1) if da.same_quantum(db) else decimal.Decimal(0)
    elif op == 'copy':
        r = da
    elif op == 'scaleb':
        r = ctx.scaleb(da, db)
    elif op == 'logb':
        r = ctx.logb(da)
    elif op == 'next_plus':
        r = ctx.next_plus(da)
    elif op == 'next_minus':
        r = ctx.next_minus(da)
    elif op == 'next_toward':
        r = ctx.next_toward(da, db)
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

/// Operand generator biased toward valid logical operands: a string of `0`/`1`
/// digits at exponent zero. The general generator almost never produces a valid
/// logical operand, so without this the logical differential would only ever
/// exercise the invalid-operand path. One time in eight it falls back to the
/// general generator so the special and invalid paths stay covered too.
fn gen_logical_operand(g: &mut Lcg) -> String {
    if g.below(8) == 0 {
        return gen_operand(g);
    }
    let ndig = 1 + g.below(12);
    let mut s = String::new();
    for _ in 0..ndig {
        s.push(if g.below(2) == 0 { '0' } else { '1' });
    }
    s
}

/// Operand generator for the shift / rotate count: a small integer at exponent
/// zero around the precision boundary (so in-range and just-out-of-range counts
/// are both exercised), occasionally a general value for the invalid path.
fn gen_shift_count(g: &mut Lcg) -> String {
    if g.below(8) == 0 {
        return gen_operand(g);
    }
    format!("{}", g.below(25) as i32 - 12)
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

const OPS: [&str; 44] = [
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
    "exp",
    "ln",
    "log10",
    "power",
    "logical_and",
    "logical_or",
    "logical_xor",
    "logical_invert",
    "shift",
    "rotate",
    "compare_signal",
    "compare_total_mag",
    "max_mag",
    "min_mag",
    "same_quantum",
    "copy",
    "scaleb",
    "logb",
    "next_plus",
    "next_minus",
    "next_toward",
];

/// Whether two finite results are within one unit in the last place. The
/// reference's `power` is only "almost always" correctly rounded, while this
/// crate's is correctly rounded by construction (see `tests/pow_oracle.rs`), so
/// `power` is compared within a one-ulp band rather than cohort-exact.
fn within_one_ulp(got: &Decimal, want_str: &str) -> bool {
    let Ok(want) = Decimal::parse_str(want_str) else {
        return false;
    };
    if !got.is_finite() || !want.is_finite() {
        // No band across the finite/infinite boundary; require an exact match.
        return got.to_string() == want_str;
    }
    let wide = Context::new(60, 1_000_000, -1_000_000, Rounding::HalfEven);
    let absdiff = got.subtract(&want, &wide).0.abs(&wide).0;
    let ge = got.finite_parts().map_or(0, |p| p.2);
    let we = want.finite_parts().map_or(0, |p| p.2);
    let ulp = Decimal::finite(false, DecBig::from_u32(1), ge.max(we));
    let cmp = absdiff.compare(&ulp, &wide).0;
    cmp.is_zero() || cmp.is_negative()
}

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
        // Bias the operands toward each op's valid domain so the differential
        // exercises real results, not only the invalid-operand path.
        let (a, b) = match op {
            "logical_and" | "logical_or" | "logical_xor" | "logical_invert" => {
                (gen_logical_operand(&mut g), gen_logical_operand(&mut g))
            }
            "shift" | "rotate" | "scaleb" => (gen_operand(&mut g), gen_shift_count(&mut g)),
            _ => (gen_operand(&mut g), gen_operand(&mut g)),
        };
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
    let mut power_band = 0;
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
            "compare_total" => (da.compare_total(&db), Status::OK),
            "max" => da.max(&db, &case.ctx),
            "min" => da.min(&db, &case.ctx),
            "copy_abs" => (da.copy_abs(), Status::OK),
            "copy_negate" => (da.copy_negate(), Status::OK),
            "copy_sign" => (da.copy_sign(&db), Status::OK),
            "exp" => da.exp(&case.ctx),
            "ln" => da.ln(&case.ctx),
            "log10" => da.log10(&case.ctx),
            "power" => da.power(&db, &case.ctx),
            "logical_and" => da.and(&db, &case.ctx),
            "logical_or" => da.or(&db, &case.ctx),
            "logical_xor" => da.xor(&db, &case.ctx),
            "logical_invert" => da.invert(&case.ctx),
            "shift" => da.shift(&db, &case.ctx),
            "rotate" => da.rotate(&db, &case.ctx),
            "compare_signal" => da.compare_signal(&db, &case.ctx),
            "compare_total_mag" => (da.compare_total_mag(&db), Status::OK),
            "max_mag" => da.max_magnitude(&db, &case.ctx),
            "min_mag" => da.min_magnitude(&db, &case.ctx),
            "same_quantum" => (da.same_quantum(&db), Status::OK),
            "copy" => (da.copy(), Status::OK),
            "scaleb" => da.scaleb(&db, &case.ctx),
            "logb" => da.logb(&case.ctx),
            "next_plus" => da.next_plus(&case.ctx),
            "next_minus" => da.next_minus(&case.ctx),
            "next_toward" => da.next_toward(&db, &case.ctx),
            _ => da.fma(&db, &dc, &case.ctx),
        };
        let got_str = r.to_string();
        let got_flags = ferrodec_flags(status);
        let hard_ok = got_str == exp_str && got_flags == exp_flags;
        // `power` is allowed a one-ulp band against the reference; everything
        // else is cohort-exact.
        let ok = hard_ok || (case.op == "power" && within_one_ulp(&r, exp_str));
        if !ok {
            mismatches += 1;
            if mismatches <= 30 {
                eprintln!(
                    "MISMATCH {} {} [{}] a={} b={} c={}\n  ferrodec: {got_str:?} [{got_flags}]\n  libmpdec: {exp_str:?} [{exp_flags}]",
                    case.op, case.rnd_name, case.ctx.precision, case.a, case.b, case.c
                );
            }
        } else if case.op == "power" && !hard_ok {
            power_band += 1;
        }
    }
    eprintln!("power one-ulp band cases (correctly rounded, reference is not): {power_band}");
    assert_eq!(
        mismatches, 0,
        "{mismatches} differential mismatches vs libmpdec"
    );
}
