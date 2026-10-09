//! A1 reviewer probe (scratch copy only; not part of the repository).
//! Reads cases from $A1_CASES, calls the classifiers directly, writes
//! results to $A1_OUT.
#![allow(clippy::all, clippy::pedantic, unused)]
extern crate std;
use std::{format, string::String, vec::Vec, io::{BufRead, Write}};
use crate::format::DecimalFormat as CF;
use crate::extended::Extended as CExt;
use ferrodec_ieee::{IeeeDecodedClass as Class, RoundingMode, Status};
use ferrodec_multiword::U256;
use core::cmp::Ordering;
use ferrodec_transcend::DecimalFormat as RF;

macro_rules! wrap {
    ($w:ident, $t:ty) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $w(pub $t);

        impl CF for $w {
            const BIAS: i32 = <$t as RF>::BIAS;
            const PRECISION: u32 = <$t as RF>::PRECISION;
            const ZERO: Self = $w(<$t as RF>::ZERO);
            const NEG_ZERO: Self = $w(<$t as RF>::NEG_ZERO);
            const ONE: Self = $w(<$t as RF>::ONE);
            const NEG_ONE: Self = $w(<$t as RF>::NEG_ONE);
            const TEN: Self = $w(<$t as RF>::TEN);
            const INFINITY: Self = $w(<$t as RF>::INFINITY);
            const NEG_INFINITY: Self = $w(<$t as RF>::NEG_INFINITY);
            const NAN: Self = $w(<$t as RF>::NAN);
            const SIGNALING_NAN: Self = $w(<$t as RF>::SIGNALING_NAN);
            fn classify(self) -> Class { RF::classify(self.0) }
            fn is_nan(self) -> bool { RF::is_nan(self.0) }
            fn is_zero(self) -> bool { RF::is_zero(self.0) }
            fn is_infinite(self) -> bool { RF::is_infinite(self.0) }
            fn is_sign_negative(self) -> bool { RF::is_sign_negative(self.0) }
            fn is_signaling_nan(self) -> bool { RF::is_signaling_nan(self.0) }
            fn abs(self) -> Self { $w(RF::abs(self.0)) }
            fn neg(self) -> Self { $w(RF::neg(self.0)) }
            fn partial_cmp_fmt(self, o: Self) -> (Option<Ordering>, Status) { RF::partial_cmp_fmt(self.0, o.0) }
            fn nan_from(self) -> Self { $w(RF::nan_from(self.0)) }
            fn propagate_nan2(self, o: Self) -> Self { $w(RF::propagate_nan2(self.0, o.0)) }
            fn to_extended_parts(self) -> Option<(U256, i32, bool)> { RF::to_extended_parts(self.0) }
            fn round_and_pack_finite(c: U256, e: i32, q: i32, s: bool, ps: bool, rm: RoundingMode, st: Status) -> (Self, Status) {
                let (v, s2) = <$t as RF>::round_and_pack_finite(c, e, q, s, ps, rm, st); ($w(v), s2)
            }
            fn recip_seed(self, rm: RoundingMode) -> (Self, Status) { let (v, s) = RF::recip_seed(self.0, rm); ($w(v), s) }
            fn sqrt_seed(self, rm: RoundingMode) -> (Self, Status) { let (v, s) = RF::sqrt_seed(self.0, rm); ($w(v), s) }
            fn div_fmt(self, o: Self, rm: RoundingMode) -> (Self, Status) { let (v, s) = RF::div_fmt(self.0, o.0, rm); ($w(v), s) }
            fn mul_fmt(self, o: Self, rm: RoundingMode) -> (Self, Status) { let (v, s) = RF::mul_fmt(self.0, o.0, rm); ($w(v), s) }
            fn to_i32_fmt(self, rm: RoundingMode) -> (i32, Status) { RF::to_i32_fmt(self.0, rm) }
            fn exp_overflow_limit() -> CExt { let e = <$t as RF>::exp_overflow_limit(); CExt { coef: e.coef, exp: e.exp, sign: e.sign } }
            fn exp_underflow_limit() -> CExt { let e = <$t as RF>::exp_underflow_limit(); CExt { coef: e.coef, exp: e.exp, sign: e.sign } }
        }
    };
}
wrap!(W128, ferrodec::Decimal128);
wrap!(W64, ferrodec_decimal64::Decimal64);
wrap!(W32, ferrodec_decimal32::Decimal32);

trait Fmt: CF + core::fmt::Display { fn parse(s: &str) -> Self; }
impl core::fmt::Display for W128 { fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { write!(f, "{}", self.0) } }
impl core::fmt::Display for W64 { fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { write!(f, "{}", self.0) } }
impl core::fmt::Display for W32 { fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { write!(f, "{}", self.0) } }
impl Fmt for W128 { fn parse(s: &str) -> Self { let (v, st) = ferrodec::Decimal128::parse_str(s, RoundingMode::NearestEven).expect(s); assert!(!st.inexact(), "inexact parse {s}"); W128(v) } }
impl Fmt for W64 { fn parse(s: &str) -> Self { let (v, st) = ferrodec_decimal64::Decimal64::parse_str(s, RoundingMode::NearestEven).expect(s); assert!(!st.inexact(), "inexact parse {s}"); W64(v) } }
impl Fmt for W32 { fn parse(s: &str) -> Self { let (v, st) = ferrodec_decimal32::Decimal32::parse_str(s, RoundingMode::NearestEven).expect(s); assert!(!st.inexact(), "inexact parse {s}"); W32(v) } }

fn mode(s: &str) -> RoundingMode {
    match s { "NE" => RoundingMode::NearestEven, "NA" => RoundingMode::NearestAway, "TZ" => RoundingMode::TowardZero,
        "TP" => RoundingMode::TowardPositive, "TN" => RoundingMode::TowardNegative, _ => panic!("mode {s}") }
}

fn show<F: Fmt>(r: Option<(F, Status)>) -> String {
    match r { None => "None".into(), Some((v, s)) => format!("Some {} {}", v, s.bits()) }
}

fn run<F: Fmt>(op: &str, rm: RoundingMode, a: &[&str]) -> String {
    use crate::exact as X;
    match op {
        "cbrt" => show(X::cbrt_exact_input::<F>(F::parse(a[0]), rm)),
        "pow" => show(X::pow_exact_input::<F>(F::parse(a[0]), F::parse(a[1]), rm)),
        "powi" => show(X::powi_exact_input::<F>(F::parse(a[0]), a[1].parse().unwrap(), rm)),
        "rootn" => show(X::rootn_exact_input::<F>(F::parse(a[0]), a[1].parse().unwrap(), rm)),
        "rsqrt" => show(X::rsqrt_exact_input::<F>(F::parse(a[0]), rm)),
        "compound" => show(X::compound_exact_input::<F>(F::parse(a[0]), a[1].parse().unwrap(), rm)),
        "exp2" => show(X::exp2_exact_or_tie::<F>(F::parse(a[0]), rm)),
        "exp2m1" => show(X::exp2m1_exact_or_tie::<F>(F::parse(a[0]), rm)),
        "exp10" => show(X::exp10_integer::<F>(F::parse(a[0]), rm)),
        "exp10m1" => show(X::exp10m1_integer::<F>(F::parse(a[0]), rm)),
        "log2" => show(X::log2_exact::<F>(F::parse(a[0]), rm)),
        "log10" => show(X::log10_exact::<F>(F::parse(a[0]), rm)),
        "log2p1" => show(X::log2p1_exact::<F>(F::parse(a[0]), rm)),
        "log10p1" => show(X::log10p1_exact::<F>(F::parse(a[0]), rm)),
        "sinpi" | "cospi" | "tanpi" | "asinpi" | "acospi" | "atanpi" => {
            let x = F::parse(a[0]);
            let (c, e, sg) = x.to_extended_parts().unwrap();
            use crate::exact_pi as P;
            let r = match op { "sinpi" => P::sinpi_exact(c, e, sg), "cospi" => P::cospi_exact(c, e), "tanpi" => P::tanpi_exact(c, e, sg),
                "asinpi" => P::asinpi_exact(c, e, sg), "acospi" => P::acospi_exact(c, e, sg), _ => P::atanpi_exact(c, e, sg) };
            match r { None => "None".into(), Some(pe) => { let (v, st) = P::deliver_pi_exact::<F>(pe, rm); format!("Some {} {}", v, st.bits()) } }
        }
        "atan2pi" => {
            let y = F::parse(a[0]); let x = F::parse(a[1]);
            let (cy, ey, sy) = y.to_extended_parts().unwrap();
            let (cx, ex, sx) = x.to_extended_parts().unwrap();
            use crate::exact_pi as P;
            match P::atan2pi_exact(cy, ey, sy, cx, ex, sx) { None => "None".into(), Some(pe) => { let (v, st) = P::deliver_pi_exact::<F>(pe, rm); format!("Some {} {}", v, st.bits()) } }
        }
        "hypot" => {
            let x = F::parse(a[0]); let y = F::parse(a[1]);
            let (cx, qx, _) = x.to_extended_parts().unwrap();
            let (cy, qy, _) = y.to_extended_parts().unwrap();
            let x_is_w = match x.abs().partial_cmp_fmt(y.abs()).0 { Some(Ordering::Greater) => true, Some(Ordering::Less) => false, _ => qx <= qy };
            let (cw, qw, cz, qz) = if x_is_w { (cx, qx, cy, qy) } else { (cy, qy, cx, qx) };
            let delta0 = (F::PRECISION + 2).div_ceil(2) as i32;
            let adj_w = qw + cw.decimal_digit_count() as i32 - 1;
            let adj_z = qz + cz.decimal_digit_count() as i32 - 1;
            if adj_w - adj_z > delta0 { return "Anchor".into(); }
            show(X::hypot_exact_or_tie::<F>(cw, qw, cz, qz, rm))
        }
        _ => panic!("op {op}"),
    }
}

#[test]
fn a1_probe_run() {
    let Ok(cases) = std::env::var("A1_CASES") else { return; };
    let out = std::env::var("A1_OUT").unwrap();
    let f = std::fs::File::open(&cases).unwrap();
    let mut o = std::io::BufWriter::new(std::fs::File::create(&out).unwrap());
    for line in std::io::BufReader::new(f).lines() {
        let line = line.unwrap();
        if line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (fm, op, m) = (parts[0], parts[1], mode(parts[2]));
        let args = &parts[3..];
        let r = std::panic::catch_unwind(|| match fm {
            "128" => run::<W128>(op, m, args),
            "64" => run::<W64>(op, m, args),
            "32" => run::<W32>(op, m, args),
            _ => panic!(),
        });
        let r = match r { Ok(s) => s, Err(_) => "PANIC".into() };
        writeln!(o, "{line} => {r}").unwrap();
    }
}

macro_rules! api_run {
    ($t:ty, $op:expr, $rm:expr, $a:expr) => {{
        let p = |s: &str| -> $t { let (v, st) = <$t>::parse_str(s, RoundingMode::NearestEven).expect(s); assert!(!st.inexact()); v };
        let a = $a; let rm = $rm;
        let (v, s): ($t, Status) = match $op {
            "cbrt" => p(a[0]).cbrt(rm),
            "pow" => p(a[0]).pow(p(a[1]), rm),
            "powi" => p(a[0]).powi(a[1].parse().unwrap(), rm),
            "rootn" => p(a[0]).rootn(a[1].parse().unwrap(), rm),
            "rsqrt" => p(a[0]).rsqrt(rm),
            "compound" => p(a[0]).compound(a[1].parse().unwrap(), rm),
            "exp2" => p(a[0]).exp2(rm),
            "exp2m1" => p(a[0]).exp2_m1(rm),
            "exp10" => p(a[0]).exp10(rm),
            "exp10m1" => p(a[0]).exp10_m1(rm),
            "log2" => p(a[0]).log2(rm),
            "log10" => p(a[0]).log10(rm),
            "log2p1" => p(a[0]).log2_1p(rm),
            "log10p1" => p(a[0]).log10_1p(rm),
            "hypot" => p(a[0]).hypot(p(a[1]), rm),
            _ => panic!(),
        };
        format!("Some {} {}", v, s.bits())
    }};
}

#[test]
fn a1_api_run() {
    let Ok(cases) = std::env::var("A1_API_CASES") else { return; };
    let out = std::env::var("A1_API_OUT").unwrap();
    let f = std::fs::File::open(&cases).unwrap();
    let mut o = std::io::BufWriter::new(std::fs::File::create(&out).unwrap());
    for line in std::io::BufReader::new(f).lines() {
        let line = line.unwrap();
        if line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (fm, op, m) = (parts[0], parts[1], mode(parts[2]));
        let args: Vec<String> = parts[3..].iter().map(|s| String::from(*s)).collect();
        let r = std::panic::catch_unwind(|| {
            let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            match fm {
                "128" => api_run!(ferrodec::Decimal128, op, m, &a),
                "64" => api_run!(ferrodec_decimal64::Decimal64, op, m, &a),
                "32" => api_run!(ferrodec_decimal32::Decimal32, op, m, &a),
                _ => panic!(),
            }
        });
        let r = match r { Ok(s) => s, Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| String::from(*s))).unwrap_or_default();
            format!("PANIC {}", msg.replace('\n', " "))
        } };
        writeln!(o, "{line} => {r}").unwrap();
        o.flush().unwrap();
    }
}
