#!/usr/bin/env python3
"""Recertify the Lefèvre–Stehlé–Zimmermann decimal64 exp worst cases
through Arb and emit the external-anchor corpus (fd-4zo.7).

Source: V. Lefèvre, D. Stehlé, P. Zimmermann, "Worst Cases for the
Exponential Function in the IEEE 754r decimal64 Format", Dagstuhl
Seminar Proceedings 06021, 2006, DOI 10.4230/DagSemProc.06021.11,
license CC BY 4.0 (the registry entry
`docs/references/lefevre-stehle-zimmermann-d64-exp.md` carries the
provenance, archives, and the calibration note). Tables 2 and 3 list
the found bad cases — inputs whose exp sits within 1e-15 ulp of a
rounding breakpoint — plus four mantissa-pattern cases in the text;
they are the only externally certified worst-case table in any IEEE
decimal format.

Transcription safety: each table row below carries the paper's own
claimed digit expansion of exp(x) (the 16-digit significand, the
round digit, the repeated digit run, and the three digits after it).
The generator verifies, in Arb ball arithmetic at escalating
precision, that exp(x) provably lies inside the claimed 35-or-more
digit window BEFORE emitting anything: a transcription error in
either column cannot produce agreement, so the committed corpus is
certified against the paper rather than trusted from it. The four
pattern cases carry no digit expansion in the paper and are certified
by Arb alone (still valid exp vectors; marked in the corpus).

Outputs, per input, all five IEEE rounding modes, derived rigorously
from the ball (floor/tie comparisons must be provable, else the
precision escalates): `tests/vectors/transcend/external/lsz_d64_exp.txt`
in the frozen-corpus line format (`16 <mode> <input> <output>`).
Deterministic; refresh the manifest afterwards:

    (cd tests/vectors/transcend/external && shasum -a 256 *.txt > SHA256SUMS)

Run (any Python >= 3.9 with python-flint; FLINT 3 / Arb):

    python3 tools/gen_lsz_d64_exp.py

The worst-case values are mathematical facts; the table's expression
is CC BY 4.0 with attribution above.
"""

import os
import sys

try:
    from flint import arb, ctx, fmpq
except ImportError as e:  # pragma: no cover
    sys.stderr.write(
        "python-flint (FLINT 3 / Arb) is required: %s\n"
        "Install with `pip install python-flint`.\n" % e
    )
    sys.exit(1)

OUT_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "tests",
    "vectors",
    "transcend",
    "external",
)

PREC = 16  # decimal64
START_BITS = 512
CAP_BITS = 65536

# (input coefficient, input exponent, claimed expansion, output adjusted
# exponent, table tag). The claimed expansion is the paper's digit
# string for exp(x): 16 significand digits, the round digit, the
# repeated digit expanded, then the three digits the paper prints after
# the run. `None` for the four pattern cases (certified by Arb alone).
ROWS = [
    # Table 2 (domain [1e-9, 6.907755278982137] completely checked, plus
    # subintervals of [6.907755278982138, 421.1499284665963]).
    (6581539478341669, -24, "1000000006581539" "5" + "0" * 15 + "177", 0, "t2"),
    (2662858264545929, -23, "1000000026628583" "0" + "0" * 15 + "318", 0, "t2"),
    (3639588333766983, -23, "1000000036395884" "0" + "0" * 15 + "240", 0, "t2"),
    (6036998017773271, -23, "1000000060369982" "0" + "0" * 15 + "379", 0, "t2"),
    (6638670361402304, -22, "1000000663867256" "4" + "9" * 15 + "569", 0, "t2"),
    (9366572213364879, -22, "1000000936657659" "9" + "9" * 15 + "883", 0, "t2"),
    (7970613003079781, -21, "1000007970644768" "5" + "0" * 15 + "362", 0, "t2"),
    (3089765552852523, -20, "1000030898132866" "0" + "0" * 15 + "241", 0, "t2"),
    (1302531956641873, -19, "1000130261678980" "0" + "0" * 16 + "798", 0, "t2"),
    (2241856702421245, -19, "1000224210801727" "5" + "0" * 15 + "118", 0, "t2"),
    (7230293679121590, -19, "1000723290816653" "4" + "9" * 16 + "127", 0, "t2"),
    (5259640428979129, -18, "1005273496619909" "4" + "9" * 15 + "739", 0, "t2"),
    (9407822313572878, -17, "1098645682066338" "5" + "0" * 16 + "278", 0, "t2"),
    (1267914924960933, -16, "1135180299492843" "0" + "0" * 16 + "706", 0, "t2"),
    (5091077534282133, -16, "1663806007261509" "5" + "0" * 15 + "492", 0, "t2"),
    (3359104074009002, -15, "2876340944572687" "5" + "0" * 16 + "904", 1, "t2"),
    (2949551257293143, -13, "1251363586659789" "5" + "0" * 15 + "108", 128, "t2"),
    # Table 3 (c+ <= x < 1e-9; at most two bad cases per exponent).
    (5999879998200072, -25, "1000000000599988" "0" + "0" * 16 + "431", 0, "t3"),
    (6000119998199928, -25, "1000000000600011" "9" + "9" * 16 + "567", 0, "t3"),
    (1019999999994798, -26, "1000000000010199" "9" + "9" * 17 + "646", 0, "t3"),
    (1039999999994592, -26, "1000000000010399" "9" + "9" * 17 + "625", 0, "t3"),
    (1099999999999395, -27, "1000000000001099" "9" + "9" * 20 + "556", 0, "t3"),
    (1199999999999280, -27, "1000000000001199" "9" + "9" * 20 + "423", 0, "t3"),
    (1199999999999928, -28, "1000000000000119" "9" + "9" * 23 + "423", 0, "t3"),
    (1399999999999902, -28, "1000000000000139" "9" + "9" * 23 + "085", 0, "t3"),
    (1999999999999980, -29, "1000000000000019" "9" + "9" * 25 + "733", 0, "t3"),
    (2999999999999955, -29, "1000000000000029" "9" + "9" * 25 + "099", 0, "t3"),
    (1999999999999998, -30, "1000000000000001" "9" + "9" * 28 + "733", 0, "t3"),
    (3999999999999992, -30, "1000000000000003" "9" + "9" * 27 + "786", 0, "t3"),
    (9999999999999995, -31, "1000000000000000" "9" + "9" * 29 + "666", 0, "t3"),
    # Section 3.2's mantissa-pattern bad cases (eps = 3e-15); no digit
    # expansion is printed in the paper for these.
    (3897940992403028, -24, None, 0, "pattern"),
    (4230932991049603, -24, None, 0, "pattern"),
    (4291382990792016, -24, None, 0, "pattern"),
    (4581289989505891, -24, None, 0, "pattern"),
]

MODES = ["NearestEven", "NearestAway", "TowardZero", "TowardPositive", "TowardNegative"]


def q_to_arb(q):
    return arb(q.p) / arb(q.q)


def fmt_sci(coef, exp, prec):
    s = str(coef)
    assert len(s) == prec
    adjusted = exp + prec - 1
    return f"{s[0]}.{s[1:]}e{adjusted}"


def certify_row(coef, exp, claimed, out_adj):
    """All five correctly rounded 16-digit outputs for exp(coef*10^exp),
    every decision provable in the ball, the claimed digit window
    verified first when present. Returns (outputs: mode->(coef16, exp16),
    bits)."""
    x_q = fmpq(coef) * fmpq(10) ** exp if exp >= 0 else fmpq(coef, 10 ** (-exp))
    bits = START_BITS
    while bits <= CAP_BITS:
        ctx.prec = bits
        y = q_to_arb(x_q).exp()

        if claimed is not None:
            # exp(x) must lie in [claimed, claimed + 1) at the claimed
            # window's last place: 10^(out_adj - len + 1).
            n = len(claimed)
            lsd = out_adj - n + 1
            lo_q = fmpq(int(claimed)) * fmpq(10) ** lsd if lsd >= 0 else fmpq(
                int(claimed), 10 ** (-lsd)
            )
            hi_q = lo_q + (fmpq(10) ** lsd if lsd >= 0 else fmpq(1, 10 ** (-lsd)))
            in_window = (y >= q_to_arb(lo_q)) and (y < q_to_arb(hi_q))
            if not in_window:
                bits *= 2
                continue

        # 16-digit truncation: t = floor(y * 10^(15 - out_adj)), proven
        # unique in the ball; then the tie and side comparisons.
        shift = 15 - out_adj
        s_q = fmpq(10) ** shift if shift >= 0 else fmpq(1, 10 ** (-shift))
        scaled = y * q_to_arb(s_q)
        t = scaled.floor().unique_fmpz()
        if t is None:
            bits *= 2
            continue
        t = int(t)
        assert 10**15 <= t < 10**16, f"truncation {t} is not 16 digits"
        exp16 = out_adj - 15
        # Strictly between t and t+1 (exactness would contradict the
        # bad-case premise); the half comparison decides the nearest
        # modes. Every comparison must be provable.
        above_t = scaled > arb(t)
        below_next = scaled < arb(t + 1)
        twice = scaled * arb(2)
        above_half = twice > arb(2 * t + 1)
        below_half = twice < arb(2 * t + 1)
        if not (above_t and below_next and (above_half != below_half)):
            bits *= 2
            continue

        up = (t + 1, exp16)
        down = (t, exp16)
        nearest = up if above_half else down
        outputs = {
            "NearestEven": nearest,
            "NearestAway": nearest,
            "TowardZero": down,  # x > 0 throughout the tables
            "TowardPositive": up,
            "TowardNegative": down,
        }
        return outputs, bits
    raise SystemExit(
        f"certification cap: exp({coef}e{exp}) undecided at {CAP_BITS} bits — "
        "check the transcription against the paper before pinning"
    )


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    lines = [
        "# Lefèvre–Stehlé–Zimmermann decimal64 exp worst cases, recertified",
        "# through Arb (fd-4zo.7). Line: <prec> <mode> <input> <output>.",
        "# Source: DOI 10.4230/DagSemProc.06021.11 (CC BY 4.0), Tables 2-3",
        "# plus the §3.2 mantissa-pattern cases; every table row's claimed",
        "# digit expansion was reproduced in ball arithmetic before emission",
        "# (a transcription error cannot produce agreement). Positive",
        "# arguments only: the paper's search covered them; negatives were",
        "# still running at publication.",
        "# Regenerate: tools/gen_lsz_d64_exp.py",
    ]
    for coef, exp, claimed, out_adj, tag in ROWS:
        outputs, bits = certify_row(coef, exp, claimed, out_adj)
        check = "claimed digits reproduced" if claimed else "Arb-only (pattern case)"
        print(f"exp({coef}e{exp}) [{tag}]: certified at {bits} bits ({check})")
        for m in MODES:
            oc, oe = outputs[m]
            lines.append(f"{PREC} {m} {coef}e{exp} {fmt_sci(oc, oe, PREC)}")
    path = os.path.join(OUT_DIR, "lsz_d64_exp.txt")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"\nwrote {path}: {len(ROWS)} inputs x {len(MODES)} modes = "
          f"{len(ROWS) * len(MODES)} lines; refresh SHA256SUMS before committing")


if __name__ == "__main__":
    main()
