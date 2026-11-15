# W2-A5 (reader A5): saturation gates, specials, the Newton primitives

Reader A5's driver and oracle, rerun by the coordinator with identical
output. `probe/` is a scratch crate with path dependencies on the
repository (relative paths; build with `CARGO_TARGET_DIR` outside the
tree): `drv` reads lines `fmt op args` (`d128|d64|d32`, e.g.
`d128 sinh 14150.05`) and prints the five mode cells `value#status` in
the order NE NA TZ TP TN; `newton` exercises the public
`Extended::recip`/`sqrt` at the range edges; `specials` prints the
§9.2.1 tables.

Oracle: `py/check.py` (mpmath at 60 to 130 dps, rounded per mode by
CPython `decimal` at the format's precision, range and clamp;
`/usr/bin/python3` without `-I`, mpmath 1.4.1 in the user site).

Key outputs:

- `repro_clean.txt`: the A5-1 (sinh/cosh return ±Infinity for finite
  results in `(exp overflow gate, ln(2·MAX)]`) and A5-2 (Decimal128
  `exp(-14221.3)` → +0 where `1E-6176` is correct) rows, all formats.
- `d32band_head.txt`: head and tail of the exhaustive Decimal32 band
  (439 inputs per sign per function, 4,390 of 4,390 cells wrong).
- `underflow_band.txt`: the d128 exp underflow window.
- `specials_pow_rows.txt`: the `pow(sNaN, ±0)`, `pow(1, sNaN)` and
  `pow(±0, −∞)` rows (A5-4, A5-5).

Beads: fd-b6zt (A5-1), fd-mq5e (A5-2); A5-3 is a row of fd-0o9.
