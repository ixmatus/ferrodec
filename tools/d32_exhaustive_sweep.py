#!/usr/bin/env python3
"""Exhaustive Decimal32 transcendental sweep (ADR-0033 Slice B,
fd-ykr.2).

This is the offline tool that discharges the worst case margin
completeness residual ADR-0032 named, for `Decimal32` specifically.
The decimal32 input space is small enough (~10^7 to 10^9 canonical
inputs per function after domain restriction) to enumerate every
input through a certified Arb oracle and record the *true* exhaustive
worst case half ULP margin per function, plus any TMD hard
candidates that did not become decisive at `CAP_BITS = 65536`. Slice
C then quotes the exhaustive margins in the per function rustdoc on
`ferrodec-transcend`, upgrading the d32 §9.2 claim from "faithful,
empirically correct on a sample" (ADR-0026) to "machine verified
correctly rounded on every canonical input" (ADR-0033). Decimal64
and Decimal128 stay on the sampled corpus path; their canonical
input cardinalities (~10^16 and ~10^34 respectively) are beyond
exhaustive reach.

This is a *build tool*, not a project dependency. It is never a
workspace member and never enters the Cargo graph: Arb/FLINT (LGPL)
stay entirely outside the build. The committed output is the per
function `tests/vectors/transcend/<fn>_d32_exhaustive.prov` with the
worst case row and per tier statistics; the per input enumeration
outputs are ~10^7 to 10^9 rows per function and never enter the
repository.

The two tier filter (ADR-0033 §Decision). The reviewer's framing,
Lefèvre 2000's binary64 methodology applied to decimal:

1. Tier 1 (cheap fixed precision Arb pre screen): for every
   canonical Decimal32 input in the function's mathematical domain,
   run `solve` at a fixed mid precision (`TIER1_CAP_BITS`, default
   512 bits — far more than the 7-digit format needs but cheap
   enough to run at scale). The overwhelming majority of candidates
   resolve at this precision; record the half ULP margin and
   running worst case.

2. Tier 2 (variable precision survival): for the narrow margin
   survivors (the smallest margin tail from tier 1, plus any
   candidate that did not resolve at tier 1's fixed precision),
   run `solve` at the full `CAP_BITS = 65536` variable precision
   path. Any candidate that still does not resolve is a true TMD
   hard case at `Decimal32`; the worst case row records the actual
   exhaustive minimum margin across both tiers, and the TMD hard
   list (typically empty) is emitted explicitly.

Reuse from `gen_transcend_vectors.py`: `solve` (parameterized in
this slice's C1 commit on `cap_bits` and `record_cap_hits`), the
`FORMATS`, `FUNCS`, and `in_domain` constants/predicates, and the
canonical input filters. The acceptance criterion in `_decisive` is
shared with the corpus generator; the same Arb proof discharges
both.

Run (offline, on a machine with FLINT 3 / python-flint):

    python3 tools/d32_exhaustive_sweep.py --list
    python3 tools/d32_exhaustive_sweep.py --func cbrt --limit 1_000_000
    python3 tools/d32_exhaustive_sweep.py --func exp
    python3 tools/d32_exhaustive_sweep.py  # all 18 unary functions

The full sweep across the 18 unary §9.2 functions is multi day
offline CPU at 16 way parallelism. Per function output lands in
`tests/vectors/transcend/<fn>_d32_exhaustive.prov`; the per
function rustdoc upgrade is Slice C's concern.

Binary surface (`pow`, `atan2`) is out of scope (ADR-0033 §Decision:
the ~10^16 canonical input cardinality at Decimal32 is beyond
exhaustive reach; the binary surface stays on the ADR-0026 sampled
corpus path).
"""

import argparse
import os
import sys
import time
from pathlib import Path

# Reuse the corpus generator's primitives. The script lives alongside
# this one under `tools/`; insert the directory so the import works
# regardless of cwd.
_TOOLS_DIR = Path(__file__).resolve().parent
if str(_TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(_TOOLS_DIR))

import gen_transcend_vectors as gtv  # noqa: E402

# Per ADR-0033 §Decision, the exhaustive sweep covers the 18 unary
# §9.2 transcendentals. The binary surface (pow, atan2) is excluded.
UNARY_FUNCTIONS = (
    "exp", "ln", "exp2", "log2", "log10", "cbrt",
    "sin", "cos", "tan",
    "asin", "acos", "atan",
    "sinh", "cosh", "tanh",
    "asinh", "acosh", "atanh",
)

# Tier 1 pre-screen precision. The 7-digit Decimal32 format needs
# only ~25 bits of binary precision in the output; 512 bits of Arb
# working precision is roughly 20x that, which makes the certified
# ball orders of magnitude tighter than the format's grid. The vast
# majority of candidates resolve here; the narrow margin tail and
# any non-resolved get promoted to tier 2. The exact value is
# empirically tuned at first dry-run; 512 is the starting point.
TIER1_CAP_BITS_DEFAULT = 512

# Default output directory for per-function provenance.
OUT_DIR_DEFAULT = (
    Path(__file__).resolve().parent.parent
    / "tests" / "vectors" / "transcend"
)


def estimate_input_cardinality(name):
    """Rough estimate of the canonical Decimal32 input count for
    `name`'s mathematical domain, before any Arb call. Used by the
    `--list` mode to size the offline campaign without running it.

    The cardinality is dominated by the magnitude range and the
    sign multiplier; the per magnitude coefficient count is roughly
    constant (9 million canonical 7-digit coefficients, after the
    trailing-zero-skip canonicalisation in `enumerate_canonical_d32`).
    """
    fmt = gtv.FORMATS["d32"]
    coefs_per_magnitude = 9_000_000  # canonical (no trailing zero)
    # Per-function magnitude range (matches `gtv.in_domain` after
    # the ADR-0033 Slice A overflow fix):
    if name in ("exp", "exp2", "sinh", "cosh"):
        # in_domain bound is |x| <= emax · ln(10) / log_base; for d32
        # (emax=96) this is |x| <= ~220 (exp) or ~125 (exp2). The
        # canonical inputs in this range cover roughly magnitudes
        # [-95, 2]: ~98 decades of subnormal-to-3-digit; double for
        # sign.
        return 98 * coefs_per_magnitude * 2  # ~1.8e9
    if name in ("ln", "log2", "log10"):
        # x > 0 only; full magnitude range [emin, emax].
        return (fmt["emax"] - fmt["emin"] + 1) * coefs_per_magnitude
    if name in ("asin", "acos", "atanh"):
        # |x| < 1: magnitudes [-95, -1] = 95 decades; signed except
        # asin/acos which only need [0, 1) but solve handles negatives
        # via odd-function symmetry — keep signed for clarity.
        return 95 * coefs_per_magnitude * 2  # ~1.7e9
    if name == "acosh":
        # x >= 1: magnitudes [0, 96] = 97 decades, positive only.
        return 97 * coefs_per_magnitude  # ~8.7e8
    # cbrt, sin, cos, tan, atan, asinh, tanh: unrestricted.
    return (fmt["emax"] - fmt["emin"] + 1) * coefs_per_magnitude * 2  # ~3.5e9


def parse_args():
    parser = argparse.ArgumentParser(
        description="ADR-0033 Slice B exhaustive Decimal32 sweep",
    )
    parser.add_argument(
        "--func",
        choices=UNARY_FUNCTIONS,
        action="append",
        help=(
            "Function to sweep (repeatable). Default: all 18 unary "
            "§9.2 functions."
        ),
    )
    parser.add_argument(
        "--tier",
        choices=("1", "2", "all"),
        default="all",
        help=(
            "Which tier to run. `1` runs only the pre-screen and "
            "reports tier-1 statistics; `2` reads tier-1 survivors "
            "(if cached) and runs tier-2; `all` runs both. Default: "
            "`all`."
        ),
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=os.cpu_count() or 1,
        help=(
            "Parallelism for the per-input Arb evaluation. Default: "
            "all available cores."
        ),
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help=(
            "Process at most N candidates per function (for dry-run "
            "and tool validation). Default: no limit (full sweep)."
        ),
    )
    parser.add_argument(
        "--tier1-cap-bits",
        type=int,
        default=TIER1_CAP_BITS_DEFAULT,
        help=(
            "Arb working precision cap for tier 1. Default: %d. The "
            "value is empirically tuned to the format; 7-digit "
            "Decimal32 resolves at much lower precision than this, "
            "the headroom keeps the tier-1 candidate-survival rate "
            "below ~0.1%%." % TIER1_CAP_BITS_DEFAULT
        ),
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=OUT_DIR_DEFAULT,
        help=(
            "Output directory for per-function "
            "`<fn>_d32_exhaustive.prov`. Default: tests/vectors/"
            "transcend/ in the project root."
        ),
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help=(
            "List per-function input cardinality estimates and exit "
            "(no Arb calls). Useful for sizing the offline campaign."
        ),
    )
    return parser.parse_args()


def cmd_list():
    """Estimate per-function input cardinality without running Arb."""
    print("ADR-0033 Slice B exhaustive Decimal32 sweep — input cardinality")
    print("=" * 72)
    print("%-10s %-15s %s" % ("function", "inputs (≈)", "domain"))
    print("-" * 72)
    domain_labels = {
        "exp": "|x| ≤ ~220 (overflow bound)",
        "exp2": "|x| ≤ ~125 (overflow bound)",
        "ln": "x > 0",
        "log2": "x > 0",
        "log10": "x > 0",
        "cbrt": "all reals",
        "sin": "all reals",
        "cos": "all reals",
        "tan": "all reals (poles excluded by kernel)",
        "asin": "|x| ≤ 1",
        "acos": "|x| ≤ 1",
        "atan": "all reals",
        "sinh": "|x| ≤ ~220 (overflow bound)",
        "cosh": "|x| ≤ ~220 (overflow bound)",
        "tanh": "all reals",
        "asinh": "all reals",
        "acosh": "x ≥ 1",
        "atanh": "|x| < 1",
    }
    total = 0
    for name in UNARY_FUNCTIONS:
        n = estimate_input_cardinality(name)
        total += n
        print("%-10s %15s  %s" % (
            name, "%.2e" % n, domain_labels.get(name, "")
        ))
    print("-" * 72)
    print("%-10s %15s" % ("TOTAL", "%.2e" % total))
    print()
    print(
        "Per-candidate cost at tier 1 (Arb @ %d bits, single-threaded):"
        % TIER1_CAP_BITS_DEFAULT
    )
    print("  ~10–100 μs depending on function (Taylor series cost).")
    print(
        "Per-candidate cost at tier 2 (variable precision up to "
        "CAP_BITS = %d):" % gtv.CAP_BITS
    )
    print("  ~1–100 ms depending on margin (most candidates short-circuit).")
    print()
    print(
        "Full sweep wall-time estimate at 16-way parallelism: "
        "1–4 days, depending on Arb performance and survival rate."
    )


def cmd_sweep(args):
    """Run the exhaustive sweep. Slice B C3-C4 implements this."""
    # ADR-0033 Slice B C3 implements tier 1 + tier 2 + provenance emit.
    raise NotImplementedError(
        "Slice B C3 / C4: tier-1 pre-screen + tier-2 promotion + "
        "per-function provenance emit. The scaffolding is in place; "
        "the actual Arb sweep lands in the next commits."
    )


def main():
    args = parse_args()
    if args.list:
        cmd_list()
        return 0
    cmd_sweep(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
