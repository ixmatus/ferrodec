# W3-B1 (reader B1): the root `ferrodec` core

Scratch crates carry relative path dependencies on the repository and
their own `[workspace]` table; build with `CARGO_TARGET_DIR` outside the
tree. The release profile keeps debug assertions and overflow checks on.

- `bits/`: B1-1, the BID layout. `cargo run --release` prints the
  crate's `to_bits` beside the IEEE 754-2019 §3.5.2 pattern for the
  same value on all three formats (`1` → crate `0x2208…01`, IEEE
  `0x3040…01`; Decimal64 `0x2238…01` vs `0x31C0…01`; Decimal32
  `0x22500001` vs `0x32800001`) and shows what the crate decodes the
  IEEE pattern as. Coordinator rerun identical.
- `drv/`: `b1drv` reads `op mode operand…` lines (operands
  `F,sign,coef,exp`, `I,s`, `Q,s,payload`, `S,s,payload`, `B,hex`) and
  prints the result with the full `Status`; `orc` runs the same lines
  through the validated exact oracle. `repro_B1-2.txt` (fma OVERFLOW at
  the `10^6145` binade: driver `20`, oracle `16`), `repro_B1-3.txt`
  (scaleb GDA gate: NaN with INVALID for `n = 12357` and for `∞`),
  `repro_B1-4.txt` (non canonical NaN payloads survive `add` and order
  `total_cmp`). All three rerun by the coordinator.
- `py/`: `harness.py` (libmpdec at `prec=34, Emax=6144, Emin=-6143,
  clamp=1`, comparing value, cohort and six flags) and the sweeps
  (`sweep1.py`, `sweep2.py`, `sweep_sqrt.py`, `fam.py`, `zeros.py`,
  `clamp.py`, `emax.py`, `integ.py`, `fuzzbits.py`); about 1.9 million
  decided cases, mismatches only B1-2 and the known P1B-3.
