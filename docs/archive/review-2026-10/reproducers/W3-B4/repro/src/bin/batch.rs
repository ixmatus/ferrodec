use ferrodec_decimal::*;
use std::io::{BufRead, Write};
fn rnd(s: &str) -> Rounding { match s {
 "ROUND_HALF_EVEN"=>Rounding::HalfEven,"ROUND_HALF_UP"=>Rounding::HalfUp,"ROUND_HALF_DOWN"=>Rounding::HalfDown,
 "ROUND_DOWN"=>Rounding::Down,"ROUND_UP"=>Rounding::Up,"ROUND_CEILING"=>Rounding::Ceiling,"ROUND_FLOOR"=>Rounding::Floor,
 "ROUND_05UP"=>Rounding::ZeroFiveUp, _=>panic!("mode")}}
fn main(){
 let stdin = std::io::stdin(); let mut out = std::io::BufWriter::new(std::io::stdout());
 for line in stdin.lock().lines() {
  let line = line.unwrap(); let t: Vec<&str> = line.split_whitespace().collect();
  let ctx = Context::new(NonZeroU32::new(t[1].parse().unwrap()).unwrap(), t[2].parse().unwrap(), t[3].parse().unwrap(), rnd(t[4])).with_clamp(t[5]=="1");
  let o: Vec<Decimal> = t[6..].iter().map(|s| s.parse().unwrap()).collect();
  let r = std::panic::catch_unwind(|| match t[0] {
   "exp"=>o[0].exp(&ctx),"ln"=>o[0].ln(&ctx),"log10"=>o[0].log10(&ctx),"sqrt"=>o[0].sqrt(&ctx),
   "power"=>o[0].power(&o[1],&ctx),"add"=>o[0].add(&o[1],&ctx),"subtract"=>o[0].subtract(&o[1],&ctx),
   "multiply"=>o[0].multiply(&o[1],&ctx),"divide"=>o[0].divide(&o[1],&ctx),"divide_int"=>o[0].divide_integer(&o[1],&ctx),
   "remainder"=>o[0].remainder(&o[1],&ctx),"remainder_near"=>o[0].remainder_near(&o[1],&ctx),
   "quantize"=>o[0].quantize(&o[1],&ctx),"fma"=>o[0].fma(&o[1],&o[2],&ctx),"plus"=>o[0].plus(&ctx),
   "minus"=>o[0].minus(&ctx),"next_plus"=>o[0].next_plus(&ctx),"next_minus"=>o[0].next_minus(&ctx),
   "next_toward"=>o[0].next_toward(&o[1],&ctx),"to_integral_value"=>o[0].round_to_integral_value(&ctx),
   "to_integral_exact"=>o[0].round_to_integral_exact(&ctx),"normalize"=>o[0].reduce(&ctx),
   "scaleb"=>o[0].scaleb(&o[1],&ctx),"logb"=>o[0].logb(&ctx),"compare"=>o[0].compare(&o[1],&ctx),
   "max"=>o[0].maxnum(&o[1],&ctx),"min"=>o[0].minnum(&o[1],&ctx),
   _=>panic!("op")});
  match r { Ok((d,s)) => { let b=s.bits(); writeln!(out, "{} {}", d, b).unwrap(); }
            Err(_) => { writeln!(out, "PANIC 0").unwrap(); } }
 }
}
