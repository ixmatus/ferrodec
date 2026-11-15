// Usage: dc <op> <prec> <emax> <emin> <rounding> <clamp0/1> <a> [b] [c]
use ferrodec_decimal::*;
use std::time::Instant;
fn rnd(s: &str) -> Rounding { match s {
 "HalfEven"=>Rounding::HalfEven,"HalfUp"=>Rounding::HalfUp,"HalfDown"=>Rounding::HalfDown,
 "Down"=>Rounding::Down,"Up"=>Rounding::Up,"Ceiling"=>Rounding::Ceiling,"Floor"=>Rounding::Floor,
 "ZeroFiveUp"=>Rounding::ZeroFiveUp, _=>panic!("mode")}}
fn rd(s:&str)->Decimal{ if let Some(f)=s.strip_prefix("@") { std::fs::read_to_string(f).unwrap().trim().parse().unwrap() } else { s.parse().unwrap() } }
fn main(){
 let a: Vec<String> = std::env::args().collect();
 let p: u32 = a[2].parse().unwrap();
 let ctx = Context::new(NonZeroU32::new(p).unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap(), rnd(&a[5])).with_clamp(&a[6]=="1");
 let x = rd(&a[7]);
 let y = a.get(8).map(|s| rd(s));
 let z = a.get(9).map(|s| rd(s));
 let t = Instant::now();
 let (r, s) = match a[1].as_str() {
  "exp"=>x.exp(&ctx),"ln"=>x.ln(&ctx),"log10"=>x.log10(&ctx),"sqrt"=>x.sqrt(&ctx),
  "pow"=>x.power(y.as_ref().unwrap(),&ctx),"add"=>x.add(y.as_ref().unwrap(),&ctx),
  "sub"=>x.subtract(y.as_ref().unwrap(),&ctx),"mul"=>x.multiply(y.as_ref().unwrap(),&ctx),
  "div"=>x.divide(y.as_ref().unwrap(),&ctx),"divint"=>x.divide_integer(y.as_ref().unwrap(),&ctx),
  "rem"=>x.remainder(y.as_ref().unwrap(),&ctx),"remnear"=>x.remainder_near(y.as_ref().unwrap(),&ctx),
  "quantize"=>x.quantize(y.as_ref().unwrap(),&ctx),"fma"=>x.fma(y.as_ref().unwrap(),z.as_ref().unwrap(),&ctx),
  "scaleb"=>x.scaleb(y.as_ref().unwrap(),&ctx),"logb"=>x.logb(&ctx),"plus"=>x.plus(&ctx),
  "nextplus"=>x.next_plus(&ctx),"nextminus"=>x.next_minus(&ctx),"nexttoward"=>x.next_toward(y.as_ref().unwrap(),&ctx),
  "rti"=>x.round_to_integral_value(&ctx),"rtix"=>x.round_to_integral_exact(&ctx),"reduce"=>x.reduce(&ctx),
  "shift"=>x.shift(y.as_ref().unwrap(),&ctx),"rotate"=>x.rotate(y.as_ref().unwrap(),&ctx),
  _=>panic!("op")};
 let el = t.elapsed();
 let rs = r.to_string();
 let shown = if rs.len()>200 { format!("{}...{} (len {})", &rs[..80], &rs[rs.len()-60..], rs.len()) } else { rs };
 println!("{} [{:?}] elapsed={:.3?}", shown, s, el);
}
