#!/usr/bin/env python3
"""Freeze the S2 campaign corpus (fd-4zo.19, ADR-0059 S2).

Selects the hardest surviving rows per (function, format) from the
campaign sweep TSVs, takes each row's five per-mode outputs from the
CERTIFIER's verdict files (the Arb proof tier, never the production
outputs), and writes the committed corpus:

    tests/vectors/transcend/campaign/<func>.txt   (mixed-precision
        lines, same grammar as the sampled corpus: the loader
        filters by the leading precision token)
    tests/vectors/transcend/campaign/<func>.prov  (per-row margins
        and sweep provenance: campaign id, shard, index)
    tests/vectors/transcend/campaign/MANIFEST.json

Selection: at most --keep rows per (func, fmt), ranked by the row's
minimum boundary distance in fractional ULP (min of the grid and tie
families, from the sweep's exact integer distances). Every selected
row must be verdict `ok` in all five modes; anything else aborts the
freeze (the certification gate has already failed in that case and
the freeze should never run).

Usage:
  tools/freeze_campaign_corpus.py --sweep-dir ~/.local/share/ferrodec-campaign/s2 \
      [--keep 50] [--out tests/vectors/transcend/campaign]

Pure file transform: no Arb, no kernel. Rerunnable; output is a
deterministic function of the sweep + verdict files.
"""

import argparse
import json
import os
import subprocess
import sys
from fractions import Fraction

MODES = ("NearestEven", "NearestAway", "TowardPositive",
         "TowardNegative", "TowardZero")
FMT_PREC = {"d128": 34, "d64": 16}


def parse_sweep_line(parts):
    """S-line fields: tag func idx x y w grid_x2 tie_x2 out1..out5."""
    tag, func, idx, xs, ys = parts[0], parts[1], parts[2], parts[3], parts[4]
    w = int(parts[5])
    grid_x2 = int(parts[6])
    tie_x2 = int(parts[7])
    return tag, func, idx, xs, ys, w, grid_x2, tie_x2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sweep-dir", required=True)
    ap.add_argument("--keep", type=int, default=50)
    ap.add_argument("--out", default="tests/vectors/transcend/campaign")
    args = ap.parse_args()

    sweep = args.sweep_dir
    cert = os.path.join(sweep, "certify")
    if not os.path.isdir(cert):
        sys.exit("no certify/ dir under the sweep dir; certify first")

    # verdicts: (base, idx, mode) -> (verdict, arb_s)
    verdicts = {}
    for name in os.listdir(cert):
        if not name.endswith(".verdicts"):
            continue
        base = name[: -len(".verdicts")]
        with open(os.path.join(cert, name), encoding="ascii") as f:
            for line in f:
                v = line.rstrip("\n").split("\t")
                # verdict tag func idx xs mode arb_s got_s
                verdicts[(base, v[3], v[5])] = (v[0], v[6])

    # candidates: (func, fmt) -> list of rows
    groups = {}
    runlog_files = []
    for name in sorted(os.listdir(sweep)):
        if not name.endswith(".tsv"):
            continue
        base = name[: -len(".tsv")]
        campaign = base.split("_")[0]
        fmt = "d64" if campaign == "s2d64" else "d128"
        runlog_files.append(name)
        with open(os.path.join(sweep, name), encoding="ascii") as f:
            for line in f:
                if not line.startswith("S"):
                    continue
                parts = line.rstrip("\n").split("\t")
                if len(parts) != 13:
                    sys.exit("malformed sweep line in %s: %r" % (name, line))
                tag, func, idx, xs, ys, w, gx2, tx2 = parse_sweep_line(parts)
                # fractional-ULP distances: x2 is in units of
                # 10^-w / 2 ULP, so frac = x2 / (2 * 10^w).
                m = Fraction(min(gx2, tx2), 2 * 10 ** w)
                groups.setdefault((func, fmt), {}).setdefault(
                    (xs, ys), (m, base, idx))
                prev = groups[(func, fmt)][(xs, ys)]
                if m < prev[0]:
                    groups[(func, fmt)][(xs, ys)] = (m, base, idx)

    os.makedirs(args.out, exist_ok=True)
    manifest_groups = {}
    by_func = {}
    for (func, fmt), rows in sorted(groups.items()):
        ranked = sorted(
            ((m, xs, ys, base, idx) for (xs, ys), (m, base, idx) in rows.items()),
        )[: args.keep]
        manifest_groups["%s_%s" % (func, fmt)] = {
            "candidates": len(rows),
            "kept": len(ranked),
            "worst_kept_margin_ulp": float(ranked[-1][0]),
            "min_margin_ulp": float(ranked[0][0]),
        }
        prec = FMT_PREC[fmt]
        for m, xs, ys, base, idx in ranked:
            for mode in MODES:
                key = (base, idx, mode)
                if key not in verdicts:
                    sys.exit("no verdict for %s idx %s mode %s" % (base, idx, mode))
                verdict, arb_s = verdicts[key]
                if verdict != "ok":
                    sys.exit(
                        "non-ok verdict %s for %s idx %s mode %s: the "
                        "certification gate should have failed the run"
                        % (verdict, base, idx, mode))
                if ys != "-":
                    txt = "%d %s %s %s %s" % (prec, mode, xs, ys, arb_s)
                else:
                    txt = "%d %s %s %s" % (prec, mode, xs, arb_s)
                prov = ("%d %s %s%s margin=%.3e family=%s src=%s:%s"
                        % (prec, mode, xs,
                           (" " + ys) if ys != "-" else "",
                           float(m), "min(grid,tie)", base, idx))
                by_func.setdefault(func, ([], []))
                by_func[func][0].append(txt)
                by_func[func][1].append(prov)

    git_sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], capture_output=True,
        text=True, check=True).stdout.strip()
    for func, (txt_lines, prov_lines) in sorted(by_func.items()):
        hdr = (
            "# S2 deep-margin campaign corpus for `%s` (fd-4zo.19,\n"
            "# ADR-0059 S2). The hardest certified survivors of the\n"
            "# sweep, at most %d per (function, format), every output\n"
            "# from the Arb proof tier. Same line grammar as the\n"
            "# sampled corpus; see MANIFEST.json for the campaign\n"
            "# parameters and %s.prov for per-row margins.\n"
            "# Regenerate: tools/freeze_campaign_corpus.py\n"
            % (func, args.keep, func))
        with open(os.path.join(args.out, "%s.txt" % func), "w") as fh:
            fh.write(hdr)
            fh.write("\n".join(txt_lines) + "\n")
        with open(os.path.join(args.out, "%s.prov" % func), "w") as fh:
            fh.write("# Provenance for campaign/%s.txt: sweep margins "
                     "(fractional ULP,\n# min over both boundary "
                     "families) and the producing shard/index.\n" % func)
            fh.write("\n".join(prov_lines) + "\n")

    runlog = open(os.path.join(sweep, "RUNLOG"), encoding="ascii").read() \
        if os.path.exists(os.path.join(sweep, "RUNLOG")) else ""
    manifest = {
        "campaign": "s2 (+s2d64)",
        "frozen_at_git": git_sha,
        "sweep_runlog": runlog.strip().splitlines(),
        "threshold_frac_ulp": "1e-6",
        "keep_per_group": args.keep,
        "selection": "min(grid, tie) fractional-ULP distance, exact "
                     "integer sweep measurement; outputs from the "
                     "certifier's Arb solve at full CAP_BITS",
        "groups": manifest_groups,
    }
    with open(os.path.join(args.out, "MANIFEST.json"), "w") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
        fh.write("\n")
    total = sum(len(v[0]) for v in by_func.values())
    print("frozen: %d files, %d lines, %d groups"
          % (len(by_func), total, len(manifest_groups)))


if __name__ == "__main__":
    main()
