use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn st(b: u8) -> String {
    let mut s = String::new();
    for (bit, c) in [(1u8,'I'),(2,'Z'),(4,'O'),(8,'U'),(16,'X'),(32,'C')] { if b & bit != 0 { s.push(c); } }
    if s.is_empty() { s.push('-'); }
    s
}

macro_rules! rmap { ($m:ident, $s:expr) => { match $s {
    "NE" => $m::RoundingMode::NearestEven, "NA" => $m::RoundingMode::NearestAway,
    "TZ" => $m::RoundingMode::TowardZero, "TP" => $m::RoundingMode::TowardPositive,
    "TN" => $m::RoundingMode::TowardNegative, _ => panic!("rm") } } }

macro_rules! handler { ($name:ident, $m:ident, $T:ident, $U:ty, $w:expr) => {
fn $name(cmd: &str, rm: &str, arg: &str) -> String {
    use $m::$T as D;
    match cmd {
        "P" => match D::parse_str(arg, rmap!($m, rm)) {
            Ok((d, s)) => format!("{:0w$x} {} {}", d.to_bits(), st(s.bits()), d, w = $w),
            Err(e) => format!("ERR {:?}", e),
        },
        "B" => { let bits = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_bits(bits);
            format!("sci={} e={:e} E={:E} eng={} fp={} p3={:.3} p3e={:.3e} canon={:0w$x} dbg={:?}", d, d, d, d.engineering(), d.fixed_preferred(), d, d, d.canonicalize().to_bits(), d, w=$w) }
        "ENG" => { let (d,_) = D::parse_str(arg, $m::RoundingMode::NearestEven).unwrap(); format!("{}", d.engineering()) }
        "FMT" => { let (d,_) = D::parse_str(arg, $m::RoundingMode::NearestEven).unwrap();
            format!("[{:>12}] [{:+}] [{:<8}|] [{:.2}] [{:.2e}] [{:.0e}] [{:.2}]eng", d, d, d, d, d, d, 0) .replace("[0]eng", &format!("[{:.2}]eng", d.engineering())) }
        "F2D" => { let bits = u64::from_str_radix(arg, 16).unwrap(); let (d, s) = D::from_f64(f64::from_bits(bits), rmap!($m, rm));
            format!("{:0w$x} {} {}", d.to_bits(), st(s.bits()), d, w=$w) }
        "D2F" => { let (d,_) = D::parse_str(arg, $m::RoundingMode::NearestEven).unwrap(); let (f, s) = d.to_f64(rmap!($m, rm));
            format!("{:016x} {} {:e}", f.to_bits(), st(s.bits()), f) }
        "D2F32" => { let (d,_) = D::parse_str(arg, $m::RoundingMode::NearestEven).unwrap(); let (f, s) = d.to_f32(rmap!($m, rm));
            format!("{:08x} {} {:e}", f.to_bits(), st(s.bits()), f) }
        "BD2F" => { let bits = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_bits(bits); let (f, s) = d.to_f64(rmap!($m, rm));
            format!("{:016x} {} {:e}", f.to_bits(), st(s.bits()), f) }
        "INT" => { let (d,_) = D::parse_str(arg, $m::RoundingMode::NearestEven).unwrap(); let r = rmap!($m, rm);
            let (a, sa) = d.to_i32(r); let (b, sb) = d.to_i64(r); let (c, sc) = d.to_u64(r); let (e, se) = d.to_i128(r); let (g, sg) = d.to_u128(r); let (h, sh) = d.to_u32(r);
            format!("i32={} {} i64={} {} u64={} {} i128={} {} u128={} {} u32={} {}", a, st(sa.bits()), b, st(sb.bits()), c, st(sc.bits()), e, st(se.bits()), g, st(sg.bits()), h, st(sh.bits())) }
        "DPD" => { let bits = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_bits(bits); let b = d.to_dpd_bytes(); let back = D::from_dpd_bytes(b);
            format!("dpd={} back={:0w$x} {}", b.iter().map(|x| format!("{:02x}", x)).collect::<String>(), back.to_bits(), back, w=$w) }
        "FROMDPD" => { let v = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_dpd_bytes(v.to_be_bytes());
            format!("{:0w$x} {} dpd={}", d.to_bits(), d, d.to_dpd_bytes().iter().map(|x| format!("{:02x}", x)).collect::<String>(), w=$w) }
        "SERDE" => { let bits = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_bits(bits);
            let j = serde_json::to_string(&d).unwrap(); let back: Result<D, _> = serde_json::from_str(&j);
            let bc = bincode::serialize(&d).unwrap(); let bb: Result<D,_> = bincode::deserialize(&bc);
            format!("json={} back={:?} bincode_len={} bback={:?}", j, back.map(|x| format!("{:0w$x}", x.to_bits(), w=$w)), bc.len(), bb.map(|x| format!("{:0w$x}", x.to_bits(), w=$w))) }
        "RT" => { let bits = <$U>::from_str_radix(arg, 16).unwrap(); let d = D::from_bits(bits); let c = d.canonicalize().to_bits();
            let mut bad = String::new();
            for (k, txt) in [("sci", format!("{}", d)), ("eng", format!("{}", d.engineering())), ("e", format!("{:e}", d)), ("fp", format!("{}", d.fixed_preferred()))] {
                match D::parse_str(&txt, $m::RoundingMode::NearestEven) {
                    Ok((b, st)) => { let same = if k == "fp" { (b.is_nan() && d.is_nan()) || b.to_bits() == c || (b.is_finite() && d.is_finite() && b.sub(d, $m::RoundingMode::NearestEven).0.is_zero()) } else { b.to_bits() == c }; if !same || st.bits() != 0 { bad.push_str(&format!(" {}:{}->{:0w$x}/{}", k, txt, b.to_bits(), st.bits(), w=$w)); } }
                    Err(e) => bad.push_str(&format!(" {}:{}->ERR{:?}", k, txt, e)),
                } }
            if bad.is_empty() { "ok".into() } else { format!("BAD{}", bad) } }
        "JSON" => { let back: Result<D, _> = serde_json::from_str(arg); format!("{:?}", back.map(|x| format!("{:0w$x} {}", x.to_bits(), x, w=$w))) }
        _ => "?".into(),
    }
}
}}
handler!(h128, ferrodec, Decimal128, u128, 32);
handler!(h64, ferrodec_decimal64, Decimal64, u64, 16);
handler!(h32, ferrodec_decimal32, Decimal32, u32, 8);

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let stdin = io::stdin(); let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut it = line.splitn(4, ' ');
        let (cmd, fmt, rm, arg) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap_or(""));
        let r = catch_unwind(AssertUnwindSafe(|| match fmt { "128" => h128(cmd, rm, arg), "64" => h64(cmd, rm, arg), "32" => h32(cmd, rm, arg), _ => "?".into() }));
        match r { Ok(s) => writeln!(out, "{}", s).unwrap(), Err(e) => { let m = e.downcast_ref::<String>().cloned().or(e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(); writeln!(out, "PANIC {}", m).unwrap() } }
    }
}
