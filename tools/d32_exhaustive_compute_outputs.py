#!/usr/bin/env python3
"""Derive the proven correctly-rounded output for each ADR-0033 Plan
C4 exhaustive sweep worst-case input, and write per-function
companion vector files under `tests/vectors/transcend/exhaustive/`.

The campaign's `<fn>_d32_exhaustive.prov` files record the worst-case
input + half-ULP margin + tier statistics, but not the proven output
(a design oversight; the proven output IS what `solve` returned but
the emit code only kept margin metadata). This tool re-derives the
proven output for each worst-case input via Arb at the same precision
the campaign used, and writes
`tests/vectors/transcend/exhaustive/<fn>.txt` with one line:

    7 NearestEven <input> <proven_output>

matching the existing `<fn>.txt` line format so the test harness can
load and compare. Subdirectory placement avoids collision with the
existing corpus loader, which iterates `*.txt` in `transcend/`
directly.

Run (offline, on a machine with python-flint):

    python3 tools/d32_exhaustive_compute_outputs.py
"""

import os
import re
import sys
from pathlib import Path

_TOOLS_DIR = Path(__file__).resolve().parent
if str(_TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(_TOOLS_DIR))

import gen_transcend_vectors as gtv  # noqa: E402

UNARY_FUNCTIONS = (
    "exp", "ln", "exp2", "log2", "log10", "cbrt", "sqrt",
    "sin", "cos", "tan",
    "asin", "acos", "atan",
    "sinh", "cosh", "tanh",
    "asinh", "acosh", "atanh",
)

TRANSCEND_DIR = Path(__file__).resolve().parent.parent / "tests" / "vectors" / "transcend"
OUT_DIR = TRANSCEND_DIR / "exhaustive"

WORST_CASE_RE = re.compile(
    r"^7 NearestEven (\S+) margin=(\S+) tier=(\S+)$"
)


def parse_worst_case(prov_path):
    """Parse the worst-case row from a `<fn>_d32_exhaustive.prov`.
    Returns (input_token, margin, tier) or None if the file does not
    contain a worst-case row (e.g. the comment line indicates no
    inputs processed)."""
    for line in prov_path.read_text().splitlines():
        if line.startswith("#") or not line.strip():
            continue
        m = WORST_CASE_RE.match(line)
        if m:
            return m.group(1), m.group(2), m.group(3)
    return None


def input_to_coef_exp_neg(token):
    """Reverse of `_format_input` in d32_exhaustive_sweep.py:
    `<sign><coef>e<exp>` -> (coef, exp, neg)."""
    neg = token.startswith("-")
    body = token[1:] if neg else token
    coef_s, exp_s = body.split("e")
    return int(coef_s), int(exp_s), neg


def main():
    gtv._require_flint()
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    fmt = gtv.FORMATS["d32"]
    print(f"Re-deriving proven outputs into {OUT_DIR}/")
    for name in UNARY_FUNCTIONS:
        prov = TRANSCEND_DIR / f"{name}_d32_exhaustive.prov"
        if not prov.exists():
            print(f"  {name:7s} SKIP (no prov file)")
            continue
        wc = parse_worst_case(prov)
        if wc is None:
            print(f"  {name:7s} SKIP (no worst-case row)")
            continue
        in_token, margin, tier = wc
        coef, exp, neg = input_to_coef_exp_neg(in_token)
        fn = gtv.FUNCS[name]
        r = gtv.solve(
            name, fn, coef, exp, neg, fmt,
            mode="NearestEven", cap_bits=gtv.CAP_BITS,
            record_cap_hits=False,
        )
        if r is None:
            print(
                f"  {name:7s} FAIL: solve returned None for {in_token} "
                "(worst case should always be Arb-decisive at full cap)"
            )
            continue
        out_s, P, rad, _ = r
        # Match the existing `<fn>.txt` line format exactly:
        #   <prec> <mode> <input> <output>
        # sqrt is the IEEE §5 op added under ADR-0034; the §9.2 set
        # traces to ADR-0033 Plan C4.
        adr = "ADR-0034" if name == "sqrt" else "ADR-0033 Plan C4"
        out_path = OUT_DIR / f"{name}.txt"
        out_path.write_text(
            f"# {adr} exhaustive sweep worst-case row for `{name}`.\n"
            f"# Re-derived proven correctly-rounded value via Arb at "
            f"P={P} bits.\n"
            f"# Source: tests/vectors/transcend/{name}_d32_exhaustive.prov\n"
            f"# Tool: tools/d32_exhaustive_compute_outputs.py\n"
            f"7 NearestEven {in_token} {out_s}\n"
        )
        print(f"  {name:7s} {in_token:20s} -> {out_s} (margin={margin}, P={P})")
    print(f"Done. Wrote {len(list(OUT_DIR.glob('*.txt')))} files.")


if __name__ == "__main__":
    main()
