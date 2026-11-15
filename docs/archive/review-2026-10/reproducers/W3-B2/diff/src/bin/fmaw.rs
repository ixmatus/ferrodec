// Usage: fmaw <d64|d32> <NE|NA|TZ|TP|TN> a b c   (operands parsed exactly via parse_str)
use ferrodec_decimal64::RoundingMode;
fn rm(s: &str) -> RoundingMode {
    match s { "NE" => RoundingMode::NearestEven, "NA" => RoundingMode::NearestAway, "TZ" => RoundingMode::TowardZero, "TP" => RoundingMode::TowardPositive, _ => RoundingMode::TowardNegative }
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let m = rm(&a[2]);
    match a[1].as_str() {
        "d64" => { use ferrodec_decimal64::Decimal64 as T; let p = |s: &str| { let (v, st) = T::parse_str(s, m).unwrap(); assert!(st.is_ok(), "inexact parse {s}"); v };
            let (r, s) = p(&a[3]).fma(p(&a[4]), p(&a[5]), m); println!("{} {:?}", r, s); }
        "d32" => { use ferrodec_decimal32::Decimal32 as T; let p = |s: &str| { let (v, st) = T::parse_str(s, m).unwrap(); assert!(st.is_ok(), "inexact parse {s}"); v };
            let (r, s) = p(&a[3]).fma(p(&a[4]), p(&a[5]), m); println!("{} {:?}", r, s); }
        _ => {}
    }
}
