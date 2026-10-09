use ferrodec_test_support::oracle::{self, Dec, Expect, Format, Rounded};
use ferrodec_decimal64::{RoundingMode, Status};
use num_bigint::BigUint;
use std::io::Write;

const MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];
fn mname(m: RoundingMode) -> &'static str {
    match m {
        RoundingMode::NearestEven => "NE",
        RoundingMode::NearestAway => "NA",
        RoundingMode::TowardZero => "TZ",
        RoundingMode::TowardPositive => "TP",
        RoundingMode::TowardNegative => "TN",
        _ => "??",
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn family(p: u32, qmin: i32, qmax: i32) -> Vec<(bool, u64, i32)> {
    let mut cs: Vec<u64> = vec![0, 1, 2, 3, 5, 9, 25, 75];
    for k in 1..p {
        cs.push(10u64.pow(k));
        cs.push(5 * 10u64.pow(k - 1));
        cs.push(10u64.pow(k) + 1);
        cs.push(15 * 10u64.pow(k - 1));
    }
    for k in 1..=p {
        cs.push(10u64.pow(k) - 1);
    }
    cs.push(10u64.pow(p) - 2);
    cs.push(5 * 10u64.pow(p - 1) - 1);
    cs.push(5 * 10u64.pow(p - 1) + 1);
    cs.sort();
    cs.dedup();
    let pi = p as i32;
    let mut es = vec![
        qmin, qmin + 1, qmin + 2, qmin + pi - 1, qmin + pi, qmin + 2 * pi,
        -2 * pi, -pi, -pi + 1, -1, 0, 1, pi - 1, pi, 2 * pi,
        qmax - pi, qmax - 2, qmax - 1, qmax, qmin / 2, qmax / 2,
    ];
    es.sort();
    es.dedup();
    let mut v = vec![];
    for &c in &cs {
        for &e in &es {
            for s in [false, true] {
                v.push((s, c, e));
            }
        }
    }
    v
}

fn to_dec(o: &(bool, u64, i32)) -> Dec {
    Dec::new(o.0, BigUint::from(o.1), o.2)
}

macro_rules! fmt_mod {
    ($modname:ident, $T:ty, $Parts:ident, $C:ty, $FMT:expr, $tag:expr) => {
        mod $modname {
            use super::*;
            pub type T = $T;
            pub fn mk(o: &(bool, u64, i32)) -> T {
                <$T>::from_parts($Parts { negative: o.0, coefficient: o.1 as $C, exponent: o.2 as i16 }).unwrap()
            }
            pub fn show(x: T) -> String {
                if let Some(p) = x.decode() {
                    format!("{}{}E{}", if p.negative { "-" } else { "" }, p.coefficient, p.exponent)
                } else {
                    format!("{}", x)
                }
            }
            pub fn matches(x: T, s: Status, r: &Rounded) -> bool {
                let mask = !Status::CLAMPED.bits();
                if (s.bits() & mask) != (r.status.bits() & mask) {
                    return false;
                }
                match &r.value {
                    Expect::Finite { neg, coeff, exp } => match x.decode() {
                        Some(p) => p.negative == *neg && BigUint::from(p.coefficient as u64) == *coeff && p.exponent as i32 == *exp,
                        None => false,
                    },
                    Expect::Infinity { neg } => x.is_infinite() && x.is_sign_negative() == *neg,
                    Expect::Nan => x.is_nan() && !x.is_signaling_nan(),
                }
            }
            pub fn run(fam: &[(bool, u64, i32)], op: &str, n: usize, seed: u64, log: &mut Vec<String>) -> (usize, usize) {
                let f: Format = $FMT;
                let mut rng = Rng(seed);
                let mut cases = 0usize;
                let mut bad = 0usize;
                for _ in 0..n {
                    let a = fam[rng.below(fam.len())];
                    let b = fam[rng.below(fam.len())];
                    let c = fam[rng.below(fam.len())];
                    let (xa, xb, xc) = (mk(&a), mk(&b), mk(&c));
                    let (da, db, dc) = (to_dec(&a), to_dec(&b), to_dec(&c));
                    for m in MODES {
                        let (got, want): ((T, Status), Rounded) = match op {
                            "add" => (xa.add(xb, m), oracle::add(&da, &db, f, m)),
                            "sub" => (xa.sub(xb, m), oracle::sub(&da, &db, f, m)),
                            "mul" => (xa.mul(xb, m), oracle::mul(&da, &db, f, m)),
                            "div" => { if b.1 == 0 { continue; } (xa.div(xb, m), oracle::div(&da, &db, f, m)) }
                            "fma" => (xa.fma(xb, xc, m), oracle::fma(&da, &db, &dc, f, m)),
                            "sqrt" => { if a.0 && a.1 != 0 { continue; } (xa.sqrt(m), oracle::sqrt(&da, f, m)) }
                            "rem" => { if b.1 == 0 { continue; } if m != RoundingMode::NearestEven { continue; } (xa.rem_near(xb), oracle::rem(&da, &db, f, m)) }
                            _ => unreachable!(),
                        };
                        cases += 1;
                        if !matches(got.0, got.1, &want) {
                            bad += 1;
                            if log.len() < 400 {
                                log.push(format!("{} {} {} a={} b={} c={} got={} {:?} want={} {:?}", $tag, op, mname(m), show(xa), show(xb), show(xc), show(got.0), got.1, want.decimal_string(), want.status));
                            }
                        }
                    }
                }
                (cases, bad)
            }
            // dump quantize / scaleb lines for the Python oracle
            pub fn dump(fam: &[(bool, u64, i32)], n: usize, seed: u64, out: &mut impl Write) {
                let mut rng = Rng(seed);
                for _ in 0..n {
                    let a = fam[rng.below(fam.len())];
                    let b = fam[rng.below(fam.len())];
                    let (xa, xb) = (mk(&a), mk(&b));
                    for m in MODES {
                        let (r, s) = xa.quantize(xb, m);
                        writeln!(out, "{} Q {} {} {} {} {}", $tag, mname(m), show(xa), show(xb), show(r), s.bits()).unwrap();
                    }
                }
                let ns: Vec<i32> = vec![-900, -800, -500, -400, -398, -397, -385, -384, -383, -300, -200, -120, -110, -103, -102, -101, -100, -97, -96, -95, -20, -16, -15, -8, -7, -6, -2, -1, 0, 1, 2, 6, 7, 8, 15, 16, 20, 95, 96, 97, 100, 101, 102, 103, 110, 120, 200, 300, 383, 384, 385, 397, 398, 400, 500, 800, 900];
                for _ in 0..(n / 4) {
                    let a = fam[rng.below(fam.len())];
                    let xa = mk(&a);
                    let k = ns[rng.below(ns.len())];
                    for m in MODES {
                        let (r, s) = xa.scaleb(k, m);
                        writeln!(out, "{} S {} {} {} {} {}", $tag, mname(m), show(xa), k, show(r), s.bits()).unwrap();
                    }
                }
            }
        }
    };
}

use ferrodec_decimal32::{Decimal32, Decimal32Parts};
use ferrodec_decimal64::{Decimal64, Decimal64Parts};
fmt_mod!(f64m, Decimal64, Decimal64Parts, u64, Format::DECIMAL64, "d64");
fmt_mod!(f32m, Decimal32, Decimal32Parts, u32, Format::DECIMAL32, "d32");

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(100000);
    let mode = args.get(2).cloned().unwrap_or_else(|| "arith".into());
    let f64fam = family(16, -398, 369);
    let f32fam = family(7, -101, 90);
    eprintln!("family sizes d64 {} d32 {}", f64fam.len(), f32fam.len());
    if mode == "dump" {
        let stdout = std::io::stdout();
        let mut out = std::io::BufWriter::new(stdout.lock());
        f64m::dump(&f64fam, n, 0x1234_5678_9abc_def1, &mut out);
        f32m::dump(&f32fam, n, 0x0fed_cba9_8765_4321, &mut out);
        return;
    }
    let ops = ["add", "sub", "mul", "div", "fma", "sqrt", "rem"];
    let mut handles = vec![];
    for (i, op) in ops.iter().enumerate() {
        for w in 0..2 {
            let op = op.to_string();
            let fam = if w == 0 { f64fam.clone() } else { f32fam.clone() };
            handles.push(std::thread::spawn(move || {
                let mut log = vec![];
                let seed = 0x9E37_79B9_7F4A_7C15u64 ^ ((i as u64) << 8) ^ (w as u64 + 1);
                let (c, b) = if w == 0 { f64m::run(&fam, &op, n, seed, &mut log) } else { f32m::run(&fam, &op, n, seed, &mut log) };
                (if w == 0 { "d64" } else { "d32" }, op, c, b, log)
            }));
        }
    }
    for h in handles {
        let (t, op, c, b, log) = h.join().unwrap();
        println!("SUMMARY {} {} cases={} mismatches={}", t, op, c, b);
        for l in log {
            println!("{}", l);
        }
    }
}
