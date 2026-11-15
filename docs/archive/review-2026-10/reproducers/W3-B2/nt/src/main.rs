use num_traits::FromPrimitive;
fn main() {
    let n: i128 = 10i128.pow(20);
    println!("d128 {:?}", <ferrodec::Decimal128 as FromPrimitive>::from_i128(n).map(|d| d.to_string()));
    println!("d64  {:?}", <ferrodec_decimal64::Decimal64 as FromPrimitive>::from_i128(n).map(|d| d.to_string()));
    println!("d32  {:?}", <ferrodec_decimal32::Decimal32 as FromPrimitive>::from_i128(n).map(|d| d.to_string()));
    println!("d64 u128 {:?}", <ferrodec_decimal64::Decimal64 as FromPrimitive>::from_u128(n as u128).map(|d| d.to_string()));
    println!("d64 inherent {:?}", ferrodec_decimal64::Decimal64::from_i128(n, ferrodec_decimal64::RoundingMode::NearestEven));
}
