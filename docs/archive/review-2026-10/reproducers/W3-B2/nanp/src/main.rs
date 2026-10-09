use ferrodec_decimal64::RoundingMode;
const OPS: [&str; 17] = ["NaN12", "-NaN7", "sNaN3", "-sNaN5", "NaN", "0", "-0", "0E+3", "-0E-2", "Infinity", "-Infinity", "1", "-1", "2", "0.5", "-2.5", "10"];
const NS: [i32; 9] = [-3, -2, -1, 0, 1, 2, 3, i32::MIN, i32::MAX];
const RM: RoundingMode = RoundingMode::NearestEven;

macro_rules! probe {
    ($T:ty, $tag:expr) => {{
        let p = |s: &str| <$T>::parse_str(s, RM).unwrap().0;
        let v: Vec<$T> = OPS.iter().map(|s| p(s)).collect();
        macro_rules! bin { ($name:ident) => {
            for (i, x) in v.iter().enumerate() { for (j, y) in v.iter().enumerate() {
                let (r, s) = x.$name(*y, RM);
                println!("{} {} {} {} -> {} {}", $tag, stringify!($name), OPS[i], OPS[j], r, s.bits());
            }}
        }}
        macro_rules! bin0 { ($name:ident) => {
            for (i, x) in v.iter().enumerate() { for (j, y) in v.iter().enumerate() {
                let (r, s) = x.$name(*y);
                println!("{} {} {} {} -> {} {}", $tag, stringify!($name), OPS[i], OPS[j], r, s.bits());
            }}
        }}
        macro_rules! cmp0 { ($name:ident) => {
            for (i, x) in v.iter().enumerate() { for (j, y) in v.iter().enumerate() {
                let (r, s) = x.$name(*y);
                println!("{} {} {} {} -> {:?} {}", $tag, stringify!($name), OPS[i], OPS[j], r, s.bits());
            }}
        }}
        macro_rules! un { ($name:ident) => {
            for (i, x) in v.iter().enumerate() {
                let (r, s) = x.$name(RM);
                println!("{} {} {} _ -> {} {}", $tag, stringify!($name), OPS[i], r, s.bits());
            }
        }}
        macro_rules! un0 { ($name:ident) => {
            for (i, x) in v.iter().enumerate() {
                let (r, s) = x.$name();
                println!("{} {} {} _ -> {} {}", $tag, stringify!($name), OPS[i], r, s.bits());
            }
        }}
        macro_rules! inti { ($name:ident) => {
            for (i, x) in v.iter().enumerate() { for n in NS {
                let (r, s) = x.$name(n, RM);
                println!("{} {} {} {} -> {} {}", $tag, stringify!($name), OPS[i], n, r, s.bits());
            }}
        }}
        bin!(add); bin!(sub); bin!(mul); bin!(div); bin!(quantize);
        bin!(pow); bin!(powr); bin!(atan2); bin!(atan2_pi); bin!(hypot);
        bin0!(rem_near); bin0!(rem_trunc); bin0!(divide_integer); bin0!(min); bin0!(max);
        bin0!(min_magnitude); bin0!(max_magnitude); bin0!(next_toward); bin0!(logical_and);
        bin0!(logical_or); bin0!(logical_xor); bin0!(shift); bin0!(rotate);
        cmp0!(partial_cmp); cmp0!(compare_signaling);
        for (i, x) in v.iter().enumerate() { for (j, y) in v.iter().enumerate() {
            println!("{} same_quantum {} {} -> {} 0", $tag, OPS[i], OPS[j], x.same_quantum(*y));
            println!("{} total_cmp {} {} -> {:?} 0", $tag, OPS[i], OPS[j], x.total_cmp(*y));
            println!("{} compare_total_magnitude {} {} -> {:?} 0", $tag, OPS[i], OPS[j], x.compare_total_magnitude(*y));
            for (k, z) in v.iter().enumerate() {
                let (r, s) = x.fma(*y, *z, RM);
                println!("{} fma {} {},{} -> {} {}", $tag, OPS[i], OPS[j], OPS[k], r, s.bits());
            }
        }}
        inti!(scaleb); inti!(compound); inti!(rootn); inti!(powi);
        un!(exp); un!(ln); un!(exp2); un!(log2); un!(log10); un!(exp10); un!(exp_m1); un!(ln_1p);
        un!(exp2_m1); un!(exp10_m1); un!(log2_1p); un!(log10_1p); un!(sqrt); un!(cbrt); un!(rsqrt);
        un!(sin); un!(cos); un!(tan); un!(asin); un!(acos); un!(atan);
        un!(sinh); un!(cosh); un!(tanh); un!(asinh); un!(acosh); un!(atanh);
        un!(sin_pi); un!(cos_pi); un!(tan_pi); un!(asin_pi); un!(acos_pi); un!(atan_pi);
        un!(round_to_integral); un!(round_to_integral_exact);
        un0!(logb); un0!(reduce); un0!(next_up); un0!(next_down); un0!(abs_with_status); un0!(neg_with_status); un0!(logical_invert);
    }};
}

fn main() {
    let w = std::env::args().nth(1).unwrap();
    match w.as_str() {
        "128" => probe!(ferrodec::Decimal128, "F"),
        "64" => probe!(ferrodec_decimal64::Decimal64, "F"),
        "32" => probe!(ferrodec_decimal32::Decimal32, "F"),
        _ => {}
    }
}
