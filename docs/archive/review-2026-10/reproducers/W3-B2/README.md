# W3-B2 (reader B2): the Decimal64 and Decimal32 siblings

Scratch crates carry relative path dependencies on the repository and
their own `[workspace]` table; build with `CARGO_TARGET_DIR` outside the
tree.

- `diff/`: `fmaw d64 NE 3000000000000001 2500000000000001 -1.00000001`
  prints `7.500000000000006E+30` (B2-1; exact value
  `7500000000000005499999999999999.99999999`, so `…005` is correct;
  `repro_B2-1.txt` has four witnesses, coordinator rerun identical);
  `b2diff N` is the structured differential against the validated
  oracle; `arithdump` with `acheck.py` and `b2diff N dump` with
  `qcheck.py` compare against libmpdec with CLAMPED; `c21` runs the
  C2-1 shape at the sibling widths; `expband` with `expband.py` sweeps
  the exp family gates against mpmath.
- `nanp/`: `b2nanp 128|64|32` prints 14,331 special value rows per
  format (`F op a b -> value statusbits`); `cmp3.py` and `rows.py`
  diff them across formats. The B2-2 to B2-7 rows were rerun by the
  coordinator (`abs_with_status -NaN7`, `logical_and NaN12 0`,
  `powr NaN12 sNaN3`, `scaleb Infinity 2147483647`, `div 0 Infinity`,
  `asin 0E+3`). `consts` and `mconst` pin constants by value.
- `nt/`: B2-8, `FromPrimitive::from_i128(10^20)` is `None` on the
  siblings.
- `hunks.py`, `showres.py`, `mirror_table.md`: the mirror hunk
  classifier behind the divergence table (regenerate the diffs with
  `docs/archive/review-2026-10/mirror_diff.sh <out-dir>`).
