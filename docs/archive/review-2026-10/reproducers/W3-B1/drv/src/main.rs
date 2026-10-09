use ferrodec::{Decimal128 as D, RoundingMode as R, Status};
use std::io::{self, BufRead, Write};

fn enc(t: &str) -> D {
    let p: Vec<&str> = t.split(',').collect();
    let s: u128 = if p[1] == "1" { 1u128 << 127 } else { 0 };
    match p[0] {
        "F" => {
            let c: u128 = p[2].parse().unwrap();
            let e: i32 = p[3].parse().unwrap();
            D::from_parts(ferrodec::Decimal128Parts { negative: p[1] == "1", coefficient: c, exponent: e as i16 }).expect("operand range")
        }
        "I" => D::from_bits(s | (0x78u128 << 120)),
        "Q" => D::from_bits(s | (0x7Cu128 << 120) | p[2].parse::<u128>().unwrap()),
        "S" => D::from_bits(s | (0x7Eu128 << 120) | p[2].parse::<u128>().unwrap()),
        "B" => D::from_bits(u128::from_str_radix(p[1], 16).unwrap()),
        _ => panic!("operand {t}"),
    }
}

fn dec(d: D) -> String {
    let b = d.to_bits();
    let s = (b >> 127) & 1;
    let top = (b >> 120) as u8 & 0x7F;
    if top & 0x7C == 0x7C {
        let pl = b & ((1u128 << 110) - 1);
        let k = if top & 0x7E == 0x7E { "S" } else { "Q" };
        return format!("{k},{s},{pl}");
    }
    if top & 0x78 == 0x78 {
        return format!("I,{s}");
    }
    let p = d.decode().unwrap();
    format!("F,{s},{},{}", p.coefficient, p.exponent)
}

fn rm(s: &str) -> R {
    match s {
        "ne" => R::NearestEven,
        "na" => R::NearestAway,
        "tz" => R::TowardZero,
        "tp" => R::TowardPositive,
        "tn" => R::TowardNegative,
        _ => panic!("mode {s}"),
    }
}

fn st(s: Status) -> u8 { s.bits() }

fn main() {
    let stdin = io::stdin();
    let out = io::stdout();
    let mut out = io::BufWriter::new(out.lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.is_empty() { continue; }
        let m = rm(t[1]);
        let a = |i: usize| enc(t[2 + i]);
        let r: String = match t[0] {
            "add" => { let (v, s) = a(0).add(a(1), m); format!("{} {}", dec(v), st(s)) }
            "sub" => { let (v, s) = a(0).sub(a(1), m); format!("{} {}", dec(v), st(s)) }
            "mul" => { let (v, s) = a(0).mul(a(1), m); format!("{} {}", dec(v), st(s)) }
            "div" => { let (v, s) = a(0).div(a(1), m); format!("{} {}", dec(v), st(s)) }
            "fma" => { let (v, s) = a(0).fma(a(1), a(2), m); format!("{} {}", dec(v), st(s)) }
            "sqrt" => { let (v, s) = a(0).sqrt(m); format!("{} {}", dec(v), st(s)) }
            "remnear" => { let (v, s) = a(0).rem_near(a(1)); format!("{} {}", dec(v), st(s)) }
            "remtrunc" => { let (v, s) = a(0).rem_trunc(a(1)); format!("{} {}", dec(v), st(s)) }
            "divint" => { let (v, s) = a(0).divide_integer(a(1)); format!("{} {}", dec(v), st(s)) }
            "quantize" => { let (v, s) = a(0).quantize(a(1), m); format!("{} {}", dec(v), st(s)) }
            "rti" => { let (v, s) = a(0).round_to_integral(m); format!("{} {}", dec(v), st(s)) }
            "rtie" => { let (v, s) = a(0).round_to_integral_exact(m); format!("{} {}", dec(v), st(s)) }
            "reduce" => { let (v, s) = a(0).reduce(); format!("{} {}", dec(v), st(s)) }
            "min" => { let (v, s) = a(0).min(a(1)); format!("{} {}", dec(v), st(s)) }
            "max" => { let (v, s) = a(0).max(a(1)); format!("{} {}", dec(v), st(s)) }
            "minmag" => { let (v, s) = a(0).min_magnitude(a(1)); format!("{} {}", dec(v), st(s)) }
            "maxmag" => { let (v, s) = a(0).max_magnitude(a(1)); format!("{} {}", dec(v), st(s)) }
            "scaleb" => { let n: i32 = t[3].parse().unwrap(); let (v, s) = a(0).scaleb(n, m); format!("{} {}", dec(v), st(s)) }
            "logb" => { let (v, s) = a(0).logb(); format!("{} {}", dec(v), st(s)) }
            "nextup" => { let (v, s) = a(0).next_up(); format!("{} {}", dec(v), st(s)) }
            "nextdown" => { let (v, s) = a(0).next_down(); format!("{} {}", dec(v), st(s)) }
            "nexttoward" => { let (v, s) = a(0).next_toward(a(1)); format!("{} {}", dec(v), st(s)) }
            "totcmp" => format!("{:?}", a(0).total_cmp(a(1))),
            "totmag" => format!("{:?}", a(0).compare_total_magnitude(a(1))),
            "samequantum" => format!("{}", a(0).same_quantum(a(1))),
            "cmp" => { let (o, s) = a(0).partial_cmp(a(1)); format!("{:?} {}", o, st(s)) }
            "cmpsig" => { let (o, s) = a(0).compare_signaling(a(1)); format!("{:?} {}", o, st(s)) }
            "rotate" => { let (v, s) = a(0).rotate(a(1)); format!("{} {}", dec(v), st(s)) }
            "shift" => { let (v, s) = a(0).shift(a(1)); format!("{} {}", dec(v), st(s)) }
            "and" => { let (v, s) = a(0).logical_and(a(1)); format!("{} {}", dec(v), st(s)) }
            "or" => { let (v, s) = a(0).logical_or(a(1)); format!("{} {}", dec(v), st(s)) }
            "xor" => { let (v, s) = a(0).logical_xor(a(1)); format!("{} {}", dec(v), st(s)) }
            "invert" => { let (v, s) = a(0).logical_invert(); format!("{} {}", dec(v), st(s)) }
            "abs" => { let (v, s) = a(0).abs_with_status(); format!("{} {}", dec(v), st(s)) }
            "neg" => { let (v, s) = a(0).neg_with_status(); format!("{} {}", dec(v), st(s)) }
            "floor" => format!("{} 0", dec(a(0).floor())),
            "ceil" => format!("{} 0", dec(a(0).ceil())),
            "trunc" => format!("{} 0", dec(a(0).trunc())),
            "round" => format!("{} 0", dec(a(0).round())),
            "roundeven" => format!("{} 0", dec(a(0).round_ties_even())),
            "bits" => format!("{:032X}", a(0).to_bits()),
            "canon" => format!("{} 0", dec(a(0).canonicalize())),
            "parse" => { let (v, s) = D::parse_str(t[2], m).unwrap(); format!("{} {}", dec(v), st(s)) }
            o => panic!("op {o}"),
        };
        writeln!(out, "{r}").unwrap();
    }
}
