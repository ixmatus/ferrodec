use ferrodec_multiword::decbig::{ADDBACK, D3DEC, D3DEC2, QLIMBS};
use ferrodec_multiword::DecBig;
use std::io::{BufRead, Write};
use std::sync::atomic::Ordering::Relaxed;
fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "COUNT" {
            writeln!(out, "COUNT {} qlimbs={} d3dec1={} d3dec2={} addback={}", f[1], QLIMBS.swap(0, Relaxed), D3DEC.swap(0, Relaxed), D3DEC2.swap(0, Relaxed), ADDBACK.swap(0, Relaxed)).unwrap();
            continue;
        }
        let a = DecBig::from_ascii_digits(f[0].as_bytes());
        let b = DecBig::from_ascii_digits(f[1].as_bytes());
        let before = ADDBACK.load(Relaxed);
        let (q, r) = a.div_rem(&b);
        let fired = ADDBACK.load(Relaxed) - before;
        writeln!(out, "{} {} {}", q, r, fired).unwrap();
    }
}
