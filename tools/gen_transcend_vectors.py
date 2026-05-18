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
DIRECTED_FUNCS = ("exp", "ln", "sin", "cos", "atan", "cbrt", "log10")
BINARY = ("atan2", "pow")
# Independent rng streams so the NearestEven corpus content is
# byte-stable (the directed/binary passes do not perturb its sequence);
# all three are seeded deterministically off the one SEED.
SEED_DIRECTED = SEED ^ 0xD1EC7ED
SEED_BINARY = SEED ^ 0xB17A12


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
}

# Per-function decimal-magnitude window for the random TMD scan, and
# whether the argument is restricted (sign / unit interval / ≥1).
TRIG = {"sin", "cos", "tan"}
POSITIVE = {"ln", "log2", "log10"}          # x > 0
UNIT = {"asin", "acos", "atanh"}            # |x| < 1
GE_ONE = {"acosh"}                          # x ≥ 1


def in_domain(name, coef, exp, neg):
    mag = decimal_magnitude(coef, exp)
    if name in POSITIVE and neg:
        return False
    if name in UNIT:
        # |x| = coef·10^exp must be < 1.
        return (not neg or name == "atanh") and mag < 0
    if name in GE_ONE:
        return (not neg) and mag >= 0
    if name in ("exp", "exp2", "sinh", "cosh"):
        # keep the result finite well inside d128's range
        return mag <= 3
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
    the fixed astro-float oracle skips (|x| ≫ 10^15)."""
    out = []
    if name in TRIG or name in POSITIVE or name == "cbrt":
        # Trig is capped at ~10^180 (≫ the 10^15 astro-float skip
        # threshold, the point of the proof-tier backstop) so Arb's
        # internal argument reduction stays tractable offline; the
        # cheap non-trig families run out to the format's exponent.
        hi = min(fmt["emax"] - 4, 180) if name in TRIG else min(fmt["emax"] - 4, 300)
        ks = [1, 3, 6, 9, 12, 15, 16, 20, 30, 60, 120]
        ks += [k for k in (180, 240, 300) if k <= hi]
        ks.append(hi)
        for k in ks:
            if k <= hi:
                out.append((314_159, k - 6, False))
                if name == "cbrt":
                    out.append((314_159, -(k - 6), True))
                if name in POSITIVE:
                    out.append((271_828, -(k - 6), False))
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


def solve(name, fn, coef, exp, neg, fmt, mode="NearestEven"):
    """Smallest Arb precision at which f(coef·10^exp) is decisive at
    the format precision under `mode`. Returns
    (output_str, P_bits, radius, margin) or None (out of domain, input
    not exactly representable, result out of range, or TMD-hard past
    the cap). The NearestEven path is numerically identical to the
    fd-cb6 generator."""
    p = fmt["prec"]
    if not in_domain(name, coef, exp, neg):
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
    while P <= CAP_BITS:
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
}


def bin_in_domain(name, xt, yt):
    (xc, _xe, xn) = xt
    if name == "pow":
        # Real pow needs a positive base (x > 0); the scan/representor
        # only ever offers x ≥ 0, this rejects a zero coefficient.
        return (not xn) and xc != 0
    # atan2 is defined everywhere except (0, 0); coefficients are ≥ 1,
    # so the pair is never the origin.
    return True


def solve_binary(name, fn2, xt, yt, fmt, mode):
    """Smallest Arb precision at which f(x, y) is decisive under
    `mode`. Returns (output_str, P_bits, radius, margin) or None."""
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
        rx = (-1 if xn else 1) * xc * (10.0 ** xe)
        ry = (-1 if yn else 1) * yc * (10.0 ** ye)
        if rx <= 0:
            return None
        approx = ry * math.log10(rx)
        if abs(approx) > fmt["emax"] + 8:
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
    while P <= CAP_BITS:
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
    pts = [
        (1, 0, False), (1, 0, True), (2, 0, False),
        (5, -1, False), (15, -1, False), (1_234_567, -6, False),
    ]
    return [(a, b) for a in pts for b in pts]


def _scan_arg(rng, p, lo, hi, signed):
    coef = rng.randrange(1, 10 ** min(p, 8))
    exp = rng.randrange(lo, hi)
    neg = signed and (rng.random() < 0.5)
    return (coef, exp, neg)


def emit():
    _require_flint()
    os.makedirs(OUT_DIR, exist_ok=True)
    # name -> list of entry tuples
    #   (fmt_idx, mode_idx, sortkey, prec, mode, in_s, in2_s, out, P,
    #    rad, margin)
    # in2_s is None for the unary functions.
    acc = {name: [] for name in list(FUNCS) + list(BINARY)}
    fmt_order = {k: i for i, k in enumerate(FORMATS)}
    mode_idx = {m: i for i, m in enumerate(MODES_ALL)}

    # --- Pass 1: NearestEven, unary. The legacy candidate/scan path
    # with the legacy rng, so the NE corpus content is byte-stable
    # (only the new mode token and the headers differ); the directed
    # and binary passes draw from independent streams below so they
    # cannot perturb this one. ---
    rng = random.Random(SEED)
    for name, fn in FUNCS.items():
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
                else:
                    exp = rng.randrange(-6, 9)
                    neg = rng.random() < 0.5
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
    for name in DIRECTED_FUNCS:
        fn = FUNCS[name]
        for mode in DIRECTED_MODES:
            for fkey, fmt in FORMATS.items():
                p = fmt["prec"]
                cand = []
                for (coef, exp, neg) in representative(name) + decades(name, fmt):
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
                    else:
                        exp = rng_d.randrange(-6, 9)
                        neg = rng_d.random() < 0.5
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
    for name in BINARY:
        fn2 = BIN_FUNCS[name]
        for mode in MODES_ALL:
            for fkey, fmt in FORMATS.items():
                p = fmt["prec"]
                cand = list(binary_representative(name))
                scanned = []
                for _ in range(TMD_SCAN_BINARY):
                    if name == "pow":
                        xt = _scan_arg(rng_b, p, -3, 4, False)
                        yt = _scan_arg(rng_b, p, -2, 3, True)
                    else:
                        xt = _scan_arg(rng_b, p, -6, 7, True)
                        yt = _scan_arg(rng_b, p, -6, 7, True)
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

    for name in list(FUNCS) + list(BINARY):
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
                "# Binary line (pow, atan2): <prec> <mode> <input1> "
                "<input2> <output>.\n"
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
        emit()
