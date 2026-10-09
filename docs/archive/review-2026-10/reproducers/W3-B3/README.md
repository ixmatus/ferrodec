# W3-B3 (reader B3): the I/O boundary on all three formats

`probe/` is a scratch crate (relative path dependencies on the three
format crates, features `fmt,binary-float,serde,num-traits,ops,dpd`;
build with `CARGO_TARGET_DIR` outside the tree). `b3probe` reads lines
`P <fmt> <mode> <literal>` (parse), `B <fmt> <mode> <hex>` (from_bits
through every formatter and `canonicalize`), `F2D <fmt> <mode> <f64 hex>`
(`from_f64`), `D2F <fmt> <mode> <literal>` (`to_f64`), `SERDE …`;
`bidjson`, `nt`, `nanprop`, `engp`, `sumz` are one shot probes. `py/`
holds the differentials against CPython `decimal` (`parse_diff.py`,
`int_diff.py`, `f2d_diff.py`, `d2f_diff.py`, `fmt_diff.py`,
`bits_fuzz.py`, `dpd_check.py`) and the timer (`timeit.py`).

Coordinator reruns (identical to the reader's): `P 64 TZ 1E-1000001`
and the Decimal32 twin take about 2.6 s of CPU per parse against
0.003 s on Decimal128 (B3-1); `engp` prints `988.00E-18` for
`98765E-20` under `{:.2}` on all three formats (B3-2); `F2D 64 NE
43c957633e24d15b` gives `3.652074584957696E+18` where the exact binary
value `3652074584957695488` rounds to `…695` (B3-4); the sibling
parsers accept `NaN1000000000000000` and `sNaN1048575` as non canonical
NaNs (B3-5); `serde_bid` JSON of a Decimal128 fails to deserialize
(B3-6); a `from_bits` NaN with payload `2^110 − 1` prints a payload
that does not parse back (B3-7).
