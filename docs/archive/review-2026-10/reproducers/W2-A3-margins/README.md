# W2-A3 (reader A3): the certification chain

- `true_margins.py`, `true_margins_lib.py`, `s_true.py`: Arb (python-flint
  via `~/.cache/pfloat-arb-oracle/venv`) true boundary margins for the
  frozen S2 "hardest 50" rows and for every S survivor; `tm_*.txt` are
  the per group outputs. Result (A3-2): the sin, cos and tan Decimal128
  labels are rung 1 mirror noise (true/label ratios up to 1.3e6); the
  README's "deepest certified margin 1.8·10^-11 ULP" input has a true
  margin of 2.33e-5 ULP (coordinator's own mpmath check agrees).
- `exp_red.py`: the `k·ln10` reduction error at 50 digits (A3-4):
  4.999e-46 at k = 5599, 49,908 predicate units measured against the
  catalog's itemized 22,400 (budget 250,000 holds).
- `plant.tsv`, `plant_flags.tsv`: planted rows for
  `tools/campaign_certify.py` (A3-5): wrong side values are rejected
  (misround=3); planted status cells `#00` and `#ff` pass (ok=5).
- `powi32_floor.py`: exhaustive Decimal32 `powi(x, −q)` boundary
  distances (A3-6; reader A2 ran the same enumeration independently and
  found the same minimum, 1.5e-15 at `c = 1867094`, `q = 4`).
- `predcheck.py`: 13,593 constructed cases for the escalation predicate
  against libmpdec (clean).
- `coordinator_rerun.log`: the certifier and `exp_red.py` reruns.
