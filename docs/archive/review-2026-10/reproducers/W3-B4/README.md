# W3-B4 (reader B4): `ferrodec-decimal`

`repro/` is a scratch crate (relative path dependency, features
`fmt,binary-float,interop`; build with `CARGO_TARGET_DIR` outside the
tree) with binaries `dc` (one operation under a context, timed:
`dc <op> <prec> <emax> <emin> <rounding> <clamp> <operands…>`, an
operand `@file` reads a long literal), `conv` (`to_f64` and the interop
bridge), `batch`, `conf` (an instrumented copy of the conformance
runner that lists every skip and every `power` row rescued by the one
ulp band) and `exp32`.

- `run_repros.sh`: every reproduced claim (B4-1 to B4-11) with
  `repros.out` as the reader's output. `run_safe.sh` is the same minus
  the two cases that need 5.7 GB and 16 GB resident (`fma` and `add`
  at `precision = u32::MAX − 1` and `u32::MAX`);
  `repros_coordinator_rerun.out` is the coordinator's rerun of it,
  identical to the reader's.
- `x_exp_mid.txt`, `y_ln_mid.txt`: the crafted 20,000 digit operands
  (`ln(1.000000005)` and `exp(1.000000005)` to 20,000 digits, mpmath at
  20,100 dps) that drive `exp` and `ln` to the Ziv cap (B4-4).
- `conf.out`: the instrumented decTest replay (27,494 pass, 0 fail,
  99 skip; the 55 `BAND` rows are B4-12).
- `py/diff_arith.py` (libmpdec, 270,000 structured cases over nine
  precisions, eight modes, both clamp settings) and `py/diff_transc.py`
  (mpmath, 10,843 cases to precision 10,000).
- `sites.txt`: the `as i32`/`as u32` wrap sites behind B4-5.
