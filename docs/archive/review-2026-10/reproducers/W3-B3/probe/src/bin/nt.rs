use num_traits::{FromPrimitive, ToPrimitive, Signed, Num, Zero, One};
macro_rules! t { ($T:ty, $name:expr) => {{
    let p = |s: &str| -> $T { s.parse().unwrap() };
    println!("== {}", $name);
    println!("from_i128(1e20)={:?} from_u128(1e20)={:?}", <$T as FromPrimitive>::from_i128(100_000_000_000_000_000_000).map(|d| d.to_string()), <$T as FromPrimitive>::from_u128(100_000_000_000_000_000_000).map(|d| d.to_string()));
    println!("from_f64(NaN)={:?} from_f64(inf)={:?} from_f32(0.1)={:?}", <$T as FromPrimitive>::from_f64(f64::NAN).map(|d| d.to_string()), <$T as FromPrimitive>::from_f64(f64::INFINITY).map(|d| d.to_string()), <$T as FromPrimitive>::from_f32(0.1f32).map(|d| d.to_string()));
    println!("to_i64(2.7)={:?} to_i64(-2.5)={:?} to_u64(-0.4)={:?} to_i64(NaN)={:?} to_u8(300)={:?} to_i8(-2.7)={:?} f64 2.7 to_i64={:?}", ToPrimitive::to_i64(&p("2.7")), ToPrimitive::to_i64(&p("-2.5")), ToPrimitive::to_u64(&p("-0.4")), ToPrimitive::to_i64(&p("NaN")), ToPrimitive::to_u8(&p("300")), ToPrimitive::to_i8(&p("-2.7")), ToPrimitive::to_i64(&2.7f64));
    println!("signum(NaN)={} signum(-0)={} signum(-sNaN)={} is_neg(-0)={} is_pos(NaN)={} abs(-NaN)={}", Signed::signum(&p("NaN")), Signed::signum(&p("-0")), Signed::signum(&p("-sNaN")), Signed::is_negative(&p("-0")), Signed::is_positive(&p("NaN")), Signed::abs(&p("-NaN")));
    println!("radix16={:?} radix10 ' 1'={:?}", <$T as Num>::from_str_radix("10", 16).map(|d| d.to_string()), <$T as Num>::from_str_radix(" 1", 10).map(|d| d.to_string()));
    println!("is_one(1.0)={} is_zero(0E+5)={}", One::is_one(&p("1.0")), Zero::is_zero(&p("0E+5")));
    println!("neg(sNaN)={} -NaN5={} ", -p("sNaN"), -p("NaN5"));
    println!("7%3 ={} 7.5%2={} 5%2={} -5%2={} 3%2={} 1E+400%7={}", p("7")%p("3"), p("7.5")%p("2"), p("5")%p("2"), p("-5")%p("2"), p("3")%p("2"), p("1E+90")%p("7"));
    println!("sum={} product={}", [p("1E+300"), p("1E+300"), p("-1E+300")].iter().copied().sum::<$T>(), [p("1E-60"), p("1E-60"), p("1E+60")].iter().copied().product::<$T>());
    println!("1/0={} 1/3={}", p("1")/p("0"), p("1")/p("3"));
}}}
fn main() {
    t!(ferrodec::Decimal128, "d128");
    t!(ferrodec_decimal64::Decimal64, "d64");
    t!(ferrodec_decimal32::Decimal32, "d32");
}
