// IEEE 754-2019 §3.5.2 BID: when G0G1 != 11, biased exponent = G0..G(w+1) (the w+2 bits after the sign),
// significand = the remaining bits. decimal128: w+2 = 14, so E occupies bits 126..113.
fn ieee_bid128(neg: bool, c: u128, biased: u128) -> u128 { ((neg as u128) << 127) | (biased << 113) | c }
fn ieee_bid64(neg: bool, c: u64, biased: u64) -> u64 { ((neg as u64) << 63) | (biased << 53) | c }
fn ieee_bid32(neg: bool, c: u32, biased: u32) -> u32 { ((neg as u32) << 31) | (biased << 23) | c }
fn main() {
    use ferrodec::Decimal128;
    for (c, e) in [(1i128, 0i32), (10, 0), (1, 1), (123, -2), (1, -6176), (9999999999999999999999999999999999, 6111)] {
        let d = Decimal128::try_new(c, e).unwrap();
        let want = ieee_bid128(false, c as u128, (e + 6176) as u128);
        println!("d128 {c}E{e}: crate {:032X}  ieee {:032X}  {}", d.to_bits(), want, if d.to_bits() == want { "same" } else { "DIFFER" });
        let back = Decimal128::from_bits(want);
        println!("      crate decodes the IEEE pattern as {:?}", back.decode());
    }
    let d = ferrodec_decimal64::Decimal64::try_new(1, 0).unwrap();
    println!("d64 1E0: crate {:016X} ieee {:016X}", d.to_bits(), ieee_bid64(false, 1, 398));
    let d = ferrodec_decimal32::Decimal32::try_new(1, 0).unwrap();
    println!("d32 1E0: crate {:08X} ieee {:08X}", d.to_bits(), ieee_bid32(false, 1, 101));
}
