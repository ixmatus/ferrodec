use ferrodec_multiword::{DecBig, U256};
fn main() {
    let x = U256 { lo: 0, hi: 1u128 << 127 }; // 2^255, bit_len 256 -> half_bits 128
    let r = std::panic::catch_unwind(|| x.isqrt());
    println!("U256::isqrt(2^255) -> {:?}", r.map(|(s, rem)| (s, rem)));
    let r = std::panic::catch_unwind(|| U256 { lo: 5, hi: 1 }.div_rem_u128(0));
    println!("U256::div_rem_u128(2^128+5, 0) -> {:?}", r);
    let r = std::panic::catch_unwind(|| DecBig::from_u32(5).sub(&DecBig::from_u32(7)));
    println!("DecBig 5 - 7 -> {:?}", r.map(|v| v.to_string()));
    let r = std::panic::catch_unwind(|| ferrodec_ieee::decimal_is_binary_exact((1u128 << 126) + 1, 1, 127));
    println!("decimal_is_binary_exact(2^126+1, 1, 127) -> {:?}", r);
}
