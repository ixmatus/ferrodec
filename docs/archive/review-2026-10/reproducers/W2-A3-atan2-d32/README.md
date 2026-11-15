# W2-A3 (reader A3): Decimal32 atan2 wrong values through the subnormal Newton seed

`search/` enumerates `(y, x)` pairs with `x` in `(1E96, MAX32]` whose
`atan(y/x)` lies within the rung 1 reciprocal's error (about 1e-20
relative, from the five digit subnormal seed `1/x`) of a Decimal32
rounding boundary (exact integer fractional part of `y/x` in ULP units
plus an f64 cubic term); `search_hits_all.txt` is its output (six
witnesses). `verify_atan2.py` runs the witnesses through the public
`atan2` on all five modes via `probe/` and compares with Arb
(python-flint in `~/.cache/pfloat-arb-oracle/venv`). `newton32.py`
measures the seed precision; `atan2_search.py` is the Python
prototype of the search.

Coordinator rerun: the six witnesses reproduce (14 wrong input×mode
pairs); the `unbounded-ladder` build returns the same wrong values.

Bead: fd-h3jk. The atan2 anchor seam lead from the same reader
(directed modes return the exact ratio itself for `|y/x| ≲ 1.7e-55`)
reproduces with `printf 'd128 atan2 1E-6000 1\n'` through the W2-A5
driver and is bead fd-gu75.
