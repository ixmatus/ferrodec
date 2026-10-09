fn main() {
    use ferrodec_decimal64 as a; use ferrodec_decimal32 as b;
    println!("d64 {} {} {} {}", a::pi(), a::e(), a::ln2(), a::ln10());
    println!("d32 {} {} {} {}", b::pi(), b::e(), b::ln2(), b::ln10());
}
