# W3-B5 (reader B5): ferrodec-multiword and ferrodec-ieee

Scratch crates carry relative path dependencies on the repository and
their own `[workspace]` table; build with `CARGO_TARGET_DIR` outside
the tree. Python 3 integers are the oracle for the multiword
arithmetic; mpmath 1.4.1 (user site, `/usr/bin/python3` without `-I`)
for the constant generators.

- `bc/`: driver for the `bigconst` generators. B5-1:
  `b5bc inv_pi 99977` panics ("100001 digits requested, outside the
  supported range 8..=100000") inside the documented contract; the
  coordinator reran 99977 and 100000 with the same panic.
- `mw/src/bin/uf.rs`: B5-2, `Decimal128("1E-310").to_f64` reports
  `inexact=true underflow=false` for a subnormal result (coordinator
  rerun identical); `edge.rs` in release: `U256::isqrt(2^255)` returns
  `(0, self)` (B5-4), `div_rem_u128(_, 0)`, `DecBig 5 − 7` and
  `decimal_is_binary_exact(2^126+1, 1, 127)` return silent wrong values
  (B5-5).
- `cov/` with `covgen.py` and `decbig_counter_instrumentation.diff`
  (counters at the D3 decrement and the D6 add back in a copy of
  `decbig.rs`): the in tree witness fires the add back once, random
  operands never (683,645 quotient limbs), the Knuth family 21,398
  times, all matching Python divmod (`covres_counts.txt`). The
  coordinator's own Algorithm D simulation in Python agrees on the
  witness.
- `gen_check.py`, `directed.py`, `dbgen.py`: the fixed width and
  `DecBig` differentials (3,613,327 + 5,707 + 664,800 cases, zero
  mismatches).
- `maxcheck.py`, `maxdepth_log.txt`: every generator at its cap against
  mpmath, diff 0.
