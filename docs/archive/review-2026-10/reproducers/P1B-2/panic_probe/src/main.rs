//! Coordinator's widened probe for the C5-1/C5-2 class: every public
//! kernel that seeds a Newton reciprocal or square root through the
//! destination format, fed operands whose square or reciprocal leaves
//! the format's range. One line per call: ok(value, status) or PANICKED.
use ferrodec::{Decimal128, RoundingMode as R128};
use ferrodec_decimal32::{Decimal32, RoundingMode as R32};
use ferrodec_decimal64::{Decimal64, RoundingMode as R64};
use std::panic::{catch_unwind, UnwindSafe};

fn t<T: std::fmt::Debug>(label: String, f: impl FnOnce() -> T + UnwindSafe) {
    match catch_unwind(f) {
        Ok(v) => println!("{label}: ok {v:?}"),
        Err(_) => println!("{label}: PANICKED"),
    }
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let p = |s: &str| Decimal128::parse_str(s, R128::NearestEven).unwrap().0;
    let rm = R128::NearestEven;
    // Decimal128: square overflows past 1E3072, reciprocal overflows below 1E-6144.
    for s in ["1E3073", "9.999999999999999999999999999999999E6144", "-1E3073"] {
        let x = p(s);
        t(format!("d128 acosh({s})"), move || x.acosh(rm));
        t(format!("d128 asinh({s})"), move || x.asinh(rm));
        t(format!("d128 rsqrt({s})"), move || x.rsqrt(rm));
        t(format!("d128 hypot({s}, {s})"), move || x.hypot(x, rm));
        t(format!("d128 hypot({s}, 1)"), move || x.hypot(p("1"), rm));
        t(format!("d128 rootn({s}, 3)"), move || x.rootn(3, rm));
        t(format!("d128 rootn({s}, -3)"), move || x.rootn(-3, rm));
        t(format!("d128 cbrt({s})"), move || x.cbrt(rm));
        t(format!("d128 atan({s})"), move || x.atan(rm));
        t(format!("d128 atan_pi({s})"), move || x.atan_pi(rm));
        t(format!("d128 powi({s}, -1)"), move || x.powi(-1, rm));
        t(format!("d128 tanh({s})"), move || x.tanh(rm));
    }
    for s in ["1E-6176", "1E-6150", "1E-6145", "9E-6144"] {
        let x = p(s);
        t(format!("d128 rsqrt({s})"), move || x.rsqrt(rm));
        t(format!("d128 rootn({s}, -3)"), move || x.rootn(-3, rm));
        t(format!("d128 rootn({s}, 3)"), move || x.rootn(3, rm));
        t(format!("d128 powi({s}, -1)"), move || x.powi(-1, rm));
        t(format!("d128 pow({s}, -1)"), move || x.pow(p("-1"), rm));
        t(format!("d128 powr({s}, -1)"), move || x.powr(p("-1"), rm));
        t(format!("d128 atan2(1, {s})"), move || p("1").atan2(x, rm));
        t(format!("d128 atan2_pi(1, {s})"), move || p("1").atan2_pi(x, rm));
        t(format!("d128 atan2({s}, 1)"), move || x.atan2(p("1"), rm));
        t(format!("d128 hypot({s}, {s})"), move || x.hypot(x, rm));
        t(format!("d128 atanh(1 - {s})"), move || p("1").sub(x, rm).0.atanh(rm));
        t(format!("d128 asin(1 - {s})"), move || p("1").sub(x, rm).0.asin(rm));
        t(format!("d128 acos(-1 + {s})"), move || p("-1").add(x, rm).0.acos(rm));
    }
    // Decimal64 and Decimal32 twins of atan2, atan2_pi, acosh, hypot, rsqrt, rootn.
    let p64 = |s: &str| Decimal64::parse_str(s, R64::NearestEven).unwrap().0;
    let r64 = R64::NearestEven;
    for (y, x) in [("1", "1E-398"), ("1E384", "1E-398"), ("1E-390", "1E-398")] {
        let (yy, xx) = (p64(y), p64(x));
        t(format!("d64 atan2({y}, {x})"), move || yy.atan2(xx, r64));
        t(format!("d64 atan2_pi({y}, {x})"), move || yy.atan2_pi(xx, r64));
    }
    for s in ["1E193", "9.999999999999999E384"] {
        let x = p64(s);
        t(format!("d64 acosh({s})"), move || x.acosh(r64));
        t(format!("d64 hypot({s}, {s})"), move || x.hypot(x, r64));
        t(format!("d64 rsqrt({s})"), move || x.rsqrt(r64));
        t(format!("d64 rootn({s}, 3)"), move || x.rootn(3, r64));
    }
    let p32 = |s: &str| Decimal32::parse_str(s, R32::NearestEven).unwrap().0;
    let r32 = R32::NearestEven;
    for (y, x) in [("1", "1E-101"), ("1E96", "1E-101"), ("1E-95", "1E-101")] {
        let (yy, xx) = (p32(y), p32(x));
        t(format!("d32 atan2({y}, {x})"), move || yy.atan2(xx, r32));
        t(format!("d32 atan2_pi({y}, {x})"), move || yy.atan2_pi(xx, r32));
    }
    for s in ["1E50", "9.999999E96"] {
        let x = p32(s);
        t(format!("d32 acosh({s})"), move || x.acosh(r32));
        t(format!("d32 hypot({s}, {s})"), move || x.hypot(x, r32));
        t(format!("d32 rsqrt({s})"), move || x.rsqrt(r32));
        t(format!("d32 rootn({s}, 3)"), move || x.rootn(3, r32));
    }
}
