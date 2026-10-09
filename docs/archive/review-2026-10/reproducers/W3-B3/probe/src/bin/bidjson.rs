#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct W128 { #[serde(with = "ferrodec::serde_bid")] v: ferrodec::Decimal128 }
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct W64 { #[serde(with = "ferrodec_decimal64::serde_bid")] v: ferrodec_decimal64::Decimal64 }
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct W32 { #[serde(with = "ferrodec_decimal32::serde_bid")] v: ferrodec_decimal32::Decimal32 }
fn main() {
    for s in ["1", "0", "3.14", "NaN"] {
        let a = W128 { v: s.parse().unwrap() }; let j = serde_json::to_string(&a).unwrap(); let b: Result<W128,_> = serde_json::from_str(&j);
        println!("d128 {s}: json={j} back={:?}", b.map(|w| format!("{:032x}", w.v.to_bits())));
        let a = W64 { v: s.parse().unwrap() }; let j = serde_json::to_string(&a).unwrap(); let b: Result<W64,_> = serde_json::from_str(&j);
        println!("d64  {s}: json={j} back={:?}", b.map(|w| format!("{:016x}", w.v.to_bits())));
        let a = W32 { v: s.parse().unwrap() }; let j = serde_json::to_string(&a).unwrap(); let b: Result<W32,_> = serde_json::from_str(&j);
        println!("d32  {s}: json={j} back={:?}", b.map(|w| format!("{:08x}", w.v.to_bits())));
    }
    // negative JSON integer into serde_bid
    let b: Result<W128,_> = serde_json::from_str("{\"v\":-1}"); println!("d128 -1: {:?}", b.map(|w| format!("{:032x} {}", w.v.to_bits(), w.v)));
    let b: Result<W64,_> = serde_json::from_str("{\"v\":-1}"); println!("d64 -1: {:?}", b.map(|w| format!("{:016x} {}", w.v.to_bits(), w.v)));
    let b: Result<W32,_> = serde_json::from_str("{\"v\":-1}"); println!("d32 -1: {:?}", b.map(|w| format!("{:08x} {}", w.v.to_bits(), w.v)));
    // default deserialize of overflow / inexact strings
    let b: Result<ferrodec::Decimal128,_> = serde_json::from_str("\"1E+99999\""); println!("default d128 1E+99999: {:?}", b.map(|d| d.to_string()));
    let b: Result<ferrodec::Decimal128,_> = serde_json::from_str("\"1.00000000000000000000000000000000000001\""); println!("default d128 inexact: {:?}", b.map(|d| d.to_string()));
    let b: Result<ferrodec::Decimal128,_> = serde_json::from_str("3.14"); println!("default d128 json number: {:?}", b.map(|d| d.to_string()));
}
