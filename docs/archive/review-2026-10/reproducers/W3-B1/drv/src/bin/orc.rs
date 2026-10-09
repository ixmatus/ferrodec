// oracle.rs driver: same line format as b1drv; prints value + status bits (no CLAMPED modelled)
use ferrodec_ieee::RoundingMode as R;
use ferrodec_test_support::oracle::{self, Dec, Format, Expect};
use std::io::{self, BufRead, Write};
fn dec(t: &str) -> Dec {
    let p: Vec<&str> = t.split(',').collect();
    assert_eq!(p[0], "F");
    Dec::new(p[1] == "1", p[2].parse().unwrap(), p[3].parse().unwrap())
}
fn main() {
    let f = Format::DECIMAL128;
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.is_empty() { continue; }
        let m = match t[1] { "ne" => R::NearestEven, "na" => R::NearestAway, "tz" => R::TowardZero, "tp" => R::TowardPositive, _ => R::TowardNegative };
        let a = |i: usize| dec(t[2 + i]);
        let r = match t[0] {
            "add" => oracle::add(&a(0), &a(1), f, m), "sub" => oracle::sub(&a(0), &a(1), f, m),
            "mul" => oracle::mul(&a(0), &a(1), f, m), "div" => oracle::div(&a(0), &a(1), f, m),
            "fma" => oracle::fma(&a(0), &a(1), &a(2), f, m), "sqrt" => oracle::sqrt(&a(0), f, m),
            "remnear" => oracle::rem(&a(0), &a(1), f, m), o => panic!("{o}"),
        };
        let v = match &r.value {
            Expect::Finite { neg, coeff, exp } => format!("F,{},{},{}", *neg as u8, coeff, exp),
            Expect::Infinity { neg } => format!("I,{}", *neg as u8),
            Expect::Nan => "Q,0,0".to_string(),
        };
        writeln!(out, "{} {}", v, r.status.bits()).unwrap();
    }
}
