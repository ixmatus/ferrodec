fn main() {
    use ferrodec_decimal64::{Decimal64, RoundingMode};
    let (n, _) = Decimal64::parse_str("NaN1000000000000000", RoundingMode::NearestEven).unwrap();
    let (r, st) = n.add(Decimal64::ONE, RoundingMode::NearestEven);
    println!("d64 parsed canonical? {} ; NaN1000000000000000 + 1 = {} bits={:016x} canonical? {} st={:?}", n.is_canonical(), r, r.to_bits(), r.is_canonical(), st);
    use ferrodec_decimal32::Decimal32;
    let (n, _) = Decimal32::parse_str("sNaN1048575", ferrodec_decimal32::RoundingMode::NearestEven).unwrap();
    let (r, st) = n.mul(Decimal32::ONE, ferrodec_decimal32::RoundingMode::NearestEven);
    println!("d32 parsed canonical? {} ; sNaN1048575 * 1 = {} bits={:08x} canonical? {} st={:?}", n.is_canonical(), r, r.to_bits(), r.is_canonical(), st);
    let n = ferrodec::Decimal128::from_bits(0x7c003fffffffffffffffffffffffffff);
    let (r, _) = n.add(ferrodec::Decimal128::ONE, ferrodec::RoundingMode::NearestEven);
    println!("d128 from_bits NaN(2^110-1) + 1 = {} canonical? {}", r, r.is_canonical());
}
