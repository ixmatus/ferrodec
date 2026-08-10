#!/usr/bin/env python3
"""Offline Arb/FLINT frozen hard-to-round vector generator for the
ferrodec-transcend surface (Phase 2 of fd-cb6, ADR-0026).

This is a *build tool*, not a project dependency. It is never a
workspace member and never enters the Cargo graph: Arb/FLINT (LGPL)
stay entirely outside the build. Its output — `tests/vectors/transcend/`
— is mathematical fact (the correctly-rounded value of a transcendental
at a chosen argument), checked in as data with a provenance companion,
exactly the standing the decTest vectors already have.

Why Arb is a proof, not a sample (ADR-0026). Arb computes a *certified
ball enclosure*: an interval guaranteed to contain the true value. The
argument is built as an exact rational, so the only error is Arb's own
tracked rounding. Round-half-even to p significant decimal digits is
monotone, so when the lower and upper bounds round to the *same*
p-digit value, *every* real in the enclosure — and therefore the true
value — rounds to that value. The correctly-rounded result is then
*established*, not estimated. Working precision is raised until the
enclosure is decisive (capped); for trig it is also raised by the
argument's binary magnitude so Arb's internal argument reduction
survives the cancellation in the decades the fixed astro-float oracle
skips. A decimal Table-Maker's-Dilemma worst-case search keeps the
arguments whose true value sits pathologically near a decimal
half-ULP (the cases most likely to separate faithful from correctly
rounded).

Run (offline, on a machine with FLINT 3 / python-flint):

    python3 tools/gen_transcend_vectors.py

Deterministic: a fixed seed and fixed counts, outputs sorted, so a
re-run reproduces the corpus byte-for-byte (the Phase 2 verification).
"""

import math
import os
import random
import sys
from collections import defaultdict
from fractions import Fraction

# Endpoint Fractions carry binary denominators up to 2^CAP_BITS
# (~19.7k digits); the decimal-exponent bracketing stringifies them.
# Lift CPython's 4300-digit int→str guard well past that. The guard
# (and this call) exist only on CPython >= 3.11; the no-pip self-test
# runs on older interpreters too and never builds those huge ints.
if hasattr(sys, "set_int_max_str_digits"):
    sys.set_int_max_str_digits(200_000)

def _require_flint():
    """Import python-flint (FLINT 3 / Arb) lazily and bind the symbols
    the generator uses as module globals. Deferred so the no-pip
    `--selftest` path runs on a bare interpreter: only corpus
    *generation* needs Arb, the checked-in vectors do not."""
    global arb, ctx, fmpq
    try:
        from flint import arb as _arb, ctx as _ctx, fmpq as _fmpq
    except Exception as exc:  # pragma: no cover - offline tool
        sys.stderr.write(
            "python-flint (FLINT 3 / Arb) is required: %s\n"
            "This is an offline build tool; install python-flint to "
            "regenerate the frozen corpus. The checked-in vectors do "
            "not need it.\n" % exc
        )
        sys.exit(2)
    arb, ctx, fmpq = _arb, _ctx, _fmpq

OUT_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "tests",
    "vectors",
    "transcend",
)

# IEEE 754-2019 interchange parameters per format.
FORMATS = {
    "d32": dict(prec=7, emax=96, emin=-95),
    "d64": dict(prec=16, emax=384, emin=-383),
    "d128": dict(prec=34, emax=6144, emin=-6143),
}

SEED = 0x_F_D_C_B_6
CAP_BITS = 1 << 16  # 65536-bit ceiling before declaring a case TMD-hard
TMD_SCAN = 300      # candidate args per (function, format) in the search
TMD_KEEP = 40       # hardest decisive cases kept from the scan (NE)
TMD_SCAN_DIRECTED = 200   # scan per (func, format, directed mode)
TMD_KEEP_DIRECTED = 12    # hardest decisive directed cases kept
TMD_SCAN_BINARY = 240     # 2-D (x, y) scan per (binary func, fmt, mode)
TMD_KEEP_BINARY = 14      # hardest decisive binary cases kept

# fd-97a. The four directed IEEE modes; NearestEven is the historical
# corpus default and stays unary-and-everything. The directed scan is
# bounded to the kernel primitives plus the widest-blast-radius derived
# (the correlated-failure argument: a primitive bias propagates, so the
# directed boundary matters most there); the other unary functions stay
# NearestEven-only in the proof tier (still covered by metamorphic +
# differential). pow/atan2 are binary and carry NearestEven + directed.
DIRECTED_MODES = (
    "TowardZero",
    "TowardPositive",
    "TowardNegative",
    "NearestAway",
)
MODES_ALL = ("NearestEven",) + DIRECTED_MODES
# ADR-0059 D1 appends logp1/log2p1/log10p1 AFTER the legacy names:
# pass 2 shares one sequential rng across this tuple, so appending
# keeps the legacy directed draws (and corpus bytes) stable. The D1
# functions carry directed rows because their classifier-adjacent
# neighborhoods (inputs beside the exact families) are exactly where
# directed-mode defects have bitten before (the ADR-0047 post-hoc
# cbrt/pow failures).
DIRECTED_FUNCS = (
    "exp", "ln", "sin", "cos", "atan", "cbrt", "log10",
    "logp1", "log2p1", "log10p1",
    "expm1", "exp2m1", "exp10", "exp10m1",
    # ADR-0060 Track D group D3: appended after every legacy name so
    # the shared sequential rng leaves the legacy directed draws (and
    # corpus bytes) untouched. rsqrt's directed risk profile is the
    # classifier-adjacent neighborhoods, same as the D1/D2 families.
    "rsqrt",
    # ADR-0061 D4: classifier-adjacent neighborhoods are the directed
    # risk profile, as for every family since D1.
    "sinpi", "cospi", "tanpi", "asinpi", "acospi", "atanpi",
)
BINARY = ("atan2", "pow", "hypot", "atan2pi")  # atan2pi: ADR-0061 D4
# ADR-0060 D3: the integer-operand surface (format x i32). A fourth
# pass with its own rng stream; vector lines carry the i32 as the
# second input token.
INT_BINARY = ("powi", "rootn", "compound")
# Independent rng streams so the NearestEven corpus content is
# byte-stable (the directed/binary passes do not perturb its sequence);
# all three are seeded deterministically off the one SEED.
SEED_DIRECTED = SEED ^ 0xD1EC7ED
SEED_BINARY = SEED ^ 0xB17A12
SEED_INTOP = SEED ^ 0x14700D3

# ADR-0033 corpus-integrity gate. A `solve` / `solve_binary` call that
# does not become decisive within `CAP_BITS` Arb working precision is
# silently dropped from the scan (the worst-case-keeper sorts only the
# decisive returns), so a TMD-hard candidate at the cap vanishes from
# the corpus without trace. The counter below records every such drop;
# the end-of-run summary in `emit` asserts the total is zero and exits
# non-zero otherwise, so a silent corpus loss is impossible.
_CAP_HITS = defaultdict(int)  # (name, fmt_label, mode) -> count
_FMT_LABEL = {id(v): k for k, v in FORMATS.items()}


def _record_cap_hit(name, fmt, mode, args_str):
    """A `solve*` call exhausted `CAP_BITS` Arb precision without the
    enclosure becoming decisive. Increment the per-(function, format,
    mode) cap-hit counter and emit a single stderr line naming the
    candidate so the corpus-integrity assert in `emit` can name what
    was lost. ADR-0033."""
    fmt_label = _FMT_LABEL.get(id(fmt), "?")
    _CAP_HITS[(name, fmt_label, mode)] += 1
    sys.stderr.write(
        "cap-hit: %s %s %s %s bits=%d\n"
        % (name, fmt_label, mode, args_str, CAP_BITS)
    )


def _is_directed_exact_output_unary(name, coef, exp, neg, fmt):
    """ADR-0033 directed-mode exact-output filter. Some hand-picked
    candidates from `representative()` have a mathematically exact,
    format-representable result (e.g. log10(10^k) = k). Under directed
    rounding (TowardZero/Positive/Negative) the certified Arb ball
    straddles the exact grid value at every precision: the lower
    endpoint rounds to the adjacent grid point below, the upper
    endpoint rounds to the exact point itself, so the rounded endpoint
    pair stays distinct regardless of how tight Arb gets and
    `_decisive` cannot resolve. `solve` would then run to `CAP_BITS`
    on every directed call for these candidates and the corpus
    integrity assert in `emit` would fire on a trivially-known
    result. Filter them out of the directed scan up front; the NE
    corpus (which has no straddle issue) still covers them, so the
    coverage loss is zero. Detection is by mathematical identity, not
    by Arb endpoint analysis (the latter is unsound: a genuine
    TMD-hard candidate just below a grid boundary would look
    identical, and we would not want to hide that)."""
    if name == "log10" and coef == 1 and not neg:
        # log10(10^exp) = exp, an integer, exactly representable as
        # long as the integer's magnitude fits the format's exponent
        # range and digit count.
        return abs(exp) <= fmt["emax"]
    # ADR-0059 D1 exact families (the input-side classifier's sets,
    # re-derived here independently as the generator-side mirror):
    # log2p1 is exact iff 1+x = 2^k, i.e. x = 2^k - 1 (odd integer,
    # stripped exp 0) or x = -(10^m - 5^m)·10^-m; log10p1 is exact
    # iff 1+x = 10^k, i.e. x = k nines or x = -(1 - 10^-m). logp1
    # has no nonzero exact case (Lindemann), so it needs no filter.
    if name == "log2p1":
        if not neg and exp == 0:
            n = coef + 1
            return n & (n - 1) == 0
        if neg and exp < 0:
            m = -exp
            return m <= fmt["prec"] and coef == 10 ** m - 5 ** m
        return False
    if name == "log10p1":
        if not neg and exp == 0:
            return coef == 10 ** len(str(coef)) - 1
        if neg and exp < 0:
            m = -exp
            return m <= fmt["prec"] and coef == 10 ** m - 1
        return False
    if name == "rsqrt":
        k = _rsqrt_exact_kind(coef, exp, neg, fmt)
        return k == "exact"
    if name in M1EXP:
        n = _as_int(coef, exp)
        if n is None:
            return False
        if name == "exp10":
            # 10^n is exactly representable across the whole exponent
            # range (coefficient 1, down to etiny = emin - (prec - 1)),
            # so every in-range integer input is an exact-output
            # directed straddle.
            return fmt["emin"] - (fmt["prec"] - 1) <= n <= fmt["emax"]
        if name == "exp10m1":
            # 10^n - 1 is the |n| nines pattern: exact iff |n| <= prec.
            return abs(n) <= fmt["prec"]
        if name == "exp2m1":
            # 2^n - 1 (odd) exact iff it fits prec digits; the negative
            # side's coefficient 10^m - 5^m has exactly m digits.
            if n > 0:
                return len(str(2 ** n - 1)) <= fmt["prec"] if n <= 130 else False
            if n < 0:
                return -n <= fmt["prec"]
        # expm1: only x = 0 is exact, and the enumerator's coef >= 1
        # never produces it.
        return False
    return False


def _is_directed_exact_output_binary(name, xt, yt, fmt):
    """ADR-0033 directed-mode exact-output filter (binary surface).
    The companion to `_is_directed_exact_output_unary`; see that
    function's docstring for the rationale. The candidates filtered
    are the exact-output pairs from `binary_representative('pow')`
    that produce a representable result in the target format."""
    if name == "hypot":
        return _hypot_exact_kind(xt, yt, fmt) == "exact"
    if name == "atan2pi":
        # ADR-0061 D4: the diagonal family |y| = |x| is exact
        # (atan2pi = +-1/4 or +-3/4, representable in every format);
        # its ball sits ON a directed-mode boundary and never becomes
        # decisive. The exact_pi classifier owns it; the corpus must
        # skip it or trip the cap-hit gate.
        (xc, xe, _xn) = xt
        (yc, ye, _yn) = yt
        def _strip(c, e):
            while c % 10 == 0 and c != 0:
                c //= 10
                e += 1
            return (c, e)
        return _strip(xc, xe) == _strip(yc, ye)
    if name != "pow":
        return False
    (xc, xe, xn) = xt
    (yc, ye, yn) = yt
    if xn or ye != 0:
        return False  # not a clean integer y on a positive x
    # pow(1.25, -1) = 0.8 exact (dyadic 4/5 = 8/10, representable in
    # every format).
    if (xc, xe) == (125, -2) and yc == 1 and yn:
        return True
    # pow(1.234567, k) for small positive integer k. The result has
    # at most 7·k significant digits, so it is exactly representable
    # whenever 7·k ≤ format precision.
    if (xc, xe) == (1_234_567, -6) and not yn and yc >= 1:
        return 7 * yc <= fmt["prec"]
    return False


def _as_int(coef, exp):
    """The exact integer value of coef·10^exp, or None if fractional
    or too wide to matter (|n| beyond 10^7 is far outside every
    exact family)."""
    if exp < 0:
        c, e = coef, exp
        while c % 10 == 0 and e < 0:
            c //= 10
            e += 1
        if e < 0:
            return None
        coef, exp = c, e
    if exp > 7:
        return None
    return coef * 10 ** exp


def _is_undecidable_tie(name, coef, exp, neg, fmt):
    """ADR-0059 D2: exp2m1's nearest-mode ties. The true value IS a
    rounding midpoint, so a certified ball straddles it at every Arb
    precision in every mode: no corpus row can exist. The input-side
    classifier owns these (delivered through the format rounder's tie
    rule) and explicit test vectors pin them; the scan must skip them
    or trip the cap-hit integrity assert."""
    if name == "rsqrt":
        return _rsqrt_exact_kind(coef, exp, neg, fmt) == "tie"
    if name != "exp2m1":
        return False
    n = _as_int(coef, exp)
    if n is None:
        return False
    prec = fmt["prec"]
    if neg:
        return n == prec + 1  # value coefficient 10^(p+1) - 5^(p+1)
    return (
        n % 4 == 0
        and n <= 130
        and len(str(2 ** n - 1)) == prec + 1
        and (2 ** n - 1) % 10 == 5
    )


def _count_factor(a, f):
    c = 0
    while a % f == 0:
        a //= f
        c += 1
    return c, a


def _strip10(c):
    """Strip trailing decimal zeros; returns (stripped, count)."""
    z = 0
    while c % 10 == 0 and c != 0:
        c //= 10
        z += 1
    return c, z


def _coeff_exact_kind(c, p):
    """'exact' / 'tie' / None for a positive stripped coefficient at
    format precision p (ADR-0060 D3: the generator-side mirror of the
    classifiers' P+1 width gate; ties end in 5)."""
    w = len(str(c))
    if w <= p:
        return "exact"
    if w == p + 1 and c % 10 == 5:
        return "tie"
    return None


def _kind_with_range(c, e10, fmt):
    """Width-gate a stripped coefficient, then range-check by kind:
    an exact value must itself be representable; a TIE is a midpoint
    (P+1 digits by definition, never representable), so its range
    check is on the adjusted exponent of the straddled neighborhood.
    Without this split the tie families would leak into the scan and
    spin to the cap (the pow(5, 49) shape caught it in the probe)."""
    kind = _coeff_exact_kind(c, fmt["prec"])
    if kind is None:
        return None
    if kind == "exact":
        return "exact" if representable(c, e10, fmt) else None
    adj = decimal_magnitude(c, e10)
    return "tie" if fmt["emin"] <= adj <= fmt["emax"] else None


def _rsqrt_exact_kind(coef, exp, neg, fmt):
    """ADR-0060 D3 rsqrt classifier mirror: 1/sqrt(a*10^u) is rational
    iff the stripped coefficient is 2^v2*5^v5 (no other prime) and both
    halved exponents v2+u, v5+u are even; the value's stripped
    coefficient is then a pure power of 2 or 5 (the 5-power side is the
    tie family). Exact integer arithmetic; complete by construction."""
    if neg:
        return None
    a, z = _strip10(coef)
    u = exp + z
    v2, r = _count_factor(a, 2)
    v5, r = _count_factor(r, 5)
    if r != 1:
        return None
    A, B = v2 + u, v5 + u
    if A % 2 or B % 2:
        return None
    i, j = A // 2, B // 2
    if i >= j:
        c = 5 ** (i - j)
        e10 = -i
    else:
        c = 2 ** (j - i)
        e10 = -j
    return _kind_with_range(c, e10, fmt)


def _int_nth_root(t, b):
    """Largest r with r^b <= t, by binary search (exact). The
    bit-length guard mirrors exact.rs's nth_root_u128: a root >= 2
    needs 2^b <= t, so past t.bit_length() the answer is 1 without
    forming any power — without it the search computes `2 ** b` for
    b up to 2^31, a hundreds-of-millions-digit integer (the pure
    Python stall the third full run died of)."""
    if t < 2:
        return t
    if b >= t.bit_length():
        return 1
    lo, hi = 1, 1 << (t.bit_length() // b + 2)
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if mid ** b <= t:
            lo = mid
        else:
            hi = mid - 1
    return lo


def _frac_exact_kind(f, fmt):
    """'exact' / 'tie' / None for an exact Fraction result (ADR-0060
    D3 powi/compound filter). Terminating decimals only; the stripped
    coefficient decides through the shared width gate, and the value
    must be representable (range included)."""
    num = abs(f.numerator)
    if num == 0:
        return None
    a2, d = _count_factor(f.denominator, 2)
    a5, d = _count_factor(d, 5)
    if d != 1:
        return None
    m = max(a2, a5)
    c = num * 2 ** (m - a2) * 5 ** (m - a5)
    c, z = _strip10(c)
    return _kind_with_range(c, z - m, fmt)


def _int_exact_kind(name, xt, n, fmt):
    """'exact' / 'tie' / None for the integer-operand D3 ops. Exact
    integer/Fraction arithmetic throughout; the |n|*log10(2) width
    lower bound bails before any huge power is formed (a stripped
    base coefficient >= 2 makes the result wider than P+1 once
    |n| > (P+1)/log10(2), so nothing exact or tie is lost)."""
    (coef, exp, neg) = xt
    p = fmt["prec"]
    wide_n = abs(n) > int((p + 1) / 0.301) + 1
    if name == "powi":
        a, z = _strip10(coef)
        if a == 1:
            w = (exp + z) * n
            if representable(1, w, fmt):
                return "exact"
            return None
        if wide_n:
            return None
        return _frac_exact_kind(frac10(coef, exp) ** n, fmt)
    if name == "compound":
        base = 1 + (-1 if neg else 1) * frac10(coef, exp)
        if base <= 0:
            return None
        bn = abs(base.numerator)
        b_stripped, bz = _strip10(bn)
        if b_stripped == 1 and base.denominator == 1:
            # 1 + x = 10^k: the whole-range power-of-ten family.
            w = bz * n
            if representable(1, w, fmt):
                return "exact"
            return None
        if wide_n:
            return None
        return _frac_exact_kind(base ** n, fmt)
    if name == "rootn":
        b = abs(n)
        if b < 2:
            return None
        a, z = _strip10(coef)
        u = exp + z
        v2, r = _count_factor(a, 2)
        v5, r = _count_factor(r, 5)
        alpha, beta = v2 + u, v5 + u
        if alpha % b or beta % b:
            return None
        if r == 1:
            t_root = 1
        elif b >= r.bit_length():
            # s >= 2 would need s^b >= 2^b > r: no integer root, and
            # forming the power to prove it is the stall class.
            return None
        else:
            t_root = _int_nth_root(r, b)
            if t_root ** b != r:
                return None
        if n < 0 and t_root != 1:
            return None
        i, j = alpha // b, beta // b
        if n < 0:
            i, j = -i, -j
        k = min(i, j)
        c = t_root * 2 ** (i - k) * 5 ** (j - k)
        c, z2 = _strip10(c)
        return _kind_with_range(c, k + z2, fmt)
    return None


def frac10(coef, exp):
    """Exact rational coef·10^exp as an FLINT `fmpq` (arb accepts
    fmpq, not Python Fraction)."""
    if exp >= 0:
        return fmpq(coef * 10 ** exp)
    return fmpq(coef, 10 ** (-exp))


def frac_of_point(a):
    """Exact Fraction of an arb endpoint (a point ball), or None when
    the value is not finite (out of domain → NaN/Inf)."""
    try:
        m, e = a.man_exp()
    except Exception:
        return None
    m = int(m)
    e = int(e)
    return Fraction(m) * (Fraction(2) ** e if e >= 0 else Fraction(1, 2 ** (-e)))


def round_half_even_sig(v, p):
    """Round Fraction `v` to `p` significant decimal digits, ties to
    even. Returns (sign, digits:int with exactly p digits, exp10) so
    the value is sign·digits·10^exp10, or None for zero."""
    if v == 0:
        return None
    sign = -1 if v < 0 else 1
    a = -v if v < 0 else v
    # Bracket the decimal exponent E so a/10^E ∈ [10^(p-1), 10^p).
    E = len(str(a.numerator)) - len(str(a.denominator)) - (p - 1)
    for _ in range(4):
        scaled = a / (Fraction(10) ** E) if E >= 0 else a * (Fraction(10) ** (-E))
        n, d = scaled.numerator, scaled.denominator
        q, r = divmod(n, d)
        if q >= 10 ** p:
            E += 1
            continue
        if q < 10 ** (p - 1):
            E -= 1
            continue
        # round-half-even on the remainder r/d against 1/2
        two_r = 2 * r
        if two_r > d or (two_r == d and q % 2 == 1):
            q += 1
            if q == 10 ** p:
                q //= 10
                E += 1
        return (sign, q, E)
    return None


def round_directed_sig(v, p, mode):
    """Round Fraction `v` to `p` significant decimal digits under
    `mode` (NearestEven NearestAway TowardZero TowardPositive
    TowardNegative). Returns (sign, digits:int with exactly p digits,
    exp10) or None for zero. The directed modes round on the
    sign-aware magnitude: ceiling rounds a positive remainder up but
    truncates a negative one, floor mirrors it, truncation never
    increments, nearest-away sends an exact half away from zero. The
    Rust round_directed_sig runs the same shared case table."""
    if v == 0:
        return None
    sign = -1 if v < 0 else 1
    a = -v if v < 0 else v
    E = len(str(a.numerator)) - len(str(a.denominator)) - (p - 1)
    for _ in range(4):
        scaled = a / (Fraction(10) ** E) if E >= 0 else a * (Fraction(10) ** (-E))
        n, d = scaled.numerator, scaled.denominator
        q, r = divmod(n, d)
        if q >= 10 ** p:
            E += 1
            continue
        if q < 10 ** (p - 1):
            E -= 1
            continue
        two_r = 2 * r
        if mode == "NearestEven":
            up = two_r > d or (two_r == d and q % 2 == 1)
        elif mode == "NearestAway":
            up = two_r >= d
        elif mode == "TowardZero":
            up = False
        elif mode == "TowardPositive":
            up = sign > 0 and r != 0
        elif mode == "TowardNegative":
            up = sign < 0 and r != 0
        else:
            raise ValueError("unknown rounding mode %r" % (mode,))
        if up:
            q += 1
            if q == 10 ** p:
                q //= 10
                E += 1
        return (sign, q, E)
    return None


def directed_margin(v, p):
    """Distance (in fractional ULP) of `v` from the nearest p-digit
    *grid point* (a representable boundary). Small ⇒ the directed
    Table-Maker's-Dilemma worst case: the enclosure straddles an
    integer boundary, so a directed decision flips with its sign,
    unlike the half-ULP tie that `tie_margin` targets."""
    a = -v if v < 0 else v
    E = len(str(a.numerator)) - len(str(a.denominator)) - (p - 1)
    for _ in range(4):
        scaled = a / (Fraction(10) ** E) if E >= 0 else a * (Fraction(10) ** (-E))
        q = scaled.numerator // scaled.denominator
        if q >= 10 ** p:
            E += 1
            continue
        if q < 10 ** (p - 1):
            E -= 1
            continue
        frac = scaled - q
        return float(min(frac, 1 - frac))
    return 0.5


def tie_margin(v, p):
    """Distance (in fractional ULP) of `v` from the nearest decimal
    half-ULP tie of the p-digit grid. Small ⇒ pathologically hard to
    round (the Table-Maker's-Dilemma worst case)."""
    a = -v if v < 0 else v
    E = len(str(a.numerator)) - len(str(a.denominator)) - (p - 1)
    for _ in range(4):
        scaled = a / (Fraction(10) ** E) if E >= 0 else a * (Fraction(10) ** (-E))
        q = scaled.numerator // scaled.denominator
        if q >= 10 ** p:
            E += 1
            continue
        if q < 10 ** (p - 1):
            E -= 1
            continue
        frac = scaled - q
        return float(abs(frac - Fraction(1, 2)))
    return 0.5


def decimal_magnitude(coef, exp):
    return len(str(coef)) - 1 + exp


def representable(coef, exp, fmt):
    """The exact value coef·10^exp must be exactly representable in the
    format: at most `prec` significant digits (so `parse_str` does not
    re-round the argument out from under the proven output) and the
    adjusted exponent inside [emin, emax]. Trailing zeros do not count
    toward the significand."""
    c = coef
    while c % 10 == 0 and c != 0:
        c //= 10
    if len(str(c)) > fmt["prec"]:
        return False
    adj = decimal_magnitude(coef, exp)
    return fmt["emin"] <= adj <= fmt["emax"]


# --- function surface: name -> (arb computation, domain predicate) ---

FUNCS = {
    "exp": lambda a: a.exp(),
    "ln": lambda a: a.log(),
    "log2": lambda a: a.log() / arb(2).log(),
    "log10": lambda a: a.log() / arb(10).log(),
    "exp2": lambda a: arb(2) ** a,
    # Arb `root` rejects negatives; cbrt is odd, so solve() feeds |x|
    # and re-applies the sign.
    "cbrt": lambda a: a.root(3),
    # sqrt is the IEEE 754 §5 mandatory root. Its domain is x >= 0
    # (negatives are NaN, a special-value contract rather than a
    # hard-to-round case), so solve() needs no sign re-application,
    # unlike cbrt's odd-function branch.
    "sqrt": lambda a: a.sqrt(),
    "sin": lambda a: a.sin(),
    "cos": lambda a: a.cos(),
    "tan": lambda a: a.tan(),
    "asin": lambda a: a.asin(),
    "acos": lambda a: a.acos(),
    "atan": lambda a: a.atan(),
    "sinh": lambda a: a.sinh(),
    "cosh": lambda a: a.cosh(),
    "tanh": lambda a: a.tanh(),
    "asinh": lambda a: a.asinh(),
    "acosh": lambda a: a.acosh(),
    "atanh": lambda a: a.atanh(),
    # ADR-0059 Track D group D1 (IEEE 754-2019 §9.2 logp1/log2p1/
    # log10p1). Appended AFTER the legacy names: pass 1 and pass 2
    # share one sequential rng per pass, so appending keeps every
    # legacy function's draw sequence — and therefore the committed
    # legacy corpus bytes — stable under a full regeneration.
    # arb.log1p is the certified primitive (tighter balls near the
    # -1 pole than composing (1+x).log()).
    "logp1": lambda a: a.log1p(),
    "log2p1": lambda a: a.log1p() / arb(2).log(),
    "log10p1": lambda a: a.log1p() / arb(10).log(),
    # ADR-0059 Track D group D2 (§9.2 expm1/exp2m1/exp10/exp10m1),
    # appended after D1 for the same stream-stability reason.
    # arb.expm1 is the certified primitive; the base variants compose
    # over an exact-argument power (certified ball arithmetic
    # throughout; near-zero cancellation inflates the ball and the
    # solve loop widens until decisive).
    "expm1": lambda a: a.expm1(),
    "exp2m1": lambda a: arb(2) ** a - arb(1),
    "exp10": lambda a: arb(10) ** a,
    "exp10m1": lambda a: arb(10) ** a - arb(1),
    # ADR-0060 Track D group D3 (§9.2 rSqrt), appended after D2 for
    # the same stream-stability reason. arb.rsqrt is the certified
    # native primitive.
    "rsqrt": lambda a: a.rsqrt(),
    # ADR-0061 Track D group D4 (§9.2 sinPi..atanPi), appended after
    # D3 for the same stream-stability reason. The forward trio uses
    # Arb's native *_pi primitives (exact reduction inside the ball
    # arithmetic); the inverse three compose over a ball pi, which
    # keeps the enclosure honest and the solve loop decisive.
    "sinpi": lambda a: a.sin_pi(),
    "cospi": lambda a: a.cos_pi(),
    "tanpi": lambda a: a.tan_pi(),
    "asinpi": lambda a: a.asin() / arb.pi(),
    "acospi": lambda a: a.acos() / arb.pi(),
    "atanpi": lambda a: a.atan() / arb.pi(),
}

# Per-function decimal-magnitude window for the random TMD scan, and
# whether the argument is restricted (sign / unit interval / ≥1).
TRIG = {"sin", "cos", "tan"}
POSITIVE = {"ln", "log2", "log10", "rsqrt"}  # x > 0 (rsqrt: ADR-0060 D3)
UNIT = {"asin", "acos", "atanh", "asinpi", "acospi"}  # |x| < 1
# ADR-0061 D4: the forward pi trio. Bounded magnitude (large values
# are integers, classifier-exact, and an exact draw would spin the
# solve loop to the cap), and the exact residue classes (4x integral:
# integers, half and quarter integers) rejected for the same reason.
PIFWD = {"sinpi", "cospi", "tanpi"}
GE_ONE = {"acosh"}                          # x ≥ 1
NON_NEGATIVE = {"sqrt"}                      # x ≥ 0 (IEEE §5 sqrt)
P1LOG = {"logp1", "log2p1", "log10p1"}       # x > -1 (ADR-0059 D1)
M1EXP = {"expm1", "exp2m1", "exp10", "exp10m1"}  # exp family (ADR-0059 D2)


def in_domain(name, coef, exp, neg, fmt):
    mag = decimal_magnitude(coef, exp)
    if name in POSITIVE and neg:
        return False
    if name in P1LOG:
        # Domain x > -1 (§9.2 Table 9.1): a negative argument must
        # have |x| < 1; the enumerator's coefficient is >= 1 so
        # mag < 0 is exactly |x| < 1, never -1 itself. Positive
        # arguments are unrestricted.
        return (not neg) or mag < 0
    if name in UNIT:
        # |x| = coef·10^exp must be < 1.
        return (not neg or name == "atanh") and mag < 0
    if name in GE_ONE:
        return (not neg) and mag >= 0
    if name in NON_NEGATIVE:
        # sqrt: x >= 0. The enumerator's coefficient is always >= 1 so
        # x is never zero here; negatives are the NaN special-value
        # contract, not a TMD candidate, so they are excluded.
        return not neg
    if name in PIFWD:
        # Bounded away from the all-integer decades, and never an
        # exact residue: 4x integral covers every classifier-exact
        # class (integer, half integer, quarter integer), whose Arb
        # ball would never become decisive.
        if mag > 8:
            return False
        if exp >= 0:
            return False  # integer: classifier-exact
        scaled = 4 * coef
        e = exp
        while e < 0 and scaled % 10 == 0:
            scaled //= 10
            e += 1
        return e < 0  # still fractional after stripping: not a residue
    if name in ("exp", "exp2", "sinh", "cosh") or name in M1EXP:
        # ADR-0033 corpus-integrity gate fix. The prior `mag <= 3`
        # bound was sized for d128 (mag=3 means |x| < 10^4 which is
        # well inside d128's emax=6144), but the same predicate
        # serves d32 (emax=96) and d64 (emax=384) too, so it let
        # arguments like `cosh(560)` through at d32 where the true
        # result is ~10^243, vastly overflowing the format. `_decisive`
        # then returns None on "result out of range" and the solve
        # loop spins to `CAP_BITS = 65536` without ever resolving,
        # silently dropping a candidate that was never going to fit
        # the format in the first place. Bound the input by the
        # format's `emax`: |result| ≈ e^|x| for exp/sinh/cosh and
        # |result| ≈ 2^|x| for exp2, so the overflow boundary is
        # |x| ≈ emax·ln(10) for the first family and
        # |x| ≈ emax·log2(10) ≈ emax·ln(10)/ln(2) for exp2.
        # Bounded a touch below the boundary so the result is comfortably
        # in range (matches the prior comment's intent).
        if coef == 0:
            return True
        log10_abs_x = math.log10(coef) + exp
        # `e^|x| < 10^emax` ⟺ `|x| < emax·ln(10)` ⟺
        # `log10(|x|) < log10(emax·ln(10)) = log10(emax) + log10(ln(10))`.
        if name in ("exp2", "exp2m1"):
            limit_log10_x = math.log10(fmt["emax"] * math.log2(10))
        elif name in ("exp10", "exp10m1"):
            limit_log10_x = math.log10(fmt["emax"])
        else:
            limit_log10_x = math.log10(fmt["emax"] * math.log(10))
        # Reserve a small slack to keep the result one decade short of
        # the boundary; the corpus is interested in TMD-hard cases,
        # not overflow boundaries (those have a separate special-value
        # contract).
        return log10_abs_x <= limit_log10_x - 0.1
    return True


def representative(name):
    """Hand-picked representative + domain-boundary arguments
    (coef, exp, neg)."""
    base = [
        (2, 0, False), (3, 0, False), (5, 0, False), (7, 0, False),
        (15, -1, False), (1, 1, False), (1, 2, False),
        # ≤ 7 significant digits ⇒ exact in every format including
        # decimal32 (the fd-cb6 d32 acos finding); solve() also
        # enforces representability.
        (1_234_567, -6, False), (2_718_281, -6, False),
    ]
    if name == "rsqrt":
        # The POSITIVE probes plus perfect-square and near-square
        # neighborhoods (the classifier-adjacent directed band); the
        # exact rows themselves are filtered from the directed scan
        # and delivered by the input-side classifier.
        return base + [
            (4, 0, False), (25, -2, False), (625, -4, False),
            (1_048_576, 0, False),   # 2^20: exact, coefficient 2^10-side
            (1_000_001, -6, False), (999_999, -6, False),
            (5, -1, False), (1, -3, False), (1, 3, False),
            (4_000_001, -6, False),  # beside 4: rsqrt near 0.5
        ]
    if name in POSITIVE:
        return base + [
            (1_000_001, -6, False),  # just above 1 (ln near 0)
            (999_999, -6, False),    # just below 1
            (5, -1, False), (1, -3, False), (1, 3, False),
        ]
    if name in UNIT:
        return [
            (5, -1, False), (5, -1, True), (1, -1, False), (9, -1, False),
            (999_999, -6, False), (999_999, -6, True),
            (1, -3, False), (123_456, -6, False),
        ]
    if name in GE_ONE:
        return [
            (1_000_001, -6, False),  # 1+δ → log1p path
            (15, -1, False), (2, 0, False), (1, 1, False), (1, 3, False),
        ]
    if name in M1EXP:
        # ADR-0059 D2: both sides of zero (expm1 hugs x below; the
        # base variants scale by ln 2 / ln 10), the deep-negative -1
        # saturation approach, and near-one probes. Exact-family
        # inputs (exp10(k), exp10m1(±k) at integers) come from the
        # input-side classifiers, not the corpus; the directed scan
        # filters them.
        return base + [
            (5, -1, True), (25, -1, True),
            (1, -3, True), (1, -3, False),
            (1, 1, True),            # -10: e^x - 1 near -1
            (117, 0, True),          # deep in the -1 approach (expm1)
            (1_000_001, -6, False), (999_999, -6, True),
        ]
    if name in P1LOG:
        # ADR-0059 D1: both sides of zero (the direct log1p band and
        # the anchor seam), the near -1 pole, and the exact-family
        # neighborhoods (log2p1(3) = 2, log10p1(9) = 1 are exact; the
        # probes sit beside them, the exact rows themselves come from
        # the input-side classifier, not the corpus).
        return base + [
            (5, -1, True),           # -0.5 (log2p1 exact: value -1)
            (9, -1, True),           # -0.9 (log10p1 exact: value -1)
            (999_999, -6, True),     # deep in the -1 pole approach
            (123_456, -6, True),
            (1, -3, True), (1, -3, False),
            (1_000_001, -6, False), (999_999, -6, False),
        ]
    if name in TRIG:
        return base + [
            (1, 0, False), (1, 0, True), (1_570_796, -6, False),  # ≈ π/2
            (7, -1, False), (1, -4, False),
        ]
    if name in ("exp", "exp2", "sinh", "cosh", "tanh", "asinh"):
        return base + [(1, -3, False), (0 if False else 5, -1, True),
                       (25, -1, False), (1, 1, False)]
    return base


def decades(name, fmt):
    """Decade probes; for trig these deliberately reach the decades
    the fixed astro-float oracle skips (|x| ≫ 10^15), and (ADR-0033)
    now also reach the format's `emax` so the upper trig argument
    range is covered rather than implicitly trusting Payne-Hanek to
    hold past the prior 10^180 clamp."""
    out = []
    if name in P1LOG:
        # Large positive decades (logp1 tracks ln there) and tiny
        # decades on BOTH sides of zero: the anchor-seam band
        # (|x| ≲ 10^-47) and the escalation band just above it are
        # exactly where the D1 kernels' delivery machinery changes
        # regime, so the corpus pins rows across all of it.
        hi = fmt["emax"] - 4
        for k in [1, 3, 6, 9, 12, 15, 20, 30, 60, 120, hi]:
            if k <= hi:
                out.append((314_159, k - 6, False))
                out.append((271_828, -(k - 6), False))
                out.append((271_828, -(k - 6), True))
        return out
    if name in M1EXP:
        hi = fmt["emax"] - 4
        out = []
        for k in [1, 3, 6, 9, 12, 15, 20, 30, 60, min(120, hi)]:
            out.append((271_828, -(k - 6), False))
            out.append((271_828, -(k - 6), True))
        return out
    if name in TRIG or name in POSITIVE or name == "cbrt":
        # ADR-0033: lifted the prior `min(emax-4, 180)` clamp for
        # TRIG and the `min(emax-4, 300)` clamp for non-TRIG. The
        # 2/π table at `ferrodec-transcend/src/argred.rs` is 6300
        # digits; sized correctly for `Decimal128`'s emax=6144, so
        # the corpus now probes the table's full coverage range.
        # A TMD-hard candidate that the prior cap masked surfaces as
        # a cap-hit, caught by the corpus-integrity assert in `emit`.
        hi = fmt["emax"] - 4
        ks = [1, 3, 6, 9, 12, 15, 16, 20, 30, 60, 120]
        # Intermediate decades past the prior cap so the corpus
        # actually populates the upper range, rather than jumping
        # straight from 10^120 to `hi`.
        ks += [
            k for k in (180, 240, 300, 480, 720, 1200, 2400, 4800)
            if k <= hi
        ]
        ks.append(hi)
        for k in ks:
            if k <= hi:
                out.append((314_159, k - 6, False))
                if name == "cbrt":
                    out.append((314_159, -(k - 6), True))
                if name in POSITIVE:
                    out.append((271_828, -(k - 6), False))
    if name in PIFWD:
        # ADR-0061 D4: small-magnitude fractional decades (integers
        # and larger are classifier-exact and rejected by in_domain),
        # plus classifier-adjacent probes beside the quarter, half,
        # and integer residues, both signs, which is the family's
        # directed risk profile.
        for k in (-95, -30, -6, -1, 1, 3, 6):
            out.append((314_159, k - 6, False))
            out.append((314_159, k - 6, True))
        for coef, e in (
            (2_500_001, -7),  # just above 0.25
            (2_499_999, -7),  # just below 0.25
            (5_000_001, -7),  # just above 0.5
            (7_499_999, -7),  # just below 0.75
            (1_000_001, -6),  # just above 1
            (9_999_999, -7),  # just below 1
            (1_500_000_000_001, -12),  # beside 1.5
        ):
            out.append((coef, e, False))
            out.append((coef, e, True))
    return out


def _round_mode(v, p, mode):
    """Round endpoint `v` to `p` significant digits under `mode`. NE is
    the keystone round_half_even_sig; the directed modes go through
    round_directed_sig (both lockstep-tested by fd-tgg)."""
    if mode == "NearestEven":
        return round_half_even_sig(v, p)
    return round_directed_sig(v, p, mode)


def _margin_mode(v, p, mode):
    """TMD proximity under `mode`: distance to the nearest half-ULP tie
    (NE) or the nearest grid point (directed); small ⇒ hard to round."""
    if mode == "NearestEven":
        return tie_margin(v, p)
    return directed_margin(v, p)


def _decisive(b, fmt, mode):
    """If the Arb ball `b` rounds its lower and upper endpoints to the
    SAME value under `mode`, the correctly-rounded result is
    established (monotone rounding ⇒ every real in the enclosure, and
    so the true value, rounds there). Returns
    (out_s, rad, margin) or None (straddles a boundary, not finite, or
    the result is out of the format's range)."""
    p = fmt["prec"]
    lo = frac_of_point(b.lower())
    hi = frac_of_point(b.upper())
    if lo is None or hi is None:
        return None
    rl = _round_mode(lo, p, mode)
    rh = _round_mode(hi, p, mode)
    if rl is None or rl != rh:
        return None
    sign, q, E = rl
    ds = str(q)
    adj = E + (len(ds) - 1)
    # The correctly-rounded result must itself be in the format's
    # range; an overflow/underflow has no single representable value
    # to freeze.
    if not (fmt["emin"] <= adj <= fmt["emax"]):
        return None
    out_s = "%s%s.%se%d" % ("-" if sign < 0 else "", ds[0], ds[1:], adj)
    try:
        rad = float(b.rad()) if hasattr(b, "rad") else float(hi - lo)
    except Exception:
        rad = float(hi - lo)
    return (out_s, rad, _margin_mode(lo, p, mode))


def solve(name, fn, coef, exp, neg, fmt, mode="NearestEven",
          cap_bits=CAP_BITS, record_cap_hits=True, domain_check=True):
    """Smallest Arb precision at which f(coef·10^exp) is decisive at
    the format precision under `mode`. Returns
    (output_str, P_bits, radius, margin) or None (out of domain, input
    not exactly representable, result out of range, or TMD-hard past
    `cap_bits`). The NearestEven path is numerically identical to the
    fd-cb6 generator.

    `cap_bits` defaults to the module-level `CAP_BITS` for the
    corpus-generator's existing behaviour; the ADR-0033 Slice B
    exhaustive sweep tool overrides it to run a cheap fixed-precision
    pre-screen (tier 1, low cap) before promoting narrow-margin
    survivors to the full variable-precision pass (tier 2, full cap).

    `record_cap_hits` defaults to True for the corpus generator's
    integrity assert. The Slice B tier-1 pre-screen passes False
    because a tier-1 non-decisive return means "promote to tier 2,"
    not "silent corpus loss"; recording would conflate the two."""
    p = fmt["prec"]
    # `domain_check=False` is the ADR-0059 campaign certifier's path:
    # `in_domain`'s exp-family slack keeps the CORPUS one decade short
    # of the overflow boundary, but the S1 probe targets exactly that
    # band, and the certifier only submits rows whose production
    # outputs are already finite (it routes any non-finite output to
    # its own overflow-boundary verdict before calling here), so
    # `_decisive`'s own range rejection is the protection that
    # remains load bearing. Default behaviour is unchanged.
    if domain_check and not in_domain(name, coef, exp, neg, fmt):
        return None
    # The argument must be exact in the format, else `parse_str`
    # re-rounds it and ferrodec computes f of a different value than
    # the proven output (the fd-cb6 d32 acos finding).
    if not representable(coef, exp, fmt):
        return None
    x = frac10(coef, exp)
    if neg:
        x = -x
    mag = abs(decimal_magnitude(coef, exp))
    # trig argument reduction loses ~3.33 bits per decimal digit of |x|
    extra = int(mag * 3.34) + 16 if name in TRIG else 16
    P = 64 + extra
    while P <= cap_bits:
        ctx.prec = P
        try:
            if name == "cbrt" and neg:
                # cbrt is odd; Arb `root` needs a nonnegative base.
                b = -fn(arb(-x))
            else:
                b = fn(arb(x))
        except Exception:
            return None
        d = _decisive(b, fmt, mode)
        if d is not None:
            out_s, rad, margin = d
            return (out_s, P, rad, margin)
        if frac_of_point(b.lower()) is None or frac_of_point(b.upper()) is None:
            return None
        P *= 2
    if record_cap_hits:
        _record_cap_hit(
            name, fmt, mode,
            "coef=%s%d exp=%d" % ("-" if neg else "", coef, exp),
        )
    return None


# --- binary surface: pow(x, y) and atan2(y, x) (fd-97a) ---
#
# pow's hard-to-round structure is in y·ln x, so its working precision
# is boosted by the magnitudes of *both* operands; atan2 cancels near
# the axes, the same way trig argument reduction does. Each is one
# certified Arb call (no composed reconstruction that would inject
# extra rounding into the proof).
BIN_FUNCS = {
    "pow": lambda ax, ay: ax ** ay,
    "atan2": lambda ax, ay: arb.atan2(ay, ax),  # atan2(s=y, t=x)
    # ADR-0060 D3: hypot in certified ball composition (no native
    # arb.hypot in python-flint 0.6): squares and sqrt of balls keep
    # the enclosure honest; the solve loop widens until decisive.
    "hypot": lambda ax, ay: (ax * ax + ay * ay).sqrt(),
    # ADR-0061 D4: atan2 scaled by a ball pi; certified composition.
    "atan2pi": lambda ax, ay: arb.atan2(ay, ax) / arb.pi(),
}


def _hypot_exact_kind(xt, yt, fmt):
    """ADR-0060 D3 hypot classifier mirror: with quanta aligned at
    q = min(xe, ye), S = (xc*10^(xe-q))^2 + (yc*10^(ye-q))^2 is an
    exact integer; hypot is exact-or-tie iff S is a perfect square
    whose stripped root passes the shared width gate (sqrt of a
    non-square integer is irrational, Niven). Complete by
    construction; signs are irrelevant (hypot is even)."""
    (xc, xe, _xn) = xt
    (yc, ye, _yn) = yt
    q = min(xe, ye)
    sx = xc * 10 ** (xe - q)
    sy = yc * 10 ** (ye - q)
    S = sx * sx + sy * sy
    w = math.isqrt(S)
    if w * w != S:
        return None
    c, z = _strip10(w)
    return _kind_with_range(c, q + z, fmt)


def _is_undecidable_tie_binary(name, xt, yt, fmt):
    """Binary-surface tie exclusion (the pass-3 mirror of
    _is_undecidable_tie): a true value ON a nearest-mode midpoint
    never becomes decisive at any Arb precision in any mode."""
    if name != "hypot":
        return False
    return _hypot_exact_kind(xt, yt, fmt) == "tie"


def bin_in_domain(name, xt, yt):
    (xc, _xe, xn) = xt
    if name == "pow":
        # Real pow needs a positive base (x > 0); the scan/representor
        # only ever offers x ≥ 0, this rejects a zero coefficient.
        return (not xn) and xc != 0
    # atan2 is defined everywhere except (0, 0), and hypot is total on
    # finite pairs; coefficients are ≥ 1, so the pair is never the
    # origin.
    return True


def solve_binary(name, fn2, xt, yt, fmt, mode,
                 cap_bits=CAP_BITS, record_cap_hits=True):
    """Smallest Arb precision at which f(x, y) is decisive under
    `mode`. Returns (output_str, P_bits, radius, margin) or None.
    `cap_bits` and `record_cap_hits` mirror the unary `solve`; see
    its docstring for the ADR-0033 Slice B tier-1 / tier-2 rationale.
    Binary cap-hits at Slice B scope are out of scope (the binary
    surface stays on the ADR-0026 sampled corpus path per ADR-0033)."""
    (xc, xe, xn) = xt
    (yc, ye, yn) = yt
    if not bin_in_domain(name, xt, yt):
        return None
    if not (representable(xc, xe, fmt) and representable(yc, ye, fmt)):
        return None
    if name == "pow":
        # Reject wildly out-of-range scan samples *before* any Arb
        # call: pow(x, y) ≈ 10^(y·log10 x), and an enormous result
        # would build a multi-million-digit exact Fraction endpoint
        # that the rounding helper cannot stringify. The Arb loop and
        # _decisive still enforce exact in-range representability.
        # Log-space arithmetic: the earlier `xc * (10.0 ** xe)` float
        # form raised OverflowError for |xe| past ~290, which the
        # corpus's moderate decades never drew but the ADR-0059
        # campaign's edge strip does. Same rejection semantics.
        if xn or xc == 0:
            return None
        log10_x = math.log10(xc) + xe
        if log10_x != 0.0:
            approx_log10 = math.log10(yc) + ye + math.log10(abs(log10_x))
            if approx_log10 > math.log10(fmt["emax"] + 8):
                return None
    x = frac10(xc, xe)
    if xn:
        x = -x
    y = frac10(yc, ye)
    if yn:
        y = -y
    xmag = abs(decimal_magnitude(xc, xe))
    ymag = abs(decimal_magnitude(yc, ye))
    # pow: error ~ enters through y·ln x; atan2: axis cancellation.
    extra = int((xmag + ymag) * 3.34) + 32
    P = 64 + extra
    while P <= cap_bits:
        ctx.prec = P
        try:
            b = fn2(arb(x), arb(y))
        except Exception:
            return None
        d = _decisive(b, fmt, mode)
        if d is not None:
            out_s, rad, margin = d
            return (out_s, P, rad, margin)
        if frac_of_point(b.lower()) is None or frac_of_point(b.upper()) is None:
            return None
        P *= 2
    if not record_cap_hits:
        return None
    _record_cap_hit(
        name, fmt, mode,
        "x=(coef=%s%d exp=%d) y=(coef=%s%d exp=%d)"
        % ("-" if xn else "", xc, xe, "-" if yn else "", yc, ye),
    )
    return None


def binary_representative(name):
    """Fixed decimal-exact (x_triple, y_triple) anchor pairs (≤ 7
    significant digits ⇒ exact in every format; solve_binary also
    enforces representability)."""
    if name == "pow":
        xs = [
            (2, 0, False), (3, 0, False), (15, -1, False),
            (5, -1, False), (125, -2, False), (1_234_567, -6, False),
        ]
        ys = [
            (2, 0, False), (3, 0, False), (5, -1, False),
            (15, -1, False), (1, 0, True), (25, -1, False),
        ]
        return [(x, y) for x in xs for y in ys]
    if name == "hypot":
        pairs = [
            # Pythagorean anchors (exact; filtered from the directed
            # scan, kept for NE where they are decisive).
            ((3, 0, False), (4, 0, False)),
            ((5, 0, False), (12, 0, False)),
            ((20, 0, False), (21, 0, False)),
            ((119, 0, False), (120, 0, False)),
            ((3, -1, False), (4, -1, False)),
            ((3, 10, False), (4, 10, False)),
            # Near-Pythagorean and generic neighborhoods.
            ((3, 0, False), (5, 0, False)),
            ((1, 0, False), (1, 0, False)),
            ((2, 0, True), (3, 0, False)),
            ((1_234_567, -6, False), (7_654_321, -7, False)),
            # The S = k^2 + 1 and k^2 + k grid/midpoint-hugging
            # families (ADR-0060's near-attaining shapes).
            ((1_000_000, 0, False), (1, 0, False)),
            ((1_000_000, 0, False), (1_000, 0, False)),
            ((999_999, 0, False), (1, 0, False)),
            # Magnitude-ratio ladder across every format's anchor-band
            # edge (delta0 = 18 / 9 / 5): ratios 1e-3 .. 1e-25.
            ((314_159, -5, False), (271_828, -8, False)),
            ((314_159, -5, False), (271_828, -10, False)),
            ((314_159, -5, False), (271_828, -13, False)),
            ((314_159, -5, False), (271_828, -14, False)),
            ((314_159, -5, False), (271_828, -22, False)),
            ((314_159, -5, False), (271_828, -23, False)),
            ((314_159, -5, False), (271_828, -24, False)),
            ((314_159, -5, False), (271_828, -30, False)),
            ((314_159, 100, False), (271_828, 70, False)),
        ]
        return pairs
    pts = [
        (1, 0, False), (1, 0, True), (2, 0, False),
        (5, -1, False), (15, -1, False), (1_234_567, -6, False),
    ]
    return [(a, b) for a in pts for b in pts]


# --- integer-operand surface: powi(x, n), rootn(x, n), compound(x, n)
# (ADR-0060 Track D D3). The second operand is an i32, exact by type;
# vector lines carry it as a plain integer token. ---

INT_FUNCS = {
    # x arrives as an exact fmpq; arb(x) is one certified conversion.
    # Integer powers and roots are certified arb primitives; compound's
    # base 1 + x is formed EXACTLY in fmpq before the one conversion
    # (the width-collapse discipline: no ball-arithmetic cancellation
    # near 1, and no stringify-then-round instrument anywhere).
    "powi": lambda x, n: arb(x) ** n,
    "rootn": lambda x, n: _arb_rootn(x, n),
    "compound": lambda x, n: arb(1 + x) ** n,
}


def _arb_rootn(x, n):
    """x^(1/n) as a certified ball. arb's native `root` is used only
    for small |n|: python-flint 0.6's `root(n)` degrades to minutes
    per call once |n| reaches the 10^8 range (measured: 66 minutes at
    n = 936,680,469 — the second stall class the first full run hid
    behind the underflow spin). Past the threshold the composition
    `exp(ln(x)/n)` is ball arithmetic end to end, so the enclosure
    stays honest and the solve loop's widening handles the slightly
    fatter ball; the threshold keeps the tight native path where the
    corpus's small-order roots (the delegation and adjudication
    ranges) live."""
    b = abs(n)
    if b <= 4096:
        return arb(x).root(b) if n > 0 else 1 / arb(x).root(b)
    v = arb(x).log() / n
    return v.exp()


def _compound_base(xt):
    (xc, xe, xn) = xt
    x = frac10(xc, xe)
    if xn:
        x = -x
    return 1 + x


def int_in_domain(name, xt, n, fmt):
    """Domain and range pre-rejection for the integer-operand scan.
    |n| < 2 is excluded everywhere: n = 0 and n = ±1 are the delegation
    rows (identity / reciprocal / one), not TMD candidates. Overflow
    and deep-underflow results are rejected before any Arb call (they
    have a §7.4 special-value contract, not a rounding to prove; an
    out-of-range power-of-ten would otherwise spin to the cap)."""
    (xc, xe, xn) = xt
    if xc == 0 or abs(n) < 2:
        return False
    if name == "powi":
        # BOTH range walls, a safe margin inside: `_decisive` cannot
        # freeze a result outside the format's normal range (it
        # rejects the enclosure at every precision), so a result past
        # either wall spins the Arb ladder to the cap. The first full
        # run demonstrated it: powi d32 draws with negative `n` landed
        # results ~10^-144, forty decades below emin = -95, and each
        # burned the whole 65,536-bit ladder before cap-hitting. The
        # estimates are within a decade; the two-decade margin absorbs
        # them.
        est = n * (math.log10(xc) + xe)
        return fmt["emin"] + 2 <= est <= fmt["emax"] - 2
    if name == "rootn":
        # Even-n negative bases are the NaN contract; odd-n negatives
        # exercise the sign-reflection path and stay.
        return (not xn) or (n % 2 != 0)
    if name == "compound":
        base = _compound_base(xt)
        if base <= 0:
            return False
        # Digit-length log10 estimate (within one decade; the slack
        # absorbs it). fmpq exposes numerator/denominator as p/q.
        # The digit-length estimate is only usable when it is large;
        # for bases within a few decades of 1 it reads 0 while
        # n·log10(base) can be thousands (the compound d32 cap-hit
        # row: base 1.2407844, n = 14594, true est 1367). Use a float
        # logarithm when the base fits a float, the digit-length form
        # only in the astronomic regime where it is accurate.
        # (math.log10 takes arbitrarily large ints natively; base > 0
        # was established above, so p and q are positive.)
        log10_base = math.log10(int(base.p)) - math.log10(int(base.q))
        est = n * log10_base
        return fmt["emin"] + 2 <= est <= fmt["emax"] - 2
    return False


def solve_int(name, fni, xt, n, fmt, mode,
              cap_bits=CAP_BITS, record_cap_hits=True):
    """Smallest Arb precision at which f(x, n) is decisive under
    `mode`. Mirrors solve_binary; the i32 operand is exact, so the
    precision driver is the |n|-fold relative-error amplification of
    the powering (log2|n| extra bits), plus the usual slack."""
    (xc, xe, xn) = xt
    if not int_in_domain(name, xt, n, fmt):
        return None
    if not representable(xc, xe, fmt):
        return None
    x = frac10(xc, xe)
    extra = 32 + 2 * int(math.log2(abs(n)))
    P = 64 + extra
    while P <= cap_bits:
        ctx.prec = P
        try:
            if name == "compound":
                if xn:
                    x_signed = -x
                else:
                    x_signed = x
                b = fni(x_signed, n)
            elif xn:
                # powi/rootn odd-n negative base: compute the positive
                # magnitude and re-apply the sign (both are odd there;
                # even-n powi results are positive).
                mag = fni(x, n)
                b = -mag if n % 2 != 0 else mag
            else:
                b = fni(x, n)
        except Exception:
            return None
        d = _decisive(b, fmt, mode)
        if d is not None:
            out_s, rad, margin = d
            return (out_s, P, rad, margin)
        if frac_of_point(b.lower()) is None or frac_of_point(b.upper()) is None:
            return None
        P *= 2
    if record_cap_hits:
        _record_cap_hit(
            name, fmt, mode,
            "x=(coef=%s%d exp=%d) n=%d" % ("-" if xn else "", xc, xe, n),
        )
    return None


def int_representative(name):
    """Hand-picked (x_triple, n) anchors: the classifier-adjacent
    neighborhoods, the delegation seams (|n| = 6 vs 7), the odd-n
    negative-base path, the i32 extremes on near-1 bases (the hug-at-1
    anchor band), and the exact families (kept for the nearest modes,
    filtered from the directed scan)."""
    if name == "powi":
        return [
            ((2, 0, False), 2), ((2, 0, False), 3), ((2, 0, False), 5),
            ((2, 0, False), 6), ((2, 0, False), 7), ((2, 0, False), 49),
            ((2, 0, False), 112), ((2, 0, False), 113),
            ((3, 0, True), 3), ((3, 0, True), 2),
            ((15, -1, False), 3), ((2, -1, False), 2),
            ((1_234_567, -6, False), 3), ((7, 0, False), -2),
            ((2, 0, False), -3), ((1_000_001, -6, False), 1_048_576),
            ((999_999, -6, False), -12_345),
            ((2_718_281, -6, False), 1_000), ((5, 0, False), 48),
        ]
    if name == "rootn":
        return [
            ((8, 0, False), 3), ((27, 0, True), 3), ((2, 0, False), 2),
            ((2, 0, False), 5), ((5, 0, False), 2), ((7, 0, False), 4),
            ((1_234_567, -6, False), 7), ((2, 0, False), -2),
            ((8, 0, False), -3), ((1_000_001, -6, False), 2_147_483_647),
            ((999_999, -6, False), -2_147_483_647),
            ((314_159, -5, False), 1_000), ((2, 0, False), 113),
            ((32, 0, False), 5), ((1, 30, False), 5), ((625, -4, False), 4),
        ]
    return [
        ((5, -2, False), 12), ((5, -2, False), 360),
        ((1, -3, False), 365), ((9, 0, False), 2), ((99, 0, False), 5),
        ((25, -2, True), 4), ((999_999, -6, True), 3),
        ((1, -40, False), 3), ((1, -40, False), 2_147_483_647),
        ((271_828, -6, False), 100), ((5, -1, True), 7),
        ((1_234_567, -6, False), 30), ((3, -1, False), -12),
        ((1, -1, True), 24), ((5, -2, False), -60),
    ]


def _draw_int_pair(rng, name, p):
    """One scan draw for pass 4. n mixes the powering-arm range, the
    moderate band, and the deep band up to the i32 edge; x favors the
    near-1 neighborhoods for powi/compound (the pow hard band, and the
    only region where a huge |n| survives the range check)."""
    r = rng.random()
    if r < 0.45:
        n = rng.randrange(2, 10)
    elif r < 0.75:
        n = rng.randrange(10, 400)
    elif r < 0.92:
        n = rng.randrange(400, 30_000)
    else:
        n = rng.randrange(30_000, 2 ** 31)
    if rng.random() < 0.5:
        n = -n
    if name == "powi":
        if rng.random() < 0.3:
            xt = (10_000_000 + rng.randrange(-99_999, 100_000), -7, False)
        else:
            xt = _scan_arg(rng, p, -3, 3, True)
    elif name == "rootn":
        xt = _scan_arg(rng, p, -6, 7, True)
        if xt[2] and n % 2 == 0:
            n += 1 if n > 0 else -1
    else:
        if rng.random() < 0.4:
            xt = _scan_arg(rng, p, -45, -5, True)
        else:
            xt = _scan_arg(rng, p, -4, 3, True)
    return xt, n


def _scan_arg(rng, p, lo, hi, signed):
    coef = rng.randrange(1, 10 ** min(p, 8))
    exp = rng.randrange(lo, hi)
    neg = signed and (rng.random() < 0.5)
    return (coef, exp, neg)


def emit(only=None):
    _require_flint()
    os.makedirs(OUT_DIR, exist_ok=True)
    # `only` (the --funcs flag): restrict generation and file writes
    # to the named functions, leaving every other committed corpus
    # file untouched. This is the ADR-0059 Track D accretion mode: a
    # new function's vectors are generated without re-running (or
    # re-writing) the frozen legacy corpus, so a python-flint version
    # skew cannot silently churn committed bytes outside the slice.
    unary_names = [n for n in FUNCS if only is None or n in only]
    directed_names = [n for n in DIRECTED_FUNCS if only is None or n in only]
    binary_names = [n for n in BINARY if only is None or n in only]
    int_names = [n for n in INT_BINARY if only is None or n in only]
    # name -> list of entry tuples
    #   (fmt_idx, mode_idx, sortkey, prec, mode, in_s, in2_s, out, P,
    #    rad, margin)
    # in2_s is None for the unary functions.
    acc = {name: [] for name in list(FUNCS) + list(BINARY) + list(INT_BINARY)}
    fmt_order = {k: i for i, k in enumerate(FORMATS)}
    mode_idx = {m: i for i, m in enumerate(MODES_ALL)}

    # --- Pass 1: NearestEven, unary. The legacy candidate/scan path
    # with the legacy rng, so the NE corpus content is byte-stable
    # (only the new mode token and the headers differ); the directed
    # and binary passes draw from independent streams below so they
    # cannot perturb this one. ---
    rng = random.Random(SEED)
    for name in unary_names:
        fn = FUNCS[name]
        for fkey, fmt in FORMATS.items():
            p = fmt["prec"]
            cand = []
            for (coef, exp, neg) in representative(name) + decades(name, fmt):
                cand.append((coef, exp, neg))
            scanned = []
            for _ in range(TMD_SCAN):
                coef = rng.randrange(1, 10 ** min(p, 8))
                if name in UNIT:
                    exp = -(len(str(coef)))  # |x| < 1
                    neg = (name == "atanh") and (rng.random() < 0.5)
                elif name in GE_ONE:
                    exp = rng.randrange(0, 6)
                    neg = False
                elif name in POSITIVE:
                    exp = rng.randrange(-6, 7)
                    neg = False
                elif name in ("exp", "exp2", "sinh", "cosh"):
                    exp = rng.randrange(-4, 4)
                    neg = rng.random() < 0.5
                elif name in P1LOG:
                    # Both sides of zero; in_domain rejects the
                    # negative draws with |x| >= 1 inside solve().
                    exp = rng.randrange(-8, 7)
                    neg = rng.random() < 0.5
                elif name in M1EXP:
                    exp = rng.randrange(-8, 4)
                    neg = rng.random() < 0.5
                else:
                    exp = rng.randrange(-6, 9)
                    neg = rng.random() < 0.5
                if _is_undecidable_tie(name, coef, exp, neg, fmt):
                    continue
                r = solve(name, fn, coef, exp, neg, fmt)
                if r is not None:
                    scanned.append((r[3], coef, exp, neg))
            scanned.sort(key=lambda t: (t[0], t[1], t[2], t[3]))
            for (_m, coef, exp, neg) in scanned[:TMD_KEEP]:
                cand.append((coef, exp, neg))

            seen = set()
            for (coef, exp, neg) in cand:
                key = (coef, exp, neg)
                if key in seen:
                    continue
                seen.add(key)
                if _is_undecidable_tie(name, coef, exp, neg, fmt):
                    continue
                r = solve(name, fn, coef, exp, neg, fmt)
                if r is None:
                    continue
                out_s, P, rad, margin = r
                in_s = "%s%de%d" % ("-" if neg else "", coef, exp)
                acc[name].append((
                    fmt_order[fkey], mode_idx["NearestEven"],
                    (exp, coef, neg), p, "NearestEven", in_s, None,
                    out_s, P, rad, margin,
                ))

    # --- Pass 2: directed modes, the bounded unary subset. ---
    rng_d = random.Random(SEED_DIRECTED)
    for name in directed_names:
        fn = FUNCS[name]
        for mode in DIRECTED_MODES:
            for fkey, fmt in FORMATS.items():
                p = fmt["prec"]
                cand = []
                for (coef, exp, neg) in representative(name) + decades(name, fmt):
                    # ADR-0033 directed-mode exact-output filter.
                    if mode != "NearestAway" and _is_directed_exact_output_unary(
                        name, coef, exp, neg, fmt
                    ):
                        continue
                    cand.append((coef, exp, neg))
                scanned = []
                for _ in range(TMD_SCAN_DIRECTED):
                    coef = rng_d.randrange(1, 10 ** min(p, 8))
                    if name in POSITIVE:
                        exp = rng_d.randrange(-6, 7)
                        neg = False
                    elif name == "exp":
                        exp = rng_d.randrange(-4, 4)
                        neg = rng_d.random() < 0.5
                    elif name in P1LOG:
                        exp = rng_d.randrange(-8, 7)
                        neg = rng_d.random() < 0.5
                    elif name in M1EXP:
                        exp = rng_d.randrange(-8, 4)
                        neg = rng_d.random() < 0.5
                    else:
                        exp = rng_d.randrange(-6, 9)
                        neg = rng_d.random() < 0.5
                    if _is_undecidable_tie(name, coef, exp, neg, fmt):
                        continue
                    if mode != "NearestAway" and _is_directed_exact_output_unary(
                        name, coef, exp, neg, fmt
                    ):
                        continue
                    r = solve(name, fn, coef, exp, neg, fmt, mode)
                    if r is not None:
                        scanned.append((r[3], coef, exp, neg))
                scanned.sort(key=lambda t: (t[0], t[1], t[2], t[3]))
                for (_m, coef, exp, neg) in scanned[:TMD_KEEP_DIRECTED]:
                    cand.append((coef, exp, neg))

                seen = set()
                for (coef, exp, neg) in cand:
                    key = (coef, exp, neg)
                    if key in seen:
                        continue
                    seen.add(key)
                    if _is_undecidable_tie(name, coef, exp, neg, fmt):
                        continue
                    if mode != "NearestAway" and _is_directed_exact_output_unary(
                        name, coef, exp, neg, fmt
                    ):
                        continue
                    r = solve(name, fn, coef, exp, neg, fmt, mode)
                    if r is None:
                        continue
                    out_s, P, rad, margin = r
                    in_s = "%s%de%d" % ("-" if neg else "", coef, exp)
                    acc[name].append((
                        fmt_order[fkey], mode_idx[mode],
                        (exp, coef, neg), p, mode, in_s, None,
                        out_s, P, rad, margin,
                    ))

    # --- Pass 3: binary pow/atan2, NearestEven + directed, 2-D TMD. ---
    rng_b = random.Random(SEED_BINARY)
    for name in binary_names:
        fn2 = BIN_FUNCS[name]
        for mode in MODES_ALL:
            for fkey, fmt in FORMATS.items():
                p = fmt["prec"]
                # ADR-0033 directed-mode exact-output filter (binary).
                cand = [
                    (xt, yt)
                    for (xt, yt) in binary_representative(name)
                    if not _is_undecidable_tie_binary(name, xt, yt, fmt)
                    and (
                        mode in ("NearestEven", "NearestAway")
                        or not _is_directed_exact_output_binary(name, xt, yt, fmt)
                    )
                ]
                scanned = []
                for _ in range(TMD_SCAN_BINARY):
                    if name == "pow":
                        xt = _scan_arg(rng_b, p, -3, 4, False)
                        yt = _scan_arg(rng_b, p, -2, 3, True)
                    elif name == "hypot":
                        # Half the draws pull the second operand deep
                        # below the first, crossing every format's
                        # anchor-band edge (ADR-0060's two-band split).
                        xt = _scan_arg(rng_b, p, -6, 7, True)
                        if rng_b.random() < 0.5:
                            yt = _scan_arg(rng_b, p, -30, 7, True)
                        else:
                            yt = _scan_arg(rng_b, p, -6, 7, True)
                    else:
                        xt = _scan_arg(rng_b, p, -6, 7, True)
                        yt = _scan_arg(rng_b, p, -6, 7, True)
                    if _is_undecidable_tie_binary(name, xt, yt, fmt):
                        continue
                    if mode not in ("NearestEven", "NearestAway") and \
                            _is_directed_exact_output_binary(name, xt, yt, fmt):
                        continue
                    r = solve_binary(name, fn2, xt, yt, fmt, mode)
                    if r is not None:
                        scanned.append((r[3], xt, yt))
                scanned.sort(key=lambda t: (t[0], t[1], t[2]))
                for (_m, xt, yt) in scanned[:TMD_KEEP_BINARY]:
                    cand.append((xt, yt))

                seen = set()
                for (xt, yt) in cand:
                    if (xt, yt) in seen:
                        continue
                    seen.add((xt, yt))
                    if _is_undecidable_tie_binary(name, xt, yt, fmt):
                        continue
                    if mode not in ("NearestEven", "NearestAway") and \
                            _is_directed_exact_output_binary(name, xt, yt, fmt):
                        continue
                    r = solve_binary(name, fn2, xt, yt, fmt, mode)
                    if r is None:
                        continue
                    out_s, P, rad, margin = r
                    (xc, xe, xn) = xt
                    (yc, ye, yn) = yt
                    in_s = "%s%de%d" % ("-" if xn else "", xc, xe)
                    in2_s = "%s%de%d" % ("-" if yn else "", yc, ye)
                    acc[name].append((
                        fmt_order[fkey], mode_idx[mode],
                        (xe, xc, xn, ye, yc, yn), p, mode, in_s,
                        in2_s, out_s, P, rad, margin,
                    ))

    # --- Pass 4: the integer-operand D3 surface (ADR-0060), NearestEven
    # + directed, 2-D TMD over (x, n). Its own rng stream; the exact and
    # tie families are filtered by the exact-integer mirrors of the
    # input-side classifiers (complete by construction), so no
    # undecidable candidate can reach the cap-hit assert. ---
    rng_i = random.Random(SEED_INTOP)
    for name in int_names:
        fni = INT_FUNCS[name]
        for mode in MODES_ALL:
            for fkey, fmt in FORMATS.items():
                p = fmt["prec"]
                cand = []
                for (xt, n) in int_representative(name):
                    kind = _int_exact_kind(name, xt, n, fmt)
                    if kind == "tie":
                        continue
                    if kind == "exact" and mode not in ("NearestEven", "NearestAway"):
                        continue
                    cand.append((xt, n))
                scanned = []
                for _ in range(TMD_SCAN_BINARY):
                    xt, n = _draw_int_pair(rng_i, name, p)
                    kind = _int_exact_kind(name, xt, n, fmt)
                    if kind == "tie":
                        continue
                    if kind == "exact" and mode not in ("NearestEven", "NearestAway"):
                        continue
                    r = solve_int(name, fni, xt, n, fmt, mode)
                    if r is not None:
                        scanned.append((r[3], xt, n))
                scanned.sort(key=lambda t: (t[0], t[1], t[2]))
                for (_m, xt, n) in scanned[:TMD_KEEP_BINARY]:
                    cand.append((xt, n))

                seen = set()
                for (xt, n) in cand:
                    if (xt, n) in seen:
                        continue
                    seen.add((xt, n))
                    kind = _int_exact_kind(name, xt, n, fmt)
                    if kind == "tie":
                        continue
                    if kind == "exact" and mode not in ("NearestEven", "NearestAway"):
                        continue
                    r = solve_int(name, fni, xt, n, fmt, mode)
                    if r is None:
                        continue
                    out_s, P, rad, margin = r
                    (xc, xe, xn) = xt
                    in_s = "%s%de%d" % ("-" if xn else "", xc, xe)
                    acc[name].append((
                        fmt_order[fkey], mode_idx[mode],
                        (xe, xc, xn, n), p, mode, in_s,
                        str(n), out_s, P, rad, margin,
                    ))

    for name in unary_names + binary_names + int_names:
        entries = sorted(acc[name], key=lambda e: (e[0], e[1], e[2]))
        vec_lines = []
        prov_lines = []
        for (_fi, _mi, _sk, p, mode, in_s, in2_s, out_s, P, rad, margin) in entries:
            label = "margin" if mode == "NearestEven" else "boundary"
            if in2_s is None:
                vec_lines.append("%d %s %s %s" % (p, mode, in_s, out_s))
                prov_lines.append(
                    "%d %s %s P=%d rad=%.3e %s=%.3e decisive"
                    % (p, mode, in_s, P, rad, label, margin)
                )
            else:
                vec_lines.append(
                    "%d %s %s %s %s" % (p, mode, in_s, in2_s, out_s)
                )
                prov_lines.append(
                    "%d %s %s %s P=%d rad=%.3e %s=%.3e decisive"
                    % (p, mode, in_s, in2_s, P, rad, label, margin)
                )

        with open(os.path.join(OUT_DIR, name + ".txt"), "w") as f:
            f.write(
                "# Arb/FLINT certified frozen vectors for `%s` "
                "(ADR-0026, fd-cb6; directed + binary fd-97a).\n"
                "# Unary line: <prec> <mode> <input> "
                "<correctly-rounded-output>.\n"
                "# Binary line (pow, atan2, hypot): <prec> <mode> <input1> "
                "<input2> <output>.\n"
                "# Integer-operand line (powi, rootn, compound): <prec> "
                "<mode> <input> <n:i32> <output>.\n"
                "# Output is the proven correctly-rounded value under "
                "<mode> at <prec>\n# significant digits; see %s.prov "
                "for the enclosure provenance.\n"
                "# Mathematical fact, not copyrightable expression; "
                "MPFR cross-validates\n# the same values (Phase 3). "
                "Regenerate: tools/gen_transcend_vectors.py\n"
                % (name, name)
            )
            f.write("\n".join(vec_lines))
            f.write("\n")
        with open(os.path.join(OUT_DIR, name + ".prov"), "w") as f:
            f.write(
                "# Enclosure provenance for `%s.txt` (ADR-0026, "
                "fd-97a). P = Arb\n# working precision (bits) at which "
                "the enclosure became decisive;\n# rad = ball radius. "
                "For NearestEven, margin = fractional-ULP\n# distance "
                "of the true value from the nearest half-ULP tie; for "
                "the\n# directed modes, boundary = distance from the "
                "nearest p-digit grid\n# point (the directed "
                "Table-Maker's-Dilemma worst case). small ⇒ hard\n# to "
                "round. `decisive` ⇒ the correctly-rounded value is "
                "established,\n# not sampled.\n" % name
            )
            f.write("\n".join(prov_lines))
            f.write("\n")
        sys.stderr.write("%-7s %d vectors\n" % (name, len(vec_lines)))

    # ADR-0033 corpus-integrity assert. A non-zero cap-hits total means
    # the corpus silently lost a TMD-hard candidate; either CAP_BITS
    # needs raising for that candidate, or the candidate is genuinely
    # TMD-hard at every reasonable Arb precision and the situation
    # needs an explicit ADR-0033 addendum naming it. Exit non-zero so
    # the operator cannot mistake the partial corpus for the full one.
    total_cap_hits = sum(_CAP_HITS.values())
    if total_cap_hits == 0:
        sys.stderr.write("cap-hits: 0\n")
        return
    sys.stderr.write("cap-hits: %d total\n" % total_cap_hits)
    for key in sorted(_CAP_HITS):
        name, fmt_label, mode = key
        sys.stderr.write(
            "  %-7s %-4s %-15s : %d\n"
            % (name, fmt_label, mode, _CAP_HITS[key])
        )
    sys.exit(1)


CASES_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "tests",
    "vectors",
    "round_half_even",
    "cases.txt",
)


def _selftest():
    """No-pip lockstep self-test of the rounding keystone (fd-tgg).

    Runs the shared committed case table
    (`tests/vectors/round_half_even/cases.txt`) through
    `round_half_even_sig`, asserting the rounded VALUE equals the
    table's `expected` (the Rust `round_sig` runs the same table in
    `ferrodec-test-support/tests/round_dec.rs`, so the two stay in
    lockstep). Also guards `representable`, the input-exactness check
    whose absence caused the fd-cb6 d32 `acos` finding. Plain
    `assert`; needs neither python-flint nor any pip package."""
    with open(CASES_PATH) as fh:
        lines = fh.read().splitlines()

    n = 0
    for line in lines:
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        assert len(parts) == 5, "malformed case line: %r" % line
        prec = int(parts[0])
        mode = parts[1]
        value = Fraction(parts[2])
        expected = Fraction(parts[3])
        name = parts[4]
        # NearestEven exercises round_half_even_sig (the keystone the
        # generator's TMD decisiveness uses); the directed modes
        # exercise round_directed_sig. The Rust side runs the same
        # rows through round_directed_sig.
        if mode == "NearestEven":
            r = round_half_even_sig(value, prec)
        else:
            r = round_directed_sig(value, prec, mode)
        if r is None:
            assert expected == 0, "[%s] None result but expected %s" % (
                name,
                parts[2],
            )
        else:
            sign, q, E = r
            got = sign * Fraction(q) * (Fraction(10) ** E)
            assert got == expected, (
                "[%s] round_half_even_sig(%s, %d) value = %s, expected %s"
                % (name, parts[1], prec, got, expected)
            )
            assert len(str(q)) == prec, (
                "[%s] round_half_even_sig returned %d significant digits, "
                "contract is exactly prec=%d" % (name, len(str(q)), prec)
            )
        n += 1
    assert n >= 28, (
        "expected the full shared case table (NearestEven + directed), "
        "ran %d" % n
    )

    # Named regression guard: the round_sig all-nines carry-exponent
    # bug. cos(1e-4) = 0.999999995 must round to the value 1 at p7.
    guard = round_half_even_sig(Fraction("0.999999995"), 7)
    assert guard is not None
    gs, gq, gE = guard
    assert gs * Fraction(gq) * (Fraction(10) ** gE) == 1, (
        "all-nines carry regression: round_half_even_sig(0.999999995, 7) "
        "= %r, must be the value 1" % (guard,)
    )

    # Named regression guard: the fd-cb6 d32 acos finding. The argument
    # 99860965e-8 has 8 significant digits, so it is not exact in
    # decimal32 (prec 7); `representable` must reject it, else
    # `parse_str` re-rounds the argument out from under the proven
    # output. Controls: a 7-digit coefficient is representable, and
    # trailing zeros do not count toward the significand.
    assert representable(99860965, -8, FORMATS["d32"]) is False, (
        "d32-acos-nonrepresentable: 8-digit coef must be rejected at "
        "decimal32 prec 7"
    )
    assert representable(1234567, -6, FORMATS["d32"]) is True
    assert representable(12300000, -2, FORMATS["d32"]) is True  # -> 123
    assert representable(1, 200, FORMATS["d32"]) is False  # out of range

    sys.stderr.write(
        "round-half-even self-test: %d shared cases + 2 named regression "
        "guards (cos1e-4 all-nines carry, d32 acos non-representable) "
        "OK\n" % n
    )


if __name__ == "__main__":
    if "--selftest" in sys.argv[1:]:
        _selftest()
    else:
        only = None
        args = sys.argv[1:]
        for i, a in enumerate(args):
            if a == "--funcs":
                only = set(args[i + 1].split(","))
                unknown = only - set(FUNCS) - set(BINARY) - set(INT_BINARY)
                if unknown:
                    sys.stderr.write(
                        "--funcs: unknown function(s): %s\n"
                        % ", ".join(sorted(unknown))
                    )
                    sys.exit(2)
        emit(only)
