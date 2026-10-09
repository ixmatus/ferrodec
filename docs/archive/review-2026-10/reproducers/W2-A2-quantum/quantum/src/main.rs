//! A2: §9.2.2 preferred exponents on exact deliveries (cohort check).
use ferrodec::Decimal128;
use ferrodec_decimal32::Decimal32;
use ferrodec_decimal64::Decimal64;
use ferrodec::RoundingMode as R;
const POWI: &[(&str, i32)] = &[("1.20", 2), ("1.20", 3), ("2.0", -1), ("0.50", -2), ("1E+2", 3), ("100", 3),
    ("10E+1", 2), ("-1.5", 3), ("0.2", -3), ("0.20", -3), ("1.50", 1), ("0E+5", 3), ("0E+5", 1), ("-0E-3", 3),
    ("2.50", 4), ("1.1", 6), ("1.10", 6), ("-1.1", 5), ("5", -6), ("0.5", -6), ("2", 6), ("2.000", -6), ("1E-30", -3), ("4E+40", 2)];
const RSQRT: &[&str] = &["4", "0.25", "0.0025", "25E-4", "1E+72", "100E+70", "4.00", "400E-2", "1E-71", "16E-3", "1.6E-1"];
const ROOTN: &[(&str, i32)] = &[("8", 3), ("8.000", 3), ("1E+30", 5), ("1000000E+24", 5), ("8", -3), ("0.008", -3),
    ("-8.000", 3), ("32E-5", 5), ("1.50", 1), ("2.0", -1), ("4.00", 2), ("4.00", -2), ("0E+6", 3), ("0E+5", 1), ("64E-6", 6), ("64E-6", -6), ("1E-12", -4), ("16.0000", 4)];
const COMP: &[(&str, i32)] = &[("0.05", 3), ("0.050", 3), ("5", 2), ("0.25", -1), ("1E+1", 2), ("1", -3), ("0.10", 2),
    ("-0.5", -2), ("-0.50", 3), ("0E-3", 2), ("-0E-3", 3), ("0.5", 6), ("0.50", -6), ("3", 6), ("1", 49)];
const HYP: &[(&str, &str)] = &[("3", "4"), ("3.0", "4.00"), ("0.3", "0.4"), ("30E-1", "4"), ("0E+3", "0E-2"), ("-7.0", "0E+2"), ("5E+2", "12E+2")];
macro_rules! probe {
    ($T:ty, $name:expr) => {{
        let p = |s: &str| <$T>::parse_str(s, R::NearestEven).unwrap().0;
        let rm = R::NearestEven;
        for (x, n) in POWI { let (r, s) = p(x).powi(*n, rm); println!("{}|powi|{x}|{n}|{r}|{}", $name, s.bits()); }
        for x in RSQRT { let (r, s) = p(x).rsqrt(rm); println!("{}|rsqrt|{x}|-|{r}|{}", $name, s.bits()); }
        for (x, n) in ROOTN { let (r, s) = p(x).rootn(*n, rm); println!("{}|rootn|{x}|{n}|{r}|{}", $name, s.bits()); }
        for (x, n) in COMP { let (r, s) = p(x).compound(*n, rm); println!("{}|compound|{x}|{n}|{r}|{}", $name, s.bits()); }
        for (x, y) in HYP { let (r, s) = p(x).hypot(p(y), rm); println!("{}|hypot|{x}|{y}|{r}|{}", $name, s.bits()); }
    }};
}
fn main() { probe!(Decimal32, "d32"); probe!(Decimal64, "d64"); probe!(Decimal128, "d128"); }
