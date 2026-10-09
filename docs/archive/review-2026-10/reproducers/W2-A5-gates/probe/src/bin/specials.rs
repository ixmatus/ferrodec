use ferrodec::{Decimal128 as D, RoundingMode as R};
fn p(s: &str) -> D { D::parse_str(s, R::NearestEven).unwrap().0 }
macro_rules! un { ($($f:ident),*) => { vec![$((stringify!($f), (|x: D| x.$f(R::NearestEven)) as fn(D)->(D, ferrodec::Status))),*] } }
fn main() {
    let ins = ["sNaN45", "-sNaN46", "-NaN12", "0", "-0", "Inf", "-Inf", "1", "-1"];
    let fs = un!(exp, exp2, exp10, exp_m1, exp2_m1, exp10_m1, ln, log2, log10, ln_1p, log2_1p, log10_1p, sin, cos, tan, sin_pi, cos_pi, tan_pi, asin, acos, atan, asin_pi, acos_pi, atan_pi, sinh, cosh, tanh, asinh, acosh, atanh, cbrt, rsqrt);
    for (name, f) in &fs {
        let mut line = format!("{name:10}");
        for s in ins { let (r, st) = f(p(s)); line += &format!(" | {s}->{r} {:?}", st); }
        println!("{line}");
    }
    let bins: Vec<(&str, fn(D, D) -> (D, ferrodec::Status))> = vec![
        ("pow", |a, b| a.pow(b, R::NearestEven)), ("powr", |a, b| a.powr(b, R::NearestEven)),
        ("atan2", |a, b| a.atan2(b, R::NearestEven)), ("atan2_pi", |a, b| a.atan2_pi(b, R::NearestEven)),
        ("hypot", |a, b| a.hypot(b, R::NearestEven))];
    let bin_ins = ["sNaN45", "-NaN12", "0", "-0", "Inf", "-Inf", "1", "-1", "2", "-2", "0.5", "3"];
    for (name, f) in &bins {
        for a in bin_ins { for b in bin_ins {
            let (r, st) = f(p(a), p(b)); println!("{name}({a}, {b}) = {r} {st:?}");
        } }
    }
    for (x, n) in [("sNaN45", 0), ("-NaN12", 0), ("-NaN12", 2), ("0", -3), ("-0", -3), ("-0", 3), ("-Inf", -3), ("-Inf", 3)] {
        let (r, st) = p(x).powi(n, R::NearestEven); println!("powi({x}, {n}) = {r} {st:?}");
        let (r, st) = p(x).rootn(n, R::NearestEven); println!("rootn({x}, {n}) = {r} {st:?}");
    }
    for (x, n) in [("sNaN45", 0), ("-NaN12", 0), ("0", 0), ("Inf", 0), ("-1", 0), ("-1", 3), ("-1", -3), ("-2", 1), ("-0.5", 0), ("Inf", -1), ("-0", 3)] {
        let (r, st) = p(x).compound(n, R::NearestEven); println!("compound({x}, {n}) = {r} {st:?}");
    }
}
