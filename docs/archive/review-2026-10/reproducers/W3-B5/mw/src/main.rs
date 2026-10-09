use ferrodec_multiword::{u1024::u768_mul_u128_to_u1024, u768::u384_mul_u384_to_u768, DecBig, U1024, U256, U384, U512, U768};
use std::io::{BufRead, Write};

fn parse<const N: usize>(h: &str) -> [u128; N] {
    let h = h.trim_start_matches("0x");
    let mut out = [0u128; N];
    let bytes = h.as_bytes();
    let mut end = bytes.len();
    let mut i = 0;
    while end > 0 {
        let start = end.saturating_sub(32);
        let s = std::str::from_utf8(&bytes[start..end]).unwrap();
        out[i] = u128::from_str_radix(s, 16).unwrap();
        i += 1;
        end = start;
    }
    out
}
fn hx(l: &[u128]) -> String {
    let mut s = String::from("0x");
    for x in l.iter().rev() { s.push_str(&format!("{:032x}", x)); }
    s
}
fn u256(h: &str) -> U256 { let l = parse::<2>(h); U256 { lo: l[0], hi: l[1] } }
fn u384(h: &str) -> U384 { let l = parse::<3>(h); U384 { lo: l[0], mid: l[1], hi: l[2] } }
fn u512(h: &str) -> U512 { let l = parse::<4>(h); U512 { lo: l[0], mid_lo: l[1], mid_hi: l[2], hi: l[3] } }
fn u768(h: &str) -> U768 { U768 { limbs: parse::<6>(h) } }
fn u1024(h: &str) -> U1024 { U1024 { limbs: parse::<8>(h) } }
fn h256(x: U256) -> String { hx(&[x.lo, x.hi]) }
fn h384(x: U384) -> String { hx(&[x.lo, x.mid, x.hi]) }
fn h512(x: U512) -> String { hx(&[x.lo, x.mid_lo, x.mid_hi, x.hi]) }
fn ord(o: std::cmp::Ordering) -> i32 { o as i32 }
fn db(s: &str) -> DecBig { DecBig::from_ascii_digits(s.as_bytes()) }

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        let r: String = match f[0] {
            "256add" => h256(u256(f[1]).add(u256(f[2]))),
            "256sub" => h256(u256(f[1]).sub(u256(f[2]))),
            "256cmp" => ord(u256(f[1]).cmp(u256(f[2]))).to_string(),
            "256mulu" => h256(u256(f[1]).mul_u128(parse::<1>(f[2])[0])),
            "256mul10" => h256(u256(f[1]).mul10()),
            "256mulp" => h256(u256(f[1]).mul_pow10(f[2].parse().unwrap())),
            "256dr10" => { let (q, r) = u256(f[1]).div_rem10(); format!("{} {}", h256(q), r) }
            "256divu" => { let (q, r) = u256(f[1]).div_rem_u128(parse::<1>(f[2])[0]); format!("{} {}", h256(q), hx(&[r])) }
            "256isqrt" => { let (s, r) = u256(f[1]).isqrt(); format!("{} {}", hx(&[s]), h256(r)) }
            "256dig" => u256(f[1]).decimal_digit_count().to_string(),
            "wmul" => { let (h, l) = ferrodec_multiword::u256::widening_mul_u128(parse::<1>(f[1])[0], parse::<1>(f[2])[0]); hx(&[l, h]) }
            "384add" => h384(u384(f[1]).add(u384(f[2]))),
            "384sub" => h384(u384(f[1]).sub(u384(f[2]))),
            "384cmp" => ord(u384(f[1]).cmp(u384(f[2]))).to_string(),
            "384mul10" => h384(u384(f[1]).mul10()),
            "384mulp" => h384(u384(f[1]).mul_pow10(f[2].parse().unwrap())),
            "384dr10" => { let (q, r) = u384(f[1]).div_rem10(); format!("{} {}", h384(q), r) }
            "384divu" => { let (q, r) = u384(f[1]).div_rem_u128(parse::<1>(f[2])[0]); format!("{} {}", h384(q), hx(&[r])) }
            "384dig" => u384(f[1]).decimal_digit_count().to_string(),
            "384shr" => { let (v, s, st) = u384(f[1]).shift_right_to_u256(f[2] == "1"); format!("{} {} {}", h256(v), s, st as u8) }
            "512add" => h512(u512(f[1]).add(u512(f[2]))),
            "512sub" => h512(u512(f[1]).sub(u512(f[2]))),
            "512cmp" => ord(u512(f[1]).cmp(u512(f[2]))).to_string(),
            "512mul10" => h512(u512(f[1]).mul10()),
            "512dr10" => { let (q, r) = u512(f[1]).div_rem10(); format!("{} {}", h512(q), r) }
            "512dig" => u512(f[1]).decimal_digit_count().to_string(),
            "512shr" => { let (v, s, st) = u512(f[1]).shift_right_to_u256(f[2] == "1"); format!("{} {} {}", h256(v), s, st as u8) }
            "768add" => hx(&u768(f[1]).add(u768(f[2])).limbs),
            "768sub" => hx(&u768(f[1]).sub(u768(f[2])).limbs),
            "768cmp" => ord(u768(f[1]).cmp(u768(f[2]))).to_string(),
            "768mulu" => hx(&u768(f[1]).mul_u128(parse::<1>(f[2])[0]).limbs),
            "768dr10" => { let (q, r) = u768(f[1]).div_rem10(); format!("{} {}", hx(&q.limbs), r) }
            "768dig" => u768(f[1]).decimal_digit_count().to_string(),
            "768shr" => { let (v, s, st) = u768(f[1]).shift_right_to_u384(f[2] == "1"); format!("{} {} {}", h384(v), s, st as u8) }
            "768x" => hx(&u384_mul_u384_to_u768(u384(f[1]), u384(f[2])).limbs),
            "1024add" => hx(&u1024(f[1]).add(u1024(f[2])).limbs),
            "1024sub" => hx(&u1024(f[1]).sub(u1024(f[2])).limbs),
            "1024cmp" => ord(u1024(f[1]).cmp(u1024(f[2]))).to_string(),
            "1024mulu" => hx(&u1024(f[1]).mul_u128(parse::<1>(f[2])[0]).limbs),
            "1024cm" => match u1024(f[1]).checked_mul(u1024(f[2])) { Some(v) => hx(&v.limbs), None => "None".into() },
            "1024x" => hx(&u768_mul_u128_to_u1024(u768(f[1]), parse::<1>(f[2])[0]).limbs),
            "1024dr10" => { let (q, r) = u1024(f[1]).div_rem10(); format!("{} {}", hx(&q.limbs), r) }
            "1024dig" => u1024(f[1]).decimal_digit_count().to_string(),
            "dbadd" => db(f[1]).add(&db(f[2])).to_string(),
            "dbsub" => db(f[1]).sub(&db(f[2])).to_string(),
            "dbmul" => db(f[1]).mul(&db(f[2])).to_string(),
            "dbcmp" => ord(db(f[1]).cmp_ref(&db(f[2]))).to_string(),
            "dbdiv" => { let (q, r) = db(f[1]).div_rem(&db(f[2])); format!("{} {}", q, r) }
            "dbmulp" => db(f[1]).mul_pow10(f[2].parse().unwrap()).to_string(),
            "dbdrp" => { let (q, r) = db(f[1]).div_rem_pow10(f[2].parse().unwrap()); format!("{} {}", q, r) }
            "dbpow10" => DecBig::pow10(f[1].parse().unwrap()).to_string(),
            "dbisqrt" => { let (s, r) = db(f[1]).isqrt(); format!("{} {}", s, r) }
            "dbdig" => db(f[1]).decimal_digit_count().to_string(),
            "dbu128" => match db(f[1]).to_u128() { Some(v) => v.to_string(), None => "None".into() },
            "dbdr10" => { let (q, r) = db(f[1]).div_rem10(); format!("{} {}", q, r) }
            _ => panic!("unknown op {}", f[0]),
        };
        writeln!(out, "{}", r).unwrap();
    }
}
