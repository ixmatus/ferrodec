#!/usr/bin/env python3
"""Certify the anchor-band corpus's sub-1e-100 directed corrections
with Arb ball arithmetic, and upgrade the nearest-mode-only groups to
all five rounding modes (ADR-0051's oracle-floor note; fd-4zo.6).

The anchor-band generator (`gen_anchor_band_vectors.py`, mpmath at
160 dps) declines to emit directed-mode lines for inputs whose true
value hugs a grid point closer than 1e-100 of an ULP: at that depth
its 160-digit approximation cannot certify the side, and an
uncertified pin would be faith, not verification. This tool closes
exactly that gap with a rigorous oracle: FLINT/Arb ball arithmetic,
where an enclosure excluding zero is a proof of sign, not an estimate.

For every nearest-only group in the corpus it certifies, at escalating
working precision up to the 65536-bit cap the frozen-corpus tooling
already uses:

* the SIDE: the ball for `f(x) - g` (g the hugged grid point, read
  from the group's own nearest-mode output) provably excludes zero;
* the BRACKET: `|f(x) - g|` is provably below 1e-100 of one ULP of
  `g`, so no other rounding boundary sits between the value and its
  anchor, the nearest modes round to `g` regardless of side (the
  existing NearestEven/NearestAway lines are re-verified against
  this), and the three directed modes are decided by the side alone.

The certified directed lines are inserted into the corpus files in
the generator's own line format, and the run is deterministic and
idempotent (a second run finds no nearest-only groups and rewrites
nothing). Refresh the manifest afterwards:

    (cd tests/vectors/transcend/anchor_bands && shasum -a 256 *.txt > SHA256SUMS)

Run (any Python >= 3.9 with python-flint, e.g.
`pip install python-flint`; FLINT 3 / Arb):

    python3 tools/certify_anchor_floor.py

Mathematical facts, not copyrightable expression.
"""

import os
import re
import sys

try:
    from flint import arb, ctx, fmpq
except ImportError as e:  # pragma: no cover
    sys.stderr.write(
        "python-flint (FLINT 3 / Arb) is required: %s\n"
        "Install with `pip install python-flint`.\n" % e
    )
    sys.exit(1)

CORPUS_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "tests",
    "vectors",
    "transcend",
    "anchor_bands",
)

# Arb evaluators for every unary function the corpus carries. Only the
# odd small-argument family has nearest-only groups today, but the
# certifier stays total over the corpus's unary surface so a future
# regeneration cannot silently outgrow it.
ARB_FN = {
    "sin": arb.sin,
    "cos": arb.cos,
    "tan": arb.tan,
    "atan": arb.atan,
    "sinh": arb.sinh,
    "cosh": arb.cosh,
    "tanh": arb.tanh,
    "asinh": arb.asinh,
    "atanh": arb.atanh,
    "exp": arb.exp,
    "exp2": lambda x: (x * arb(2).log()).exp(),
    "ln": arb.log,
    "log10": lambda x: x.log() / arb(10).log(),
    "asin": arb.asin,
    "acos": arb.acos,
}

MODES_ALL = ["NearestEven", "NearestAway", "TowardZero", "TowardPositive", "TowardNegative"]

START_BITS = 1024
CAP_BITS = 65536

# The generator's oracle floor: corrections certified here sit below
# 1e-100 of an ULP, which is the bracket bound the directed-mode
# derivation and the nearest-mode re-verification both lean on.
FLOOR = fmpq(1, 10**100)


def parse_token(tok):
    """`<sign><coef>e<exp>` or a plain decimal -> (sign, coef, exp) with
    the coefficient an int and the token's digit count preserved."""
    m = re.fullmatch(r"(-?)(\d+)(?:\.(\d+))?e(-?\d+)", tok)
    if not m:
        raise ValueError(f"unparseable corpus token {tok!r}")
    sign, whole, frac, exp = m.group(1), m.group(2), m.group(3) or "", int(m.group(4))
    digits = whole + frac
    return (sign == "-", int(digits), exp - len(frac))


def to_fmpq(neg, coef, exp):
    q = fmpq(coef) * fmpq(10) ** exp if exp >= 0 else fmpq(coef, 10 ** (-exp))
    return -q if neg else q


def fmt_sci(neg, coef, exp, prec):
    """The generator's scientific rendering: `prec` significant digits,
    point after the first."""
    s = str(coef)
    assert len(s) == prec, f"coefficient {coef} is not {prec} digits"
    adjusted = exp + prec - 1
    body = s[0] + "." + s[1:] if prec > 1 else s
    return f"{'-' if neg else ''}{body}e{adjusted}"


def neighbor(neg, coef, exp, prec, toward_pinf):
    """The adjacent `prec`-digit grid point in the signed direction."""
    lo, hi = 10 ** (prec - 1), 10**prec - 1
    magnitude_up = toward_pinf != neg
    if magnitude_up:
        coef += 1
        if coef > hi:
            coef, exp = lo, exp + 1
    else:
        coef -= 1
        if coef < lo:
            coef, exp = hi, exp - 1
    return (neg, coef, exp)


def certify(func, x_q, anchor_q, ulp_q, label):
    """Certified side of the anchor: (+1 above, -1 below), plus the
    bits it took. Escalates precision until BOTH the sign and the
    1e-100-ULP bracket are proven; a cap hit is a loud failure.
    `ulp_q` is one ULP of the anchor (10^quantum: the anchor carries
    exactly the format precision's digits)."""
    bound_q = FLOOR * ulp_q
    bits = START_BITS
    while bits <= CAP_BITS:
        ctx.prec = bits
        x = arb(x_q.p) / arb(x_q.q)
        y = ARB_FN[func](x)
        g = arb(anchor_q.p) / arb(anchor_q.q)
        delta = y - g
        above = delta > arb(0)
        below = delta < arb(0)
        bracket = abs(delta) < arb(bound_q.p) / arb(bound_q.q)
        if (above != below) and bracket:
            return (1 if above else -1), bits, delta
        bits *= 2
    raise SystemExit(
        f"certification cap: {func}({label}) undecided at {CAP_BITS} bits — "
        "investigate before pinning"
    )


def main():
    total = 0
    for fname in sorted(os.listdir(CORPUS_DIR)):
        if not fname.endswith(".txt"):
            continue
        func = fname[:-4]
        path = os.path.join(CORPUS_DIR, fname)
        lines = open(path).read().splitlines()

        # Group data lines by (prec, input); pow (binary) files have no
        # nearest-only groups and 5-token lines are left untouched.
        groups = {}
        for i, line in enumerate(lines):
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            parts = s.split()
            if len(parts) != 4:
                continue
            key = (parts[0], parts[2])
            groups.setdefault(key, []).append((i, parts[1], parts[3]))

        inserts = {}  # line index -> list of new lines
        for (prec_s, input_tok), rows in sorted(groups.items()):
            modes = {m for (_, m, _) in rows}
            if len(modes) == 5:
                continue
            assert modes == {"NearestEven", "NearestAway"}, (
                f"{fname}: unexpected partial group {sorted(modes)} for {input_tok}"
            )
            prec = int(prec_s)
            neg, coef, exp = parse_token(input_tok)
            # The hugged grid point is the nearest-mode output; both
            # nearest rows must agree on it.
            outs = {o for (_, _, o) in rows}
            assert len(outs) == 1, f"{fname}: nearest outputs disagree for {input_tok}"
            g_neg, g_coef, g_exp = parse_token(outs.pop())
            assert len(str(g_coef)) == prec, f"{fname}: anchor is not {prec} digits"
            anchor_q = to_fmpq(g_neg, g_coef, g_exp)
            ulp_q = fmpq(10) ** g_exp if g_exp >= 0 else fmpq(1, 10 ** (-g_exp))

            side, bits, delta = certify(
                func, to_fmpq(neg, coef, exp), anchor_q, ulp_q, input_tok
            )

            # y strictly inside the open interval between the anchor and
            # its neighbor on the certified side; every directed mode is
            # the matching endpoint.
            if side > 0:
                a, b = (g_neg, g_coef, g_exp), neighbor(g_neg, g_coef, g_exp, prec, True)
            else:
                a, b = neighbor(g_neg, g_coef, g_exp, prec, False), (g_neg, g_coef, g_exp)
            toward_zero = b if a[0] else a  # negative interval: b is nearer zero
            directed = {
                "TowardPositive": b,
                "TowardNegative": a,
                "TowardZero": toward_zero,
            }
            last_row_idx = max(i for (i, _, _) in rows)
            new = [
                f"{prec} {m} {input_tok} {fmt_sci(*directed[m], prec)}"
                for m in ("TowardZero", "TowardPositive", "TowardNegative")
            ]
            inserts.setdefault(last_row_idx, []).extend(new)
            total += 1
            print(
                f"{func}({input_tok}) @p{prec}: side {'+' if side > 0 else '-'} "
                f"certified at {bits} bits, delta in {delta}"
            )

        if inserts:
            out = []
            for i, line in enumerate(lines):
                out.append(line)
                if i in inserts:
                    out.extend(inserts[i])
            with open(path, "w") as f:
                f.write("\n".join(out) + "\n")
            print(f"  -> {fname}: {sum(len(v) for v in inserts.values())} directed lines inserted")

    if total == 0:
        print("no nearest-only groups: corpus already certified (idempotent no-op)")
    else:
        print(f"\ncertified {total} groups; refresh SHA256SUMS before committing")


if __name__ == "__main__":
    main()
