#!/usr/bin/env python3
"""S2 campaign coverage table (read only): rows certified per
(function, format, input decade), from the frozen campaign provenance
files, plus the decades inside each function's observed span with no
row. Reader A5 receives the output as the uncovered list.

    python3 -I docs/archive/review-2026-10/s2_coverage.py
"""
import re, sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PROV = list((ROOT / "tests/vectors/transcend/campaign").glob("*.prov")) + \
       list((ROOT / "ferrodec-decimal64/tests/vectors/transcend/campaign").glob("*.prov"))
ROW = re.compile(r"^(\d+)\s+(\w+)\s+(-?)(\d+)E([+-]?\d+)\s+margin=(\S+)")
PREC = {"34": "d128", "16": "d64", "7": "d32"}

rows = defaultdict(set)   # (func, fmt, decade) -> set of inputs
for p in sorted(PROV):
    func = p.stem
    for line in p.read_text().split("\n"):
        m = ROW.match(line)
        if not m:
            continue
        prec, mode, sign, coeff, exp = m.group(1), m.group(2), m.group(3), m.group(4), int(m.group(5))
        decade = exp + len(coeff) - 1
        rows[(func, PREC.get(prec, prec), decade)].add(sign + coeff + "E" + str(exp))

print("| function | format | decade | distinct inputs |\n|---|---|---|---|")
for (func, fmt, decade), inputs in sorted(rows.items()):
    print(f"| {func} | {fmt} | {decade} | {len(inputs)} |")

# What the sweep itself covered, from tools/campaign_s2_local.sh (the
# launcher's emit lines are the authority; the frozen rows above are the
# 50 hardest survivors per group and say nothing about the swept span).
SWEPT = [
    # function, format, stratum, decade range (inclusive) or edge band, launcher line
    ("sin", "d128", "decades", "15..6140", "campaign_s2_local.sh:72-74"),
    ("cos", "d128", "decades", "15..6140", "campaign_s2_local.sh:72-74"),
    ("tan", "d128", "decades", "15..6140", "campaign_s2_local.sh:72-74"),
    ("exp", "d128", "exp-edge", "overflow/underflow edge bands", "campaign_s2_local.sh:78"),
    ("exp", "d128", "decades", "-20..4", "campaign_s2_local.sh:79"),
    ("ln", "d128", "decades", "-6100..6100", "campaign_s2_local.sh:80"),
    ("log10", "d128", "decades", "-6100..6100", "campaign_s2_local.sh:81"),
    ("pow", "d128", "pow-edge", "edge only; the 2-D body has no designed stratum", "campaign_s2_local.sh:31,82"),
    ("sinh", "d128", "decades", "-7..4", "campaign_s2_local.sh:83"),
    ("sinh", "d128", "exp-edge", "e^|x|/2 edge bands", "campaign_s2_local.sh:84"),
    ("cosh", "d128", "decades", "-7..4", "campaign_s2_local.sh:85"),
    ("cosh", "d128", "exp-edge", "e^|x|/2 edge bands", "campaign_s2_local.sh:86"),
    ("exp", "d64", "exp-edge", "edge bands", "campaign_s2_local.sh:87"),
    ("exp", "d64", "decades", "-20..2", "campaign_s2_local.sh:88"),
    ("sin", "d64", "decades", "15..370", "campaign_s2_local.sh:89"),
    ("cos", "d64", "decades", "15..370", "campaign_s2_local.sh:90"),
]
print("\n## What the S2 sweep covered (the launcher is the authority)\n")
print("| function | format | stratum | span | source |\n|---|---|---|---|---|")
for row in SWEPT:
    print("| " + " | ".join(row) + " |")

print("\n## Not swept by S2 (reader A5's uncovered list)\n")
print("- Decimal32: nothing (rung 1 decides the whole d32 space; ADR-0033/0034 exhaustive tables cover the 18 unary functions and sqrt).")
print("- Decimal64: only exp, sin, cos; tan, ln, log10, pow, the hyperbolics, the inverse trig, the D1 to D4 groups, and the pi family rest on the planted and frozen corpora.")
print("- Decimal128 trig: decades below 15 (small and moderate arguments) are outside S2; the main Arb corpus, the anchor bands, and S1 cover them.")
print("- Decimal128 exp: decades below -20 (where exp(x) hugs 1) are the ADR-0050/0051 anchor band territory, not S2.")
print("- Decimal128 sinh/cosh: decades below -7 (anchor hugging) excluded by design.")
print("- Decimal128 pow: no body stratum at all; edge only.")
print("- Never swept at any format: exp2, exp10, expm1 and the base variants, log2, logp1 family, cbrt, rootn, rsqrt, hypot, compound, powi, powr, asin, acos, atan, atan2, asinh, acosh, atanh, and the seven pi-family functions. Their budgets rest on their derivations plus six planted rows each (ADR-0059 S3) and the frozen Arb corpus.")
