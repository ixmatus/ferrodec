// Reads lines "<d64|d32> <fn> <x>" and prints the result in all five modes.
use ferrodec_decimal64::RoundingMode;
use std::io::BufRead;
const MODES: [(RoundingMode, &str); 5] = [(RoundingMode::NearestEven, "NE"), (RoundingMode::NearestAway, "NA"), (RoundingMode::TowardZero, "TZ"), (RoundingMode::TowardPositive, "TP"), (RoundingMode::TowardNegative, "TN")];
macro_rules! run { ($T:ty, $f:expr, $x:expr, $line:expr) => {{
    let (x, st) = <$T>::parse_str($x, RoundingMode::NearestEven).unwrap(); assert!(st.is_ok());
    for (m, mn) in MODES {
        let (r, s) = match $f { "exp" => x.exp(m), "exp10" => x.exp10(m), "exp2" => x.exp2(m), "sinh" => x.sinh(m), "cosh" => x.cosh(m), "expm1" => x.exp_m1(m), _ => unreachable!() };
        let d = r.decode();
        let v = match d { Some(p) => format!("{}{}E{}", if p.negative {"-"} else {""}, p.coefficient, p.exponent), None => format!("{}", r) };
        println!("{} {} {} {}", $line, mn, v, s.bits());
    }
}}}
fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap(); let p: Vec<&str> = line.split_whitespace().collect();
        match p[0] { "d64" => run!(ferrodec_decimal64::Decimal64, p[1], p[2], line), "d32" => run!(ferrodec_decimal32::Decimal32, p[1], p[2], line), _ => {} }
    }
}
