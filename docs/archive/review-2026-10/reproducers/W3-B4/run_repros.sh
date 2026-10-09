#!/bin/zsh
# B4 reproducers. Build: CARGO_TARGET_DIR=<scratch>/target-B4 cargo build --release --manifest-path repro/Cargo.toml
B=${0:A:h}/../target-B4/release/dc; C=${0:A:h}/../target-B4/release/conv
echo "## Ziv cap: pow exact/midpoint results"
for m in Down Floor ZeroFiveUp HalfEven; do echo "pow(4,0.5) p9 $m: $($B pow 9 999 -999 $m 0 4 0.5)"; done
echo "pow(1.5625,0.5) p2 HalfUp: $($B pow 2 999 -999 HalfUp 0 1.5625 0.5)"
for m in HalfEven Ceiling Floor Up; do echo "pow(-1,100000) p9 $m: $($B pow 9 999999 -999999 $m 0 -1 100000)"; echo "pow(-1,100001) p9 $m: $($B pow 9 999999 -999999 $m 0 -1 100001)"; done
for m in HalfEven Down; do echo "pow(10,10001) p9 $m: $($B pow 9 99999 -99999 $m 0 10 10001)"; done
echo "pow(0.1,999999) p9 Ceiling: $($B pow 9 999999 -999999 Ceiling 0 0.1 999999)"
echo "exp(crafted 20000-digit x) p9: $($B exp 9 999 -999 HalfEven 0 @${0:A:h}/x_exp_mid.txt)"
echo "ln(crafted 20000-digit y) p9: $($B ln 9 999 -999 HalfEven 0 @${0:A:h}/y_ln_mid.txt)"
echo "## Context extremes"
echo "1E-2147483648 * 0.1 (p9, emin i32::MIN): $($B mul 9 2147483647 -2147483648 HalfEven 0 1E-2147483648 0.1)"
echo "1E-2147483648 / 10 (p9, emin i32::MIN): $($B div 9 2147483647 -2147483648 HalfEven 0 1E-2147483648 10)"
echo "next_plus(0) p2 emin i32::MIN: $($B nextplus 2 999 -2147483648 HalfEven 0 0)"
echo "next_plus(0) p1.2e9 emin -999999999: $($B nextplus 1200000000 999999999 -999999999 HalfEven 0 0)"
echo "sqrt(2) p2147483649: $($B sqrt 2147483649 999999999 -999999999 HalfEven 0 2)"
echo "sqrt(2) pU32MAX: $($B sqrt 4294967295 999999999 -999999999 HalfEven 0 2)"
echo "1/3 pU32MAX: $($B div 4294967295 999999999 -999999999 HalfEven 0 1 3)"
echo "## Panics"
$B fma 4294967295 2147483647 -2147483648 HalfEven 0 1E+2147483647 1E+2147483647 1E-2147483648 2>&1 | head -2
$B fma 4294967295 2147483647 -2147483648 HalfEven 0 1E+2147483647 1E+1 1E-2147483648 2>&1 | head -2
$C d128 NaN1234567890123456789012345678901234567890 2>&1 | head -2
$C d32 NaN1234567 2>&1 | head -2
echo "## divide_integer overflow"
echo "divint(11E+9,1) p16 emax9: $($B divint 16 9 -8 HalfEven 0 11E+9 1)"
echo "## clamp=1 ignored"
echo "reduce(1000E+381) p16 emax384 clamp1: $($B reduce 16 384 -383 HalfEven 1 1000E+381)"
echo "quantize(1E+370,1E+380) p16 emax384 clamp1: $($B quantize 16 384 -383 HalfEven 1 1E+370 1E+380)"
echo "## to_f64"
for x in 1 1E-310; do echo "to_f64($x) NE: $($C f64 $x)"; done; echo "to_f64(1E+400) TZ: $($C f64 1E+400 tz)"; echo "to_f64(-NaN): $($C f64 -NaN)"
echo "## pow integer_magnitude materialization"
/usr/bin/time -l $B pow 9 999999999 -999999999 HalfEven 0 2 1E+2147483647 2>&1 | egrep "elapsed|maximum res"
