// C2-1 shape at the sibling widths: a·b an exact power of ten with >= p digits, opposite-sign addend far below.
use ferrodec_test_support::oracle::{self, Dec, Expect, Format};
use ferrodec_decimal64::{RoundingMode, Status};
use num_bigint::BigUint;
const MODES: [RoundingMode; 5] = [RoundingMode::NearestEven, RoundingMode::NearestAway, RoundingMode::TowardZero, RoundingMode::TowardPositive, RoundingMode::TowardNegative];
macro_rules! sweep { ($T:ty, $P:ident, $C:ty, $p:expr, $qmin:expr, $qmax:expr, $F:expr, $tag:expr) => {{
    let mut cases = 0u64; let mut bad = 0u64;
    let mk = |n: bool, c: u64, e: i32| <$T>::from_parts($P { negative: n, coefficient: c as $C, exponent: e as i16 });
    let p: u32 = $p;
    let cs: Vec<u64> = vec![1, 5, 9, 10u64.pow(p - 1), 10u64.pow(p) - 1, 5 * 10u64.pow(p - 1), 10u64.pow(p - 1) + 1];
    for i in 0..p { for j in 0..p { if i + j + 1 < p { continue; }
        for ea in [-20i32, -1, 0, 3, 10] { for eb in [-5i32, 0, 7] {
            for &cc in &cs { for gap in 1..(3 * p as i32 + 8) {
                let top_ab = ea + eb + (i + j) as i32;
                let ec = top_ab - gap - (cc.to_string().len() as i32 - 1);
                if ec < $qmin || ec > $qmax { continue; }
                for sc in [false, true] {
                    let (Some(a), Some(b), Some(c)) = (mk(false, 10u64.pow(i), ea), mk(false, 10u64.pow(j), eb), mk(sc, cc, ec)) else { continue };
                    let da = Dec::new(false, BigUint::from(10u64.pow(i)), ea); let db = Dec::new(false, BigUint::from(10u64.pow(j)), eb); let dc = Dec::new(sc, BigUint::from(cc), ec);
                    for m in MODES {
                        let (r, s) = a.fma(b, c, m); let w = oracle::fma(&da, &db, &dc, $F, m); cases += 1;
                        let mask = !Status::CLAMPED.bits();
                        let ok = (s.bits() & mask) == (w.status.bits() & mask) && match &w.value {
                            Expect::Finite { neg, coeff, exp } => r.decode().map_or(false, |q| q.negative == *neg && BigUint::from(q.coefficient as u64) == *coeff && q.exponent as i32 == *exp),
                            Expect::Infinity { neg } => r.is_infinite() && r.is_sign_negative() == *neg, Expect::Nan => r.is_nan() };
                        if !ok { bad += 1; if bad < 5 { println!("{} BAD 10^{}E{} 10^{}E{} {}{}E{} {:?} got {} want {}", $tag, i, ea, j, eb, if sc {"-"} else {""}, cc, ec, m as u8, r, w.decimal_string()); } }
                    }
                }
            }}
        }}
    }}
    println!("{} c21-shape cases={} mismatches={}", $tag, cases, bad);
}}}
fn main() {
    use ferrodec_decimal64::{Decimal64, Decimal64Parts}; use ferrodec_decimal32::{Decimal32, Decimal32Parts};
    sweep!(Decimal64, Decimal64Parts, u64, 16, -398, 369, Format::DECIMAL64, "d64");
    sweep!(Decimal32, Decimal32Parts, u32, 7, -101, 90, Format::DECIMAL32, "d32");
}
