use ferrodec::{Decimal128, RoundingMode as R128};
use ferrodec_decimal32::{Decimal32, RoundingMode as R32};
use ferrodec_decimal64::{Decimal64, RoundingMode as R64};
use std::panic::{catch_unwind, UnwindSafe};
fn t<T: std::fmt::Display>(label: String, f: impl FnOnce() -> T + UnwindSafe) {
    match catch_unwind(f) { Ok(v) => println!("{label}: {v}"), Err(_) => println!("{label}: PANICKED") }
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let p = |s: &str| Decimal128::parse_str(s, R128::NearestEven).unwrap().0;
    for s in ["14149.9","14150", "14150.01", "14150.05", "14150.07"] {
        let x = p(s);
        t(format!("d128 sinh({s})"), move || { let (r, st) = x.sinh(R128::NearestEven); format!("{r} {st:?}") });
        t(format!("d128 cosh({s})"), move || { let (r, st) = x.cosh(R128::NearestEven); format!("{r} {st:?}") });
    }
    let p64 = |s: &str| Decimal64::parse_str(s, R64::NearestEven).unwrap().0;
    for s in ["886.9","887", "887.01", "887.1", "887.18"] {
        let x = p64(s);
        t(format!("d64 sinh({s})"), move || { let (r, st) = x.sinh(R64::NearestEven); format!("{r} {st:?}") });
        t(format!("d64 cosh({s})"), move || { let (r, st) = x.cosh(R64::NearestEven); format!("{r} {st:?}") });
    }
    let p32 = |s: &str| Decimal32::parse_str(s, R32::NearestEven).unwrap().0;
    for s in ["223.9","224", "224.01", "224.03"] {
        let x = p32(s);
        t(format!("d32 sinh({s})"), move || { let (r, st) = x.sinh(R32::NearestEven); format!("{r} {st:?}") });
        t(format!("d32 cosh({s})"), move || { let (r, st) = x.cosh(R32::NearestEven); format!("{r} {st:?}") });
    }
}
