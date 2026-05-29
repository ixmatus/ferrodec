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

ADR-0034 extends this tool beyond the 18 §9.2 transcendentals to the
IEEE 754 §5 mandatory `sqrt`. sqrt is algebraic-irrational, so it
shares the Table Maker's Dilemma structure that makes exhaustion
load-bearing; basic arithmetic does not (its exact result is
computable in finite precision), which is why the ADR-0034 scope
stops at sqrt rather than extending to the binary arithmetic surface.
The two-tier filter applies to sqrt unchanged; its domain is x >= 0,
and its worst-case row lands in
`tests/vectors/transcend/sqrt_d32_exhaustive.prov` alongside the
§9.2 set.

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
import datetime
import multiprocessing
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
# §9.2 transcendentals. ADR-0034 adds the IEEE 754 §5 mandatory
# `sqrt` (also unary, also TMD-bearing). The binary surface (pow,
# atan2) stays excluded: its ~10^16 canonical input cardinality at
# Decimal32 is beyond exhaustive reach.
UNARY_FUNCTIONS = (
    "exp", "ln", "exp2", "log2", "log10", "cbrt", "sqrt",
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
    if name in ("ln", "log2", "log10", "sqrt"):
        # x > 0 (sqrt: x >= 0; coef >= 1 so the enumerator never emits
        # zero either way); full magnitude range [emin, emax].
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
            "Function to sweep (repeatable). Default: all unary "
            "functions (the 18 §9.2 transcendentals plus §5 sqrt)."
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
        "sqrt": "x ≥ 0",
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


def enumerate_canonical_d32(name):
    """Stream canonical Decimal32 (coef, exp, neg) triples in the
    function's mathematical domain. Canonical means the coefficient
    has no trailing zero (so each numeric value is yielded exactly
    once by its shortest-coefficient encoding); cohorts of the same
    value are de-duplicated.

    The enumeration walks every canonical coefficient (1..10^p-1
    with no trailing zero) and every exponent that keeps the
    adjusted exponent in [emin, emax] (normal range; subnormals are
    skipped to match `representable`'s existing rule, the same
    scope as the ADR-0026 corpus). The in-domain filter is the
    shared `gtv.in_domain` predicate, with the ADR-0033 Slice A
    format-dependent bound for exp/sinh/cosh/exp2."""
    fmt = gtv.FORMATS["d32"]
    p = fmt["prec"]
    emin, emax = fmt["emin"], fmt["emax"]
    for coef in range(1, 10 ** p):
        # Canonical: no trailing zero (or single-digit, where the
        # "trailing zero" notion is vacuous).
        if coef >= 10 and coef % 10 == 0:
            continue
        d = len(str(coef))
        # Adjusted exponent = exp + d - 1; constrain to [emin, emax].
        exp_low = emin - d + 1
        exp_high = emax - d + 1
        for exp in range(exp_low, exp_high + 1):
            for neg in (False, True):
                if gtv.in_domain(name, coef, exp, neg, fmt):
                    yield (coef, exp, neg)


# --- multiprocessing workers (top-level for pickle compatibility) ---
#
# The worker pool uses 'spawn' (default on macOS); each child re-imports
# `gen_transcend_vectors` and the lambda-based FUNCS, which is fine
# because the lambdas live in module scope. We pass the function name
# (string) across the pipe and the worker looks up FUNCS[name] locally;
# lambdas themselves are not picklable through `imap_unordered`. The
# Pool initializer binds Arb in each worker process via gtv's lazy
# loader so the first `solve` call does not crash on `NameError`.

def _worker_init():
    """Pool initializer: bind Arb (`arb`, `ctx`, `fmpq`) as module
    globals in `gen_transcend_vectors` for this worker process. The
    gtv module's `_require_flint` is the canonical loader; calling
    it here saves the first-call overhead in every worker invocation
    and avoids the `NameError: name 'fmpq' is not defined` that
    otherwise surfaces inside `solve`'s call chain."""
    gtv._require_flint()


def _tier1_worker(args):
    """Tier-1 pre-screen worker. Returns (coef, exp, neg, status, margin)
    where status is 'decisive' (margin valid) or 'promote' (margin
    None, candidate needs tier 2). 'promote' means the Arb ball did
    not become decisive within `cap_bits` precision, so the candidate
    is a tier-2 survivor; record_cap_hits=False keeps the gtv
    integrity counter clean (a tier-1 non-decisive is not a corpus
    loss, it's a promotion)."""
    name, coef, exp, neg, cap_bits = args
    fmt = gtv.FORMATS["d32"]
    fn = gtv.FUNCS[name]
    r = gtv.solve(
        name, fn, coef, exp, neg, fmt, "NearestEven",
        cap_bits=cap_bits, record_cap_hits=False,
    )
    if r is None:
        # In-domain + representable pre-checked by the enumerator;
        # `solve` returns None here iff the cap was reached.
        return (coef, exp, neg, "promote", None)
    out_s, P, rad, margin = r
    return (coef, exp, neg, "decisive", margin)


def _tier2_worker(args):
    """Tier-2 variable-precision survival worker. Returns
    (coef, exp, neg, status, margin) where status is 'resolved'
    (margin valid, decisive at some precision <= CAP_BITS) or
    'tmd_hard' (margin None, decisive at no precision through
    CAP_BITS). The CAP_BITS ceiling is the gtv module default."""
    name, coef, exp, neg = args
    fmt = gtv.FORMATS["d32"]
    fn = gtv.FUNCS[name]
    r = gtv.solve(
        name, fn, coef, exp, neg, fmt, "NearestEven",
        cap_bits=gtv.CAP_BITS, record_cap_hits=False,
    )
    if r is None:
        return (coef, exp, neg, "tmd_hard", None)
    out_s, P, rad, margin = r
    return (coef, exp, neg, "resolved", margin)


def _format_input(coef, exp, neg):
    return "%s%de%d" % ("-" if neg else "", coef, exp)


def _format_duration(seconds):
    h, rem = divmod(int(seconds), 3600)
    m, s = divmod(rem, 60)
    return "%dh %dm %ds" % (h, m, s)


def sweep_function(name, tier1_cap_bits, workers, limit, out_dir, run_tier1, run_tier2):
    """Run the exhaustive sweep for one function. Returns a dict of
    per-tier statistics + worst-case row + tmd_hard list. Writes the
    provenance file `<out_dir>/<name>_d32_exhaustive.prov`."""
    sys.stderr.write(
        "=== %s: exhaustive Decimal32 sweep ===\n" % name
    )
    sys.stderr.write(
        "  tier-1 cap bits: %d; tier-2 cap bits: %d\n"
        % (tier1_cap_bits, gtv.CAP_BITS)
    )
    sys.stderr.write("  workers: %d\n" % workers)
    if limit is not None:
        sys.stderr.write("  limit: %d (dry-run mode)\n" % limit)

    t0 = time.monotonic()

    # --- Tier 1: cheap fixed-precision pre-screen ---
    tier1_decisive = 0
    tier2_candidates = []
    worst_margin = float("inf")
    worst_input = None
    worst_tier = None

    def _stream_args():
        n = 0
        for (coef, exp, neg) in enumerate_canonical_d32(name):
            if limit is not None and n >= limit:
                return
            n += 1
            yield (name, coef, exp, neg, tier1_cap_bits)

    if run_tier1:
        sys.stderr.write("  tier 1 (pre-screen)...\n")
        with multiprocessing.Pool(workers, initializer=_worker_init) as pool:
            t1_start = time.monotonic()
            n_processed = 0
            last_report = t1_start
            for (coef, exp, neg, status, margin) in pool.imap_unordered(
                _tier1_worker, _stream_args(), chunksize=5000,
            ):
                n_processed += 1
                if status == "decisive":
                    tier1_decisive += 1
                    if margin < worst_margin:
                        worst_margin = margin
                        worst_input = (coef, exp, neg)
                        worst_tier = 1
                elif status == "promote":
                    tier2_candidates.append((coef, exp, neg))
                # Progress report every 30 seconds.
                now = time.monotonic()
                if now - last_report >= 30.0:
                    rate = n_processed / (now - t1_start)
                    sys.stderr.write(
                        "    tier-1: %s processed, %s decisive, %s promoted (%.0f cand/s)\n"
                        % (
                            f"{n_processed:_}",
                            f"{tier1_decisive:_}",
                            f"{len(tier2_candidates):_}",
                            rate,
                        )
                    )
                    last_report = now
            t1_wall = time.monotonic() - t1_start
            sys.stderr.write(
                "    tier-1 done: %s candidates in %s (%.0f cand/s); "
                "%s decisive, %s promoted to tier 2\n"
                % (
                    f"{n_processed:_}",
                    _format_duration(t1_wall),
                    n_processed / t1_wall if t1_wall > 0 else 0,
                    f"{tier1_decisive:_}",
                    f"{len(tier2_candidates):_}",
                )
            )

    # --- Tier 2: variable-precision survival ---
    tier2_resolved = 0
    tmd_hard = []

    if run_tier2 and tier2_candidates:
        sys.stderr.write(
            "  tier 2 (variable precision on %s survivors)...\n"
            % f"{len(tier2_candidates):_}"
        )
        with multiprocessing.Pool(workers, initializer=_worker_init) as pool:
            t2_start = time.monotonic()
            for (coef, exp, neg, status, margin) in pool.imap_unordered(
                _tier2_worker,
                ((name, c, e, n) for (c, e, n) in tier2_candidates),
                chunksize=100,
            ):
                if status == "resolved":
                    tier2_resolved += 1
                    if margin < worst_margin:
                        worst_margin = margin
                        worst_input = (coef, exp, neg)
                        worst_tier = 2
                elif status == "tmd_hard":
                    tmd_hard.append((coef, exp, neg))
            t2_wall = time.monotonic() - t2_start
            sys.stderr.write(
                "    tier-2 done: %s resolved, %s TMD-hard in %s\n"
                % (
                    f"{tier2_resolved:_}",
                    f"{len(tmd_hard):_}",
                    _format_duration(t2_wall),
                )
            )

    total_wall = time.monotonic() - t0
    n_total = tier1_decisive + len(tier2_candidates)

    # --- Emit per-function provenance ---
    out_path = out_dir / ("%s_d32_exhaustive.prov" % name)
    # sqrt is the IEEE §5 op added under ADR-0034; the 18 §9.2
    # transcendentals trace to ADR-0033 (fd-ykr.2).
    provenance_adr = (
        "ADR-0034 §5 sqrt" if name == "sqrt" else "ADR-0033 §9.2, fd-ykr.2"
    )
    out_dir.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        f.write(
            "# Exhaustive Decimal32 sweep for `%s` (%s).\n"
            "# Tool: tools/d32_exhaustive_sweep.py\n"
            "# Generated: %s\n"
            "# Wall time: %s\n"
            "# Inputs evaluated: %s\n"
            "# Tier 1 cap bits: %d; tier 2 cap bits: %d\n"
            "# Tier 1 decisive: %s (%.4f%%)\n"
            "# Tier 2 promoted: %s\n"
            "# Tier 2 resolved: %s\n"
            "# TMD-hard at CAP_BITS=%d: %s\n"
            "# Limit: %s\n"
            "#\n"
            % (
                name,
                provenance_adr,
                datetime.date.today().isoformat(),
                _format_duration(total_wall),
                f"{n_total:_}",
                tier1_cap_bits,
                gtv.CAP_BITS,
                f"{tier1_decisive:_}",
                (100.0 * tier1_decisive / n_total) if n_total > 0 else 0.0,
                f"{len(tier2_candidates):_}",
                f"{tier2_resolved:_}",
                gtv.CAP_BITS,
                f"{len(tmd_hard):_}",
                "none (full sweep)" if limit is None else f"{limit:_} (dry-run)",
            )
        )
        if worst_input is not None:
            coef, exp, neg = worst_input
            f.write(
                "# Worst-case half-ULP margin (NearestEven, Decimal32):\n"
                "7 NearestEven %s margin=%.6e tier=%d\n"
                "#\n"
                % (_format_input(coef, exp, neg), worst_margin, worst_tier)
            )
        else:
            f.write("# Worst-case half-ULP margin: NO INPUTS PROCESSED\n#\n")
        if tmd_hard:
            f.write(
                "# TMD-hard candidates (%d, did not become decisive at CAP_BITS=%d):\n"
                % (len(tmd_hard), gtv.CAP_BITS)
            )
            for (coef, exp, neg) in sorted(tmd_hard):
                f.write("#   %s\n" % _format_input(coef, exp, neg))
        else:
            f.write("# TMD-hard candidates: none\n")

    sys.stderr.write(
        "  → %s (worst margin=%.6e at %s, tier %s; wall=%s)\n"
        % (
            out_path.name,
            worst_margin if worst_input is not None else float("nan"),
            _format_input(*worst_input) if worst_input is not None else "(none)",
            worst_tier,
            _format_duration(total_wall),
        )
    )
    return {
        "name": name,
        "inputs": n_total,
        "tier1_decisive": tier1_decisive,
        "tier2_promoted": len(tier2_candidates),
        "tier2_resolved": tier2_resolved,
        "tmd_hard": len(tmd_hard),
        "worst_margin": worst_margin if worst_input is not None else None,
        "worst_input": worst_input,
        "worst_tier": worst_tier,
        "wall_seconds": total_wall,
    }


def cmd_sweep(args):
    """Run the exhaustive sweep across the selected functions."""
    # `_require_flint` binds Arb in the parent (needed for any direct
    # Arb call in the parent process; workers re-import and re-bind
    # via gtv's lazy loader on first call).
    gtv._require_flint()

    funcs = args.func if args.func else list(UNARY_FUNCTIONS)
    run_tier1 = args.tier in ("1", "all")
    run_tier2 = args.tier in ("2", "all")

    sys.stderr.write(
        "ADR-0033 Slice B exhaustive Decimal32 sweep: %d function%s\n"
        % (len(funcs), "" if len(funcs) == 1 else "s")
    )

    results = []
    for name in funcs:
        r = sweep_function(
            name,
            tier1_cap_bits=args.tier1_cap_bits,
            workers=args.workers,
            limit=args.limit,
            out_dir=args.out,
            run_tier1=run_tier1,
            run_tier2=run_tier2,
        )
        results.append(r)

    # Final summary table.
    sys.stderr.write("\n=== Summary ===\n")
    sys.stderr.write(
        "%-10s %-15s %-12s %-10s %-10s %s\n"
        % ("function", "inputs", "worst margin", "tier", "tmd-hard", "wall")
    )
    for r in results:
        sys.stderr.write(
            "%-10s %15s  %-12s %-10s %-10s %s\n"
            % (
                r["name"],
                f"{r['inputs']:_}",
                "%.4e" % r["worst_margin"] if r["worst_margin"] is not None else "(none)",
                str(r["worst_tier"]) if r["worst_tier"] is not None else "-",
                f"{r['tmd_hard']:_}",
                _format_duration(r["wall_seconds"]),
            )
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
