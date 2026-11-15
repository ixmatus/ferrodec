use ferrodec::Decimal128;
use ferrodec_transcend::extended::Extended;
use ferrodec_ieee::RoundingMode;
fn main() {
    let mut shifted = 0; let mut total = 0;
    for &coef in &[1i128, 7, 10, 12345, 1_000_000, 10i128.pow(15), 10i128.pow(20), 10i128.pow(33), 10i128.pow(34)-1] {
        for &q in &[-6176, -100, -34, -1, 0, 1, 100, 6111] {
            for &neg in &[false, true] {
                let c = if neg { -coef } else { coef };
                let d = match Decimal128::try_new(c, q) { Ok(d) => d, Err(_) => continue };
                total += 1;
                let e = Extended::from_format::<Decimal128>(d);
                let (back, _) = e.to_format::<Decimal128>(0, RoundingMode::NearestEven);
                if back.to_string() != d.to_string() {
                    shifted += 1;
                    if shifted <= 6 { println!("cohort shift: in {} -> out {}", d, back); }
                }
            }
        }
    }
    println!("{shifted} of {total} round trips change the cohort (value equal by partial_cmp)");
}
