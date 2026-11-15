// Emits sibling arithmetic results over the structured family for a libmpdec (clamp=1) check, CLAMPED included.
use ferrodec_decimal64::RoundingMode;
const MODES: [(RoundingMode, &str); 5] = [(RoundingMode::NearestEven, "NE"), (RoundingMode::NearestAway, "NA"), (RoundingMode::TowardZero, "TZ"), (RoundingMode::TowardPositive, "TP"), (RoundingMode::TowardNegative, "TN")];
struct Rng(u64);
impl Rng { fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 } fn below(&mut self, n: usize) -> usize { (self.next() % n as u64) as usize } }
fn family(p: u32, qmin: i32, qmax: i32) -> Vec<(bool, u64, i32)> {
    let mut cs: Vec<u64> = vec![0, 1, 2, 3, 5, 9, 25, 75];
    for k in 1..p { cs.push(10u64.pow(k)); cs.push(5 * 10u64.pow(k - 1)); cs.push(10u64.pow(k) + 1); cs.push(15 * 10u64.pow(k - 1)); }
    for k in 1..=p { cs.push(10u64.pow(k) - 1); }
    cs.push(10u64.pow(p) - 2); cs.push(5 * 10u64.pow(p - 1) - 1); cs.push(5 * 10u64.pow(p - 1) + 1);
    cs.sort(); cs.dedup();
    let pi = p as i32;
    let mut es = vec![qmin, qmin + 1, qmin + 2, qmin + pi - 1, qmin + pi, qmin + 2 * pi, -2 * pi, -pi, -pi + 1, -1, 0, 1, pi - 1, pi, 2 * pi, qmax - pi, qmax - 2, qmax - 1, qmax, qmin / 2, qmax / 2];
    es.sort(); es.dedup();
    let mut v = vec![];
    for &c in &cs { for &e in &es { for s in [false, true] { v.push((s, c, e)); } } }
    v
}
macro_rules! go {
    ($T:ty, $P:ident, $C:ty, $tag:expr, $fam:expr, $n:expr, $seed:expr) => {{
        use std::io::Write;
        let out = std::io::stdout(); let mut out = std::io::BufWriter::new(out.lock());
        let mk = |o: &(bool, u64, i32)| <$T>::from_parts($P { negative: o.0, coefficient: o.1 as $C, exponent: o.2 as i16 }).unwrap();
        let sh = |x: $T| -> String { match x.decode() { Some(p) => format!("{}{}E{}", if p.negative { "-" } else { "" }, p.coefficient, p.exponent), None => format!("{}", x) } };
        let mut rng = Rng($seed);
        let fam = $fam;
        for _ in 0..$n {
            let a = mk(&fam[rng.below(fam.len())]); let b = mk(&fam[rng.below(fam.len())]); let c = mk(&fam[rng.below(fam.len())]);
            for (m, mn) in MODES {
                let r = a.add(b, m); writeln!(out, "{} add {} {} {} _ {} {}", $tag, mn, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
                let r = a.mul(b, m); writeln!(out, "{} mul {} {} {} _ {} {}", $tag, mn, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
                let r = a.div(b, m); writeln!(out, "{} div {} {} {} _ {} {}", $tag, mn, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
                let r = a.fma(b, c, m); writeln!(out, "{} fma {} {} {} {} {} {}", $tag, mn, sh(a), sh(b), sh(c), sh(r.0), r.1.bits()).unwrap();
                let r = a.sqrt(m); writeln!(out, "{} sqrt {} {} _ _ {} {}", $tag, mn, sh(a), sh(r.0), r.1.bits()).unwrap();
            }
            let r = a.rem_near(b); writeln!(out, "{} remnear NE {} {} _ {} {}", $tag, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
            let r = a.rem_trunc(b); writeln!(out, "{} remtrunc NE {} {} _ {} {}", $tag, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
            let r = a.divide_integer(b); writeln!(out, "{} divint NE {} {} _ {} {}", $tag, sh(a), sh(b), sh(r.0), r.1.bits()).unwrap();
        }
    }};
}
fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    use ferrodec_decimal64::{Decimal64, Decimal64Parts}; use ferrodec_decimal32::{Decimal32, Decimal32Parts};
    go!(Decimal64, Decimal64Parts, u64, "d64", family(16, -398, 369), n, 0xdead_beef_1234_5677);
    go!(Decimal32, Decimal32Parts, u32, "d32", family(7, -101, 90), n, 0x1357_9bdf_2468_ace1);
}
