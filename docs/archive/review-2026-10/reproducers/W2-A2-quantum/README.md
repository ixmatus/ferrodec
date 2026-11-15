# W2-A2 (reader A2): zero row cohorts and the Decimal32 Newton error

- `quantum/`: cohort probe for the exact deliveries of `powi`, `rootn`,
  `compound`, `hypot`, `rsqrt` (round to nearest even); `quantum.txt`
  is its output and `quantum_check.py` judges each row against the
  §9.2.2 preferred quantum computed from the exact value. Result
  (A2-1): the zero operand rows of `powi`, `rootn` and `compound`
  return canonical `0`/`-0`/`1` at exponent 0 on all three formats
  (seven BAD rows per format); every nonzero exact row and `hypot`'s
  zero rows pass (187 rows).
- `newton/`: public `Extended::recip`/`sqrt`/rsqrt at rung 1 over
  200,000 deterministic operands per format. Result (A2-2): Decimal32
  `recip` 8.5e-26 and `sqrt` 4.0e-26 relative against itemizations of
  15 units (1.5e-49).
- `coordinator_rerun.log`: both binaries rerun with identical output.

Both crates carry relative path dependencies on the repository; build
with `CARGO_TARGET_DIR` outside the tree.
