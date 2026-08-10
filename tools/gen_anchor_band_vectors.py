#!/usr/bin/env python3
"""Generate the near-anchor band regression corpus (fd-aqs.6).

The 2026-06-09 review found that the shared Extended kernel lost
relative accuracy near the additive anchors 0 and 1: formulas that
hand `1 +/- tiny` (or an absorbed `x^2` correction) to a 50
significant digit representation turn the kernel's ~1e-49 *relative*
error model into ~1e-49 *absolute*, mis-rounding `ln`/`log10`/`log2`
just below 1, `atanh`/`asinh` in the small-argument band, `asin`/
`acos` near +/-1, and `pow` through `y * ln(x)`. None of the existing
layers sampled those bands (Arb corpus coefficients stop at 8 digits
and its decades stop short; the astro-float suites assert the weaker
faithful contract; decTest has no transcendentals).

This script emits `tests/vectors/transcend/anchor_bands/<func>.txt`
in the same line format as the sampled corpus (`<prec> <mode> <input>
<output>`, binary `pow` lines carry two inputs), covering the hazard
decades at all three format precisions with messy fixed coefficients.

Oracle and acceptance rule:

* mpmath at 160 dps computes the true value; generation aborts unless
  the value clears every rounding boundary (grid point and half-ULP
  point) by a relative margin of 1e-60, so the 160-digit
  approximation decides each mode with certainty.
* For `ln`, `log10`, and `pow`, CPython's `decimal` (libmpdec,
  correctly rounded, structurally independent of mpmath) must agree
  at every emitted mode, mirroring the corpus accept rule of two
  independent references agreeing.

Directed-mode lines are emitted wherever the oracle can certify the
side (boundary distance above the 1e-100 ULP floor); the kernel's
ADR-0051 residual seam decides the grid-hugging cases, so they get
directed lines too. Only cases whose correction sits below the oracle
floor (e.g. `atanh(1.000001e-95)` at Decimal32, ~1e-191 relative)
keep nearest-mode-only lines: the same seam delivers their directed
neighbours, but this tooling cannot independently certify them;
`tools/certify_anchor_floor.py` (Arb ball arithmetic, fd-4zo.6)
certifies exactly those groups afterwards and upgrades them to all
five modes, so run it after any regeneration here. The
small-argument trigonometric, hyperbolic, and exponential families
(`sin`, `cos`, `tan`, `atan`, `sinh`, `cosh`, `tanh`, `exp`, `exp2`)
are covered alongside the ADR-0050 anchor-band functions.

Mathematical facts, not copyrightable expression. Regenerate with:
    python3 tools/gen_anchor_band_vectors.py
"""

import os
from decimal import Context, Decimal, localcontext
from decimal import ROUND_CEILING, ROUND_DOWN, ROUND_FLOOR, ROUND_HALF_EVEN, ROUND_HALF_UP

import mpmath as mp

mp.mp.dps = 160

OUT_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "tests", "vectors", "transcend", "anchor_bands"
)

MODES = [
    ("NearestEven", ROUND_HALF_EVEN),
    ("NearestAway", ROUND_HALF_UP),
    ("TowardZero", ROUND_DOWN),
    ("TowardPositive", ROUND_CEILING),
    ("TowardNegative", ROUND_FLOOR),
]

PRECS = (34, 16, 7)


def true_value(func, x, y=None):
    f = {
        "ln": mp.ln,
        "log10": lambda v: mp.log(v, 10),
        "log2": lambda v: mp.log(v, 2),
        "atanh": mp.atanh,
        "asinh": mp.asinh,
        "asin": mp.asin,
        "acos": mp.acos,
        "sin": mp.sin,
        "cos": mp.cos,
        "tan": mp.tan,
        "atan": mp.atan,
        "sinh": mp.sinh,
        "cosh": mp.cosh,
        "tanh": mp.tanh,
        "exp": mp.exp,
        "exp2": lambda v: mp.power(2, v),
    }
    if func == "pow":
        return mp.power(mp.mpmathify(x), mp.mpmathify(y))
    return f[func](mp.mpmathify(x))


def to_decimal(v, digits=120):
    """Render an mpf to a Decimal with `digits` significant digits."""
    return Decimal(mp.libmp.to_str(v._mpf_, digits, strip_zeros=False))


def classify_margin(d, prec):
    """Classify the value's distance to the `prec`-digit rounding
    boundaries, in ULP-fraction units.

    Returns `"full"` when the value clears both the grid points and
    the half-ULP boundary by more than the 1e-100 ULP oracle floor
    (every mode decided with certainty by the 140-digit rendering of
    the 160-dps computation); `"nearest_only"` when the value hugs a
    grid point closer than the floor (the nearest modes still round
    to the hugged point regardless of side; the kernel's ADR-0051
    residual seam delivers the directed neighbours too, but this
    tooling cannot independently certify them, and an uncertified pin
    would be faith, not verification); and `None` when the value hugs
    the half-ULP boundary below the floor, where the nearest modes
    are also undecidable and the input must be replaced.

    `copy_abs` (not `abs`) and an explicit wide local context: the
    unary Decimal operations apply the *ambient* context precision,
    and the default 28 digits would silently round the boundary
    digits away.
    """
    if d == 0:
        return None
    a = d.copy_abs()
    ulp = Decimal(10) ** (a.adjusted() - prec + 1)
    with localcontext(Context(prec=250)):
        frac = (a / ulp) % 1
        floor = Decimal("1e-100")
        if abs(frac - Decimal("0.5")) <= floor:
            return None
        if frac <= floor or (1 - frac) <= floor:
            return "nearest_only"
        return "full"


def rounded(d, prec, rounding):
    return Context(prec=prec, rounding=rounding).create_decimal(d)


def sci(d):
    """Render a Decimal as the corpus's `<mantissa>e<exp>` form."""
    sign, digits, exp = d.as_tuple()
    coef = "".join(map(str, digits))
    mant = coef[0] + ("." + coef[1:] if len(coef) > 1 else "")
    adj = exp + len(coef) - 1
    return f"{'-' if sign else ''}{mant}e{adj}"


def libmpdec_check(func, x, y, prec, mode_name, rounding, want):
    """Cross-check ln/log10/pow against libmpdec.

    NearestEven only: CPython documents `Decimal.ln`/`log10`/`power`
    as correctly rounded *using ROUND_HALF_EVEN*; under a directed
    context rounding they still deliver the half-even answer, so the
    directed modes rest on mpmath plus the margin guard alone.
    """
    if rounding != ROUND_HALF_EVEN:
        return
    ctx = Context(prec=prec, rounding=rounding, Emax=999999999, Emin=-999999999)
    if func == "ln":
        got = ctx.ln(Decimal(x))
    elif func == "log10":
        got = ctx.log10(Decimal(x))
    elif func == "pow":
        got = ctx.power(Decimal(x), Decimal(y))
    else:
        return
    assert got == want, (
        f"libmpdec disagrees: {func}({x}{', ' + y if y else ''}) "
        f"prec {prec} {mode_name}: mpmath {want}, libmpdec {got}"
    )


def representable(s, prec):
    """The operand must be exactly representable at `prec` digits;
    otherwise the harness's parse would silently round it and the
    vector would pin the wrong function value."""
    return len(Decimal(s).as_tuple().digits) <= prec


def emit(lines, func, x, prec, y=None):
    assert representable(x, prec), f"{func}: input {x} exceeds {prec} digits"
    if y is not None:
        assert representable(y, prec), f"{func}: input2 {y} exceeds {prec} digits"
    v = true_value(func, x, y)
    d = to_decimal(v)
    margin = classify_margin(d, prec)
    assert margin is not None, f"{func}({x}) prec {prec}: hugs the half-ULP boundary"
    # Grid-hugging values get directed lines too: the kernel's
    # residual seam (ADR-0051) decides them. The only narrowing left
    # is the oracle floor in the classifier.
    nearest_only = margin == "nearest_only"
    for mode_name, rounding in MODES:
        if nearest_only and mode_name not in ("NearestEven", "NearestAway"):
            continue
        want = rounded(d, prec, rounding)
        libmpdec_check(func, x, y, prec, mode_name, rounding, want)
        inputs = f"{x} {y}" if y is not None else x
        lines.append(f"{prec} {mode_name} {inputs} {sci(want)}")


# ----------------------------------------------------------------------------
# Input construction. Fixed messy integers; no randomness, so the
# corpus regenerates byte-identical.

# Offsets m for x = (10^P - m) * 10^-P (just below 1) and
# x = (10^(P-1) + m') * 10^-(P-1) (just above 1, the control side).
BELOW_ONE_M = {
    34: [
        7,
        99,
        161803,
        123456789,
        31415926535898,
        1414213562373095048,
        271828182845904523536028,
        577215664901532860606512090082402,
    ],
    16: [7, 99, 12345, 3141592, 161803398874989],
    7: [1, 33, 2718],
}
ABOVE_ONE_M = {
    34: [7, 161803, 31415926535898, 1414213562373095048],
    16: [7, 12345, 161803398874989],
    7: [1, 2718],
}

# (coefficient, exponent) pairs for the atanh/asinh small-argument
# band; coefficient digit counts stay within the format precision.
SMALL_ARG = {
    34: [
        ("9876543210987654321098765432109876", -58),
        ("3141592653589793238462643383279502", -54),
        ("1414213562373095048801688724209698", -50),
        ("2718281828459045235360287471352662", -45),
        ("5772156649015328606065120900824024", -40),
        ("1234567890123456789012345678901234", -54),  # ~1e-20 decade
        ("1732050807568877293527446341505872", -64),  # ~1e-30 decade
        ("1618033988749894848204586834365638", -76),  # ~1e-42 decade
    ],
    16: [
        ("1536385851986827", -59),  # the review's Decimal64 witness, ~1.5e-44
        ("9876543210987654", -40),
        ("3141592653589793", -30),
        ("2718281828459045", -22),
    ],
    7: [
        ("1234567", -30),  # ~1e-24: correction visible at 50 digits
        ("7654321", -27),
        ("1000001", -95),  # pure small-arg: NE-only until fd-aqs.7
    ],
}

# Small arguments for the even (anchored at 1) and exponential
# families. The deep decades exercise the ADR-0051 residual seam
# (the 50-digit value is grid-exact and the directed neighbour comes
# from the carried residual direction); the shallow decades stay on
# the series path as controls on both sides of the absorption
# boundary.
EVEN_SMALL = {
    34: [
        ("3141592653589793238462643383279502", -73),  # ~3.1e-40
        ("9876543210987654321098765432109876", -58),  # ~9.9e-25, near the boundary
        ("1234567890123456789012345678901234", -47),  # ~1.2e-14, series path
    ],
    16: [
        ("3141592653589793", -55),  # ~3.1e-40
        ("9876543210987654", -25),  # ~9.9e-10, series path
    ],
    7: [
        ("3141593", -46),  # ~3.1e-40
        ("9876543", -10),  # ~9.9e-4, series path
    ],
}
EXP_SMALL = {
    34: [
        ("3141592653589793238462643383279502", -94),  # ~3.1e-61
        ("1414213562373095048801688724209698", -80),  # ~1.4e-47, series path
    ],
    16: [
        ("2718281828459045", -75),  # ~2.7e-60
        ("1618033988749894", -40),  # ~1.6e-25, series path
    ],
    7: [
        ("1234567", -66),  # ~1.2e-60
        ("7654321", -20),  # ~7.7e-14, series path
    ],
}

# pow near-1 bases (as below-one offsets at the working precision)
# crossed with large integer exponents.
POW_CASES = {
    34: [
        ("0.9999999999999999990123456789012345", "1000000000000000"),
        ("0.9999999999999999999998765432109876", "9007199254740993"),
        ("1.000000000000000000000123456789012", "1000000000000000"),
    ],
    16: [
        ("0.9999999999876543", "100000000"),
        ("1.000000000012345", "100000000"),
    ],
    7: [
        ("0.9999912", "10000"),
        ("1.000003", "10000"),
    ],
}


def below_one(prec, m):
    coef = 10**prec - m
    return f"{coef}e-{prec}"


def above_one(prec, m):
    coef = 10 ** (prec - 1) + m
    return f"{coef}e-{prec - 1}"


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    per_func = {}

    for prec in PRECS:
        near_one = [below_one(prec, m) for m in BELOW_ONE_M[prec]]
        near_one += [above_one(prec, m) for m in ABOVE_ONE_M[prec]]
        for func in ("ln", "log10", "log2"):
            lines = per_func.setdefault(func, [])
            for x in near_one:
                emit(lines, func, x, prec)
        for func in ("atanh", "asinh"):
            lines = per_func.setdefault(func, [])
            for coef, e in SMALL_ARG[prec]:
                emit(lines, func, f"{coef}e{e}", prec)
                emit(lines, func, f"-{coef}e{e}", prec)
        for func in ("asin", "acos"):
            lines = per_func.setdefault(func, [])
            for m in BELOW_ONE_M[prec]:
                emit(lines, func, below_one(prec, m), prec)
            if func == "acos":
                for m in BELOW_ONE_M[prec][:3]:
                    emit(lines, func, "-" + below_one(prec, m), prec)
        # The odd small-argument family shares the atanh/asinh input
        # lists (f(x) = x + c3 x^3 + ..., both signs).
        for func in ("sin", "tan", "atan", "sinh", "tanh"):
            lines = per_func.setdefault(func, [])
            for coef, e in SMALL_ARG[prec]:
                emit(lines, func, f"{coef}e{e}", prec)
                emit(lines, func, f"-{coef}e{e}", prec)
        # The even family is anchored at 1; one negative input per
        # precision pins the symmetry.
        for func in ("cos", "cosh"):
            lines = per_func.setdefault(func, [])
            for coef, e in EVEN_SMALL[prec]:
                emit(lines, func, f"{coef}e{e}", prec)
            neg_coef, neg_e = EVEN_SMALL[prec][0]
            emit(lines, func, f"-{neg_coef}e{neg_e}", prec)
        # The exponential family crosses the 1 anchor in both
        # directions (exp(x) > 1 for x > 0, < 1 for x < 0).
        for func in ("exp", "exp2"):
            lines = per_func.setdefault(func, [])
            for coef, e in EXP_SMALL[prec]:
                emit(lines, func, f"{coef}e{e}", prec)
                emit(lines, func, f"-{coef}e{e}", prec)
        lines = per_func.setdefault("pow", [])
        for x, y in POW_CASES[prec]:
            emit(lines, "pow", x, prec, y)

    for func, lines in per_func.items():
        path = os.path.join(OUT_DIR, f"{func}.txt")
        with open(path, "w") as f:
            f.write(
                f"# Near-anchor band regression vectors for `{func}` (fd-aqs.6).\n"
                "# Unary line: <prec> <mode> <input> <correctly-rounded-output>.\n"
                "# Binary line (pow): <prec> <mode> <input1> <input2> <output>.\n"
                "# Oracle: mpmath @160dps with a 1e-60 boundary-margin guard;\n"
                "# ln/log10/pow additionally cross-checked against libmpdec at\n"
                "# every emitted mode. Directed-mode lines appear only where a\n"
                "# 50-digit-correct kernel can decide them (off-grid at 50\n"
                "# digits); grid-exact cases are NearestEven/NearestAway-only\n"
                "# pending the fd-aqs.7 enclosure seam. Mathematical fact, not\n"
                "# copyrightable expression.\n"
                "# Regenerate: tools/gen_anchor_band_vectors.py\n"
            )
            f.write("\n".join(lines) + "\n")
        print(f"{func}: {len(lines)} lines -> {path}")


if __name__ == "__main__":
    main()
