fn main() {
    let z: ferrodec::Decimal128 = "-0".parse().unwrap();
    let e: ferrodec::Decimal128 = "1E+5".parse().unwrap();
    let s1: ferrodec::Decimal128 = [z].into_iter().sum();
    let s2: ferrodec::Decimal128 = [e].into_iter().sum();
    let s3: ferrodec_decimal32::Decimal32 = ["-0".parse::<ferrodec_decimal32::Decimal32>().unwrap()].into_iter().sum();
    println!("sum([-0])={} sum([1E+5])={} d32 sum([-0])={} f64 sum([-0.0])={}", s1, s2, s3, [-0.0f64].into_iter().sum::<f64>());
}
