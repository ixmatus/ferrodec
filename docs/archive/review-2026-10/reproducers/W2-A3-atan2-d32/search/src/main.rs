use std::io::BufRead;
// stdin: lines "cx e2"; for D=-5 decade: q = cy/cx * 1e-4 (cy 7 digits),
// L = cy*1e7/cx, ulp = 1e-11, C = (q^3/3 - q^5/5)/ulp, eps = q*e2/ulp.
// hit when frac(L) - C - off in (0, eps] (+tiny slack), off in {0, 0.5}.
fn main() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut it = line.split_whitespace();
        let cx: u64 = it.next().unwrap().parse().unwrap();
        let e2: f64 = it.next().unwrap().parse().unwrap();
        for cy in 1_000_000u64..10_000_000u64 {
            let r = (cy * 10_000_000) % cx;
            let fr = r as f64 / cx as f64;
            let q = cy as f64 / cx as f64 * 1e-4;
            if !(q >= 1e-5 && q < 1e-4) { continue; }
            let c = (q * q * q / 3.0 - q.powi(5) / 5.0) / 1e-11;
            let eps = q * e2 / 1e-11;
            for off in [0.0f64, 0.5] {
                let mut d = fr - c - off;
                if d < -0.5 { d += 1.0; }
                if d > -1e-12 && d <= eps + 1e-12 {
                    println!("{} {} {} {:e} {:e}", cx, cy, off, d, eps);
                }
            }
        }
    }
}
