# P1B-3 (from C2-1): Decimal128 fma wrong by nine ULP under the directed modes

Finding: `src/ops/fma.rs:791-804`. When the exact product `a·b` is a
power of ten with 35 or more digits and an opposite sign addend sits
at least about 76 decades below it, the `ab_too_wide` path drops one
digit, leaves `kept = 10^33` with no round digit and no sticky, and
the downward correction `kept.sub(1)` produces 33 nines at the kept
quantum instead of 34 nines one quantum lower. `round_and_pack_finite`
pads the result to `999…990E0`, which is `10^34 − 10`; the correctly
rounded lower neighbour is `10^34 − 1`.

Witness: `fma(10, 1E33, -1E-102)` under TowardZero and TowardNegative
returns `9999999999999999999999999999999990` (INEXACT). The exact
value is strictly between `10^34 − 1` and `10^34` (rational check);
CPython `decimal` (libmpdec 2.5.0, precision 34, clamp 1) gives
`9999999999999999999999999999999999` with Inexact under ROUND_DOWN and
ROUND_FLOOR, and `1.000…E+34` under ROUND_CEILING and ROUND_HALF_EVEN,
which the shipped `fma` matches. Reader C2's family file (43,830
cases) has 3,366 mismatches, all Decimal128, all directed modes
(TowardZero 1,683, TowardNegative 1,122, TowardPositive 561), none on
Decimal64 or Decimal32, whose fma is separate code.

- `reader-C2-arith/`: the reader's differential driver (stdin lines of
  `format mode op operands`, compared against
  `ferrodec_test_support::oracle`), rerun by the coordinator:
  `printf '128 tz fma 10E0 1000000000000000000000000000000000E0 -1E-102\n' | arith`.
- `cmp.py`: the reader's rational oracle with its own rounding.
- `fma_family_head.txt` and `fma_family_count.txt`: the head and the
  line count of the generated family (the full file is regenerable
  from the reader's `gen*.py` scripts in the session scratchpad; the
  shape is the witness, not the file).

Coordinator's independent check (`/usr/bin/python3`):

```
from decimal import Decimal, Context, ROUND_DOWN, ROUND_FLOOR
ctx = Context(prec=34, rounding=ROUND_DOWN, Emax=6144, Emin=-6143, clamp=1)
ctx.fma(Decimal(10), Decimal(10**33), Decimal("-1E-102"))   # 9999999999999999999999999999999999
```

Why the stack missed it: the fma property strategy draws coefficients
uniformly and never produces an exact power of ten product with a far
opposite sign addend under a directed mode; the fd-7nf regression pins
the divides-evenly facet at a near gap; dqFMA has no vector of the
shape; Kani is special value only.

Closure: when `kept == 10^(PRECISION − 1)` and the rounding goes toward
the lower magnitude, deliver `10^PRECISION − 1` at `kept_q − 1`; a
regression row for the witness and its sign and mode mirrors; a
property strategy arm with powers of ten and all nines coefficients at
gaps of 110 digits and more.
