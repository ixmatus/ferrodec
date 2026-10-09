use ferrodec::Decimal128;
use ferrodec_decimal32::Decimal32;
use ferrodec_decimal64::Decimal64;
use ferrodec_ieee::RoundingMode as RM;
use ferrodec_multiword::U256;
use ferrodec_transcend::extended::Extended;
use std::io::BufRead;

fn u256_from_dec(s: &str) -> U256 {
    let mut c = U256::ZERO;
    for b in s.bytes() { c = c.mul10().add(U256::from_u128((b - b'0') as u128)); }
    c
}
fn u256_to_dec(mut c: U256) -> String {
    if c.is_zero() { return "0".into(); }
    let mut v = Vec::new();
    while !c.is_zero() { let (q, r) = c.div_rem10(); v.push(b'0' + r as u8); c = q; }
    v.reverse(); String::from_utf8(v).unwrap()
}
fn show(e: Extended) -> String {
    format!("{}{}E{}", if e.sign { "-" } else { "" }, u256_to_dec(e.coef), e.exp)
}
fn ext(coef: &str, exp: i32, sign: bool) -> Extended {
    Extended { coef: u256_from_dec(coef), exp, sign }
}

fn main() {
    // stdin protocol: one command per line
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.is_empty() { continue; }
        let out = match t[0] {
            // pred FMT COEF EXP BUDGET  -> near?
            "pred" => {
                let e = ext(t[2], t[3].parse().unwrap(), false);
                let b: u128 = t[4].parse().unwrap();
                let r = match t[1] {
                    "d128" => e.near_rounding_boundary::<Decimal128>(b),
                    "d64" => e.near_rounding_boundary::<Decimal64>(b),
                    _ => e.near_rounding_boundary::<Decimal32>(b),
                };
                format!("{}", r)
            }
            // exp X(d128 string) -> rung1 exp_extended
            "exp" => {
                let x = Decimal128::parse_str(t[1], RM::NearestEven).unwrap().0;
                show(ferrodec_transcend::exp::exp_extended(Extended::from_format(x)))
            }
            // recip FMT X  -> rung1 recip of format value x
            "recip" | "sqrt" => {
                let r = match t[1] {
                    "d128" => { let x = Decimal128::parse_str(t[2], RM::NearestEven).unwrap().0; let e = Extended::from_format(x); if t[0]=="recip" { e.recip::<Decimal128>() } else { e.sqrt::<Decimal128>() } }
                    "d64" => { let x = Decimal64::parse_str(t[2], RM::NearestEven).unwrap().0; let e = Extended::from_format(x); if t[0]=="recip" { e.recip::<Decimal64>() } else { e.sqrt::<Decimal64>() } }
                    _ => { let x = Decimal32::parse_str(t[2], RM::NearestEven).unwrap().0; let e = Extended::from_format(x); if t[0]=="recip" { e.recip::<Decimal32>() } else { e.sqrt::<Decimal32>() } }
                };
                show(r)
            }
            // erecip FMT COEF EXP : recip of an arbitrary 50-digit Extended
            "erecip" => {
                let e = ext(t[2], t[3].parse().unwrap(), false);
                let r = match t[1] { "d128" => e.recip::<Decimal128>(), "d64" => e.recip::<Decimal64>(), _ => e.recip::<Decimal32>() };
                show(r)
            }
            "esqrt" => {
                let e = ext(t[2], t[3].parse().unwrap(), false);
                let r = match t[1] { "d128" => e.sqrt::<Decimal128>(), "d64" => e.sqrt::<Decimal64>(), _ => e.sqrt::<Decimal32>() };
                show(r)
            }
            // a2 FMT Y X MODE -> atan2
            "a2" => {
                let rm = match t[4] { "NE" => RM::NearestEven, "NA" => RM::NearestAway, "TP" => RM::TowardPositive, "TN" => RM::TowardNegative, _ => RM::TowardZero };
                match t[1] {
                    "d32" => { let y = Decimal32::parse_str(t[2], RM::NearestEven).unwrap().0; let x = Decimal32::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = y.atan2(x, rm); format!("{} {:02x}", v, s.bits()) }
                    "d64" => { let y = Decimal64::parse_str(t[2], RM::NearestEven).unwrap().0; let x = Decimal64::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = y.atan2(x, rm); format!("{} {:02x}", v, s.bits()) }
                    _ => { let y = Decimal128::parse_str(t[2], RM::NearestEven).unwrap().0; let x = Decimal128::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = y.atan2(x, rm); format!("{} {:02x}", v, s.bits()) }
                }
            }
            "ediv64" => {
                let y = Decimal64::parse_str(t[1], RM::NearestEven).unwrap().0; let x = Decimal64::parse_str(t[2], RM::NearestEven).unwrap().0;
                let r = Extended::from_format(y).div::<Decimal64>(Extended::from_format(x));
                format!("{} recip={}", show(r), show(Extended::from_format(x).recip::<Decimal64>()))
            }
            "pstr" => show(Extended::parse_str(t[1])),
            // sincos FMT X -> sin, cos ext (rung 1 mirror)
            "sincos" => {
                let x = Decimal128::parse_str(t[1], RM::NearestEven).unwrap().0;
                let (s, c, _) = ferrodec_transcend::sincos::sincos_extended::<Decimal128>(x);
                format!("{} {}", show(s), show(c))
            }
            // f FMT NAME X MODE -> public result
            "f" => {
                let rm = match t[4] { "NE" => RM::NearestEven, "NA" => RM::NearestAway, "TP" => RM::TowardPositive, "TN" => RM::TowardNegative, _ => RM::TowardZero };
                match t[1] {
                    "d32" => { let x = Decimal32::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = call32(t[2], x, rm); format!("{} {:02x}", v, s.bits()) }
                    "d64" => { let x = Decimal64::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = call64(t[2], x, rm); format!("{} {:02x}", v, s.bits()) }
                    _ => { let x = Decimal128::parse_str(t[3], RM::NearestEven).unwrap().0; let (v, s) = call128(t[2], x, rm); format!("{} {:02x}", v, s.bits()) }
                }
            }
            _ => "?".into(),
        };
        println!("{}", out);
    }
}

macro_rules! callf { ($name:ident, $T:ty) => {
fn $name(n: &str, x: $T, rm: RM) -> ($T, ferrodec_ieee::Status) {
    match n {
        "sin" => x.sin(rm), "cos" => x.cos(rm), "tan" => x.tan(rm), "atan" => x.atan(rm),
        "asin" => x.asin(rm), "acos" => x.acos(rm), "exp" => x.exp(rm), "ln" => x.ln(rm),
        "sinh" => x.sinh(rm), "cosh" => x.cosh(rm), "tanh" => x.tanh(rm),
        "asinh" => x.asinh(rm), "acosh" => x.acosh(rm), "atanh" => x.atanh(rm),
        "cbrt" => x.cbrt(rm), "rsqrt" => x.rsqrt(rm),
        _ => panic!("fn"),
    }
}}}
callf!(call128, Decimal128);
callf!(call64, Decimal64);
callf!(call32, Decimal32);
// (appended) no-op
