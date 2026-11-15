use ferrodec_decimal::*;
fn main(){
  let a: Vec<String> = std::env::args().collect();
  let d: Decimal = a[2].parse().unwrap();
  let rm = match a.get(3).map(|s| s.as_str()).unwrap_or("ne") { "tz"=>RoundingMode::TowardZero, "tn"=>RoundingMode::TowardNegative, "tp"=>RoundingMode::TowardPositive, _=>RoundingMode::NearestEven };
  match a[1].as_str() {
    "f64" => { let (v,s)=d.to_f64(rm); println!("{:?} bits={:#x} {:?}", v, v.to_bits(), s); }
    "d128" => { let (v,s)=d.to_decimal128(rm); println!("{} {:?}", v, s); }
    "d64" => { let (v,s)=d.to_decimal64(rm); println!("{} {:?}", v, s); }
    "d32" => { let (v,s)=d.to_decimal32(rm); println!("{} {:?}", v, s); }
    _ => {}
  }
}
