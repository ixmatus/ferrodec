use ferrodec_multiword::bigconst::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: u64 = a[2].parse().unwrap();
    let t = std::time::Instant::now();
    let v = match a[1].as_str() {
        "pi" => pi_digits(n), "two_over_pi" => two_over_pi_digits(n), "ln2" => ln2_digits(n),
        "ln10" => ln10_digits(n), "e" => e_digits(n), "tan_pi_8" => tan_pi_over_eight_digits(n),
        "inv_ln2" => inv_ln2_digits(n), "inv_ln10" => inv_ln10_digits(n), "inv_pi" => inv_pi_digits(n),
        _ => panic!(),
    };
    eprintln!("{} {} took {:?}", a[1], n, t.elapsed());
    println!("{}", v);
}
