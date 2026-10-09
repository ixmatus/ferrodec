fn main() {
    for s in ["12345", "123456", "0.012345", "98765E-20"] {
        let a: ferrodec::Decimal128 = s.parse().unwrap();
        let b: ferrodec_decimal64::Decimal64 = s.parse().unwrap();
        let c: ferrodec_decimal32::Decimal32 = s.parse().unwrap();
        println!("{s:>10}: d128 {{:.2}} eng={} | d64={} | d32={} | {{:.2e}}={} | {{:>12}}=[{:>12}] {{:+}}=[{:+}]", format!("{:.2}", a.engineering()), format!("{:.2}", b.engineering()), format!("{:.2}", c.engineering()), format!("{:.2e}", a), a, a);
    }
}
