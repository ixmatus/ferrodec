//! stdin: "<fmt> <func> <x> [<y>|<n>]"; stdout: same line + 5 cells "value#statusbits" in NE NA TZ TP TN order.
use std::io::{self, BufRead, Write};
use std::panic::catch_unwind;
macro_rules! drive {
    ($D:ty, $R:ty, $func:expr, $x:expr, $y:expr) => {{
        let p = |s: &str| <$D>::parse_str(s, <$R>::NearestEven).unwrap().0;
        let modes = [<$R>::NearestEven, <$R>::NearestAway, <$R>::TowardZero, <$R>::TowardPositive, <$R>::TowardNegative];
        let x = p($x);
        let mut out = String::new();
        for rm in modes {
            let ys = $y.clone();
            let f = $func.to_string();
            let r = catch_unwind(move || {
                let yv = ys.as_deref().map(|s: &str| s.to_string());
                let (v, st) = match f.as_str() {
                    "exp" => x.exp(rm), "exp2" => x.exp2(rm), "exp10" => x.exp10(rm),
                    "expm1" => x.exp_m1(rm), "exp2m1" => x.exp2_m1(rm), "exp10m1" => x.exp10_m1(rm),
                    "ln" => x.ln(rm), "log2" => x.log2(rm), "log10" => x.log10(rm),
                    "log1p" => x.ln_1p(rm), "log2p1" => x.log2_1p(rm), "log10p1" => x.log10_1p(rm),
                    "sin" => x.sin(rm), "cos" => x.cos(rm), "tan" => x.tan(rm),
                    "sinpi" => x.sin_pi(rm), "cospi" => x.cos_pi(rm), "tanpi" => x.tan_pi(rm),
                    "asin" => x.asin(rm), "acos" => x.acos(rm), "atan" => x.atan(rm),
                    "asinpi" => x.asin_pi(rm), "acospi" => x.acos_pi(rm), "atanpi" => x.atan_pi(rm),
                    "sinh" => x.sinh(rm), "cosh" => x.cosh(rm), "tanh" => x.tanh(rm),
                    "asinh" => x.asinh(rm), "acosh" => x.acosh(rm), "atanh" => x.atanh(rm),
                    "cbrt" => x.cbrt(rm),
                    "pow" => x.pow(p(&yv.unwrap()), rm), "powr" => x.powr(p(&yv.unwrap()), rm),
                    "atan2" => x.atan2(p(&yv.unwrap()), rm), "atan2pi" => x.atan2_pi(p(&yv.unwrap()), rm),
                    "compound" => x.compound(yv.unwrap().parse::<i32>().unwrap(), rm),
                    other => panic!("unknown {other}"),
                };
                format!("{v}#{}", st.bits())
            });
            out += "\t";
            out += &r.unwrap_or_else(|_| "PANIC#0".to_string());
        }
        out
    }};
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let stdin = io::stdin();
    let mut so = io::BufWriter::new(io::stdout());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 { continue; }
        let y: Option<String> = parts.get(3).map(|s| s.to_string());
        let cells = match parts[0] {
            "d128" => drive!(ferrodec::Decimal128, ferrodec::RoundingMode, parts[1], parts[2], y),
            "d64" => drive!(ferrodec_decimal64::Decimal64, ferrodec_decimal64::RoundingMode, parts[1], parts[2], y),
            "d32" => drive!(ferrodec_decimal32::Decimal32, ferrodec_decimal32::RoundingMode, parts[1], parts[2], y),
            _ => continue,
        };
        writeln!(so, "{line}{cells}").unwrap();
    }
}
