use ferrodec::Decimal128;
use ferrodec_ieee::RoundingMode;
use ferrodec_multiword::bigconst;

fn main() {
    // The Decimal128 input with the deepest Payne-Hanek cancellation
    // (continued-fraction search over every exponent): ||x*2/pi|| = 1.317e-38.
    let (x, _) = Decimal128::parse_str(
        "9308532438209917461067659354862169E4639",
        RoundingMode::NearestEven,
    )
    .unwrap();
    let (k, r, st) = ferrodec_transcend::argred::reduce::<Decimal128>(x);
    println!("REDUCE k_mod4={k} status={st:?}");
    println!("REDUCE r_debug={r:?}");
    for rm in [
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
        RoundingMode::TowardZero,
    ] {
        let (s, st) = x.sin(rm);
        let (c, stc) = x.cos(rm);
        println!("SIN {rm:?} {s} {st:?}");
        println!("COS {rm:?} {c} {stc:?}");
    }
    for n in [50u64, 100, 333, 1000, 2000] {
        println!("GEN pi {n} {}", bigconst::pi_digits(n));
        println!("GEN two_over_pi {n} {}", bigconst::two_over_pi_digits(n));
        println!("GEN ln2 {n} {}", bigconst::ln2_digits(n));
        println!("GEN ln10 {n} {}", bigconst::ln10_digits(n));
        println!("GEN e {n} {}", bigconst::e_digits(n));
        println!("GEN tan_pi_over_eight {n} {}", bigconst::tan_pi_over_eight_digits(n));
        println!("GEN inv_ln2 {n} {}", bigconst::inv_ln2_digits(n));
        println!("GEN inv_pi {n} {}", bigconst::inv_pi_digits(n));
        println!("GEN inv_ln10 {n} {}", bigconst::inv_ln10_digits(n));
    }
}
