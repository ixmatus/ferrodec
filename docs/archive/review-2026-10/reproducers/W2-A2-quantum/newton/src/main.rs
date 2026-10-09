//! A2: measured relative error of the rung 1 Newton surfaces at the
//! three seed widths (Extended::recip / sqrt / the rsqrt composition and
//! polish), on deterministic pseudo-random 40-digit operands.
use ferrodec::Decimal128;
use ferrodec_decimal32::Decimal32;
use ferrodec_decimal64::Decimal64;
use ferrodec_multiword::U256;
use ferrodec_transcend::extended::Extended;
use ferrodec_transcend::DecimalFormat;
use ferrodec::RoundingMode as R;

fn rel(e: Extended) -> f64 {
    let (d, _) = e.abs().to_format::<Decimal128>(0, R::NearestEven);
    format!("{d}").parse::<f64>().unwrap()
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
}

fn operand(r: &mut Lcg, lo_exp: i32) -> Extended {
    // 40-digit coefficient in [10^39, 10^40)
    let mut c: u128 = 1 + r.next() as u128 % 9;
    for _ in 0..39 {
        c = c * 10 + (r.next() % 10) as u128;
    }
    Extended::from_components(U256::from_u128(c), lo_exp - 39, false)
}

fn measure<F: DecimalFormat>(name: &str) {
    let one = Extended::from_i32(1);
    let three = Extended::from_i32(3);
    let mut r = Lcg(12345);
    let (mut mr, mut ms, mut mq, mut mp) = (0f64, 0f64, 0f64, 0f64);
    for i in 0..200_000 {
        // recip on [1,10)
        let v = operand(&mut r, 0);
        let y = v.recip::<F>();
        mr = mr.max(rel(one.sub(v.mul(y))));
        // sqrt on [1, 200) like hypot's scaled sum
        let s = if i % 2 == 0 { operand(&mut r, 0) } else { operand(&mut r, 1).mul(Extended::from_i32(2)) };
        let q = s.sqrt::<F>();
        // rel err of sqrt ~ (q^2 - s)/(2 s)
        let es = q.square().sub(s);
        ms = ms.max(rel(es) / (2.0 * rel(s)));
        // rsqrt composition on x in [1, 100): sqrt then recip, then polish
        let x = operand(&mut r, (i % 2) as i32);
        let y0 = x.sqrt::<F>().recip::<F>();
        let e0 = rel(one.sub(x.mul(y0.square()))) / 2.0;
        mq = mq.max(e0);
        let y1 = y0.mul(three.sub(x.mul(y0.square()))).div_u32(2);
        let e1 = rel(one.sub(x.mul(y1.square()))) / 2.0;
        mp = mp.max(e1);
    }
    println!("{name}: rung1 max rel err  recip {mr:.3e}  sqrt {ms:.3e}  rsqrt-unpolished {mq:.3e}  rsqrt-polished {mp:.3e}");
}

fn main() {
    measure::<Decimal32>("d32");
    measure::<Decimal64>("d64");
    measure::<Decimal128>("d128");
}
