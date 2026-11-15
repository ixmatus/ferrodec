use core::str::FromStr;
use ferrodec::{Decimal128, RoundingMode};
fn main() {
    for s in ["1E-310", "4.9E-324", "2.2250738585072011E-308", "2.2250738585072014E-308", "1E-400", "1", "0.1", "1E-46", "1E-40"] {
        let d = Decimal128::from_str(s).unwrap();
        let (v, st) = d.to_f64(RoundingMode::NearestEven);
        println!("{s:>26} -> f64 {v:e} {:?} inexact={} underflow={}",
            v.classify(), st.inexact(), st.underflow());
    }
    // the shared predicate directly
    let st = ferrodec_ieee::binary_conversion_status(1, -310, false, false, true, 53);
    println!("binary_conversion_status(1, -310, subnormal) bits={:#04x} underflow={}", st.bits(), st.underflow());
}
