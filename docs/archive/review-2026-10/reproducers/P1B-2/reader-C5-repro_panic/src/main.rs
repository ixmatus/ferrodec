use ferrodec::{Decimal128, RoundingMode};
use ferrodec_decimal64::Decimal64;
use ferrodec_decimal32::Decimal32;
fn p128(s: &str) -> Decimal128 { Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0 }
fn p64(s: &str) -> Decimal64 { Decimal64::parse_str(s, ferrodec_decimal64::RoundingMode::NearestEven).unwrap().0 }
fn p32(s: &str) -> Decimal32 { Decimal32::parse_str(s, ferrodec_decimal32::RoundingMode::NearestEven).unwrap().0 }
fn t<T: std::fmt::Debug>(label: String, f: impl FnOnce() -> T + std::panic::UnwindSafe) {
    match std::panic::catch_unwind(f) { Ok(v) => println!("{label}: ok {v:?}"), Err(_) => println!("{label}: PANICKED") }
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let rm = RoundingMode::NearestEven;
    for s in ["1E100", "-1E100", "1E190", "1E193", "1E200", "-1E200", "7E247", "1E384"] { let v = p64(s); t(format!("d64 asinh({s})"), move || v.asinh(ferrodec_decimal64::RoundingMode::NearestEven)); }
    for s in ["1E40", "1E47", "1E48", "1E50", "-1E60", "7E69", "9E96"] { let v = p32(s); t(format!("d32 asinh({s})"), move || v.asinh(ferrodec_decimal32::RoundingMode::NearestEven)); }
    for s in ["1E3000", "1E3072", "1E3073", "1E3100", "1E6000"] { let v = p128(s); t(format!("d128 asinh({s})"), move || v.asinh(rm)); }
    for (a, b) in [("1E-6000", "1E-6100"), ("1E-6100", "1E-6000"), ("1.58455116102709060557914701823E-6147", "4.61168882725010014976E-6156"), ("1E-6150", "1E-6160"), ("1E-6170", "1E-6176"), ("1", "1E-6176"), ("1E6144", "1E-6176")] { let (x, y) = (p128(a), p128(b)); t(format!("d128 atan2({a}, {b})"), move || x.atan2(y, rm)); }
}
