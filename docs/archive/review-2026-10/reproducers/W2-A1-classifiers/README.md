# W2-A1 (reader A1): the exact and tie classifiers

`a1_probe.rs` is a test module appended to a copy of
`ferrodec-transcend/src`; it implements the copy's `DecimalFormat` for
wrappers `W128`/`W64`/`W32` by forwarding to the real formats, so the
`pub(crate)` classifiers in `exact.rs` and `exact_pi.rs` run against
the shipped rounders (about 60 lines; the in-tree closure for C1-11 and
C1-13). `gen*.py` construct the cases per family (width gate edges,
ties at P+1 digits, trailing zero cohorts, subnormal and out of range
results, the pow boundary, compound at |n|·w = 196/197, hypot families,
the pi residue edges); `oracle2.py` computes true values by integer
arithmetic from the definitions with rounding and flags by CPython
`decimal` at `prec = P`, the format's `Emax`/`Emin`, `clamp = 1` (a tie
means ROUND_HALF_UP ≠ ROUND_HALF_DOWN); `check2.py` compares. The
oracle was validated on planted errors first.

Result: 339,012 direct classifier calls and 323,000 public API calls
(default and `force_escalate`+`force_adjudicate` builds), zero wrong
`Some`, zero wrong value or flag, zero missed exact or tie. Blind spot:
cohort and CLAMPED were not compared (reader A2's W2-A2-quantum probe
covers cohorts for the algebraic group's exact rows).
