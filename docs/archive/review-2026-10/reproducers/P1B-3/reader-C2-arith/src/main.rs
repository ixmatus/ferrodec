//! Differential: ferrodec {d128,d64,d32} add/sub/mul/div/sqrt/fma/rem_near vs the exact oracle.
//! Operands come in as decTest-style strings; parsed exactly (they are in-format).
use std::io::{self, BufRead};
use ferrodec_ieee::RoundingMode as IRM;
use ferrodec_test_support::oracle::{self, Format, parse_decimal};
use ferrodec_test_support::conformance::status_conformance_eq;

macro_rules! run {
    ($T:ty, $RM:ty, $fmt:expr, $rm:expr, $op:expr, $args:expr, $irm:expr) => {{
        let rm: $RM = $rm;
        let v: Vec<$T> = $args.iter().map(|s: &&str| <$T>::parse_str(s, rm).unwrap().0).collect();
        let d: Vec<_> = $args.iter().map(|s: &&str| parse_decimal(s).unwrap()).collect();
        let (got, gs) = match $op {
            "add" => v[0].add(v[1], rm), "sub" => v[0].sub(v[1], rm), "mul" => v[0].mul(v[1], rm),
            "div" => v[0].div(v[1], rm), "sqrt" => v[0].sqrt(rm), "fma" => v[0].fma(v[1], v[2], rm),
            "rem" => v[0].rem_near(v[1]),
            _ => panic!(),
        };
        let r = match $op {
            "add" => oracle::add(&d[0], &d[1], $fmt, $irm), "sub" => oracle::sub(&d[0], &d[1], $fmt, $irm),
            "mul" => oracle::mul(&d[0], &d[1], $fmt, $irm), "div" => oracle::div(&d[0], &d[1], $fmt, $irm),
            "sqrt" => oracle::sqrt(&d[0], $fmt, $irm), "fma" => oracle::fma(&d[0], &d[1], &d[2], $fmt, $irm),
            "rem" => oracle::rem(&d[0], &d[1], $fmt, $irm), _ => panic!(),
        };
        let want = <$T>::parse_str(&r.decimal_string(), rm).unwrap().0;
        let ok_v = if want.is_nan() { got.is_nan() } else { got.to_bits() == want.to_bits() };
        let gs_i = ferrodec_ieee::Status::from_bits_truncate(gs.bits());
        let ok_s = status_conformance_eq(gs_i, r.status);
        (ok_v && ok_s, format!("got {} {:?} want {} {:?}", got, gs, r.decimal_string(), r.status))
    }};
}

fn main() {
    let (mut n, mut bad) = (0u64, 0u64);
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.is_empty() { continue; }
        let irm = match t[1] { "ne" => IRM::NearestEven, "na" => IRM::NearestAway, "tz" => IRM::TowardZero, "tp" => IRM::TowardPositive, _ => IRM::TowardNegative };
        let op = t[2]; let args = &t[3..];
        if op == "rem" && t[0] != "128" { /* rem_near exists on siblings too */ }
        let (ok, msg) = match t[0] {
            "128" => { use ferrodec::{Decimal128 as T, RoundingMode as R};
                let rm = match irm { IRM::NearestEven => R::NearestEven, IRM::NearestAway => R::NearestAway, IRM::TowardZero => R::TowardZero, IRM::TowardPositive => R::TowardPositive, IRM::TowardNegative => R::TowardNegative };
                run!(T, R, Format::DECIMAL128, rm, op, args, irm) }
            "64" => { use ferrodec_decimal64::{Decimal64 as T, RoundingMode as R};
                let rm = match irm { IRM::NearestEven => R::NearestEven, IRM::NearestAway => R::NearestAway, IRM::TowardZero => R::TowardZero, IRM::TowardPositive => R::TowardPositive, IRM::TowardNegative => R::TowardNegative };
                run!(T, R, Format::DECIMAL64, rm, op, args, irm) }
            _ => { use ferrodec_decimal32::{Decimal32 as T, RoundingMode as R};
                let rm = match irm { IRM::NearestEven => R::NearestEven, IRM::NearestAway => R::NearestAway, IRM::TowardZero => R::TowardZero, IRM::TowardPositive => R::TowardPositive, IRM::TowardNegative => R::TowardNegative };
                run!(T, R, Format::DECIMAL32, rm, op, args, irm) }
        };
        n += 1;
        if !ok { bad += 1; println!("MISMATCH {line}: {msg}"); }
    }
    println!("cases {n}, mismatches {bad}");
}
