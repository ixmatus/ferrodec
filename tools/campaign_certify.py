#!/usr/bin/env python3
"""Certify campaign sweep lines against the Arb proof tier (ADR-0059,
lane plan S1; fd-4zo.4).

Reads sweep output TSVs (survivor `S`, divergence `D`, and
unconditional substream `A` lines), re-derives the correctly rounded
value for every rounding mode through gen_transcend_vectors' certified
ball arithmetic (`solve` / `solve_binary`, full CAP_BITS, cap hits
recorded), and compares against the recorded production outputs by
exact numeric equality (Fraction, so rendering differences cannot
false-positive).

Verdicts per (row, mode): ok / MISROUND / tmd-hard (Arb cap hit:
undecidable through 65536 bits, itself a genuine finding) / oor
(result out of the format range; counted, listed, never silent).

Exit status: 0 only when there were zero misrounds, zero tmd-hard
rows, and zero parse failures. A misround is a witness against the
shipped ADR-0032 claim: surface it, do not rationalize it.

Usage:
  python3 tools/campaign_certify.py out/s1_sin_shard0.tsv [more.tsv ...]
      [--format d128] [--verdicts out/verdicts.tsv]

Two-process file handoff by design: the Rust sweep never links
python-flint, this script never links the kernel.
"""

import argparse
import os
import sys
from fractions import Fraction

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_transcend_vectors as gtv  # noqa: E402

MODES = ("NearestEven", "NearestAway", "TowardPositive",
         "TowardNegative", "TowardZero")


def parse_decimal_triplet(s):
    """`[-]<digits>E<exp>` or a plain/point decimal string ->
    (coef:int, exp:int, neg:bool)."""
    s = s.strip()
    neg = s.startswith("-")
    if neg:
        s = s[1:]
    if "E" in s or "e" in s:
        mant, _, e = s.replace("e", "E").partition("E")
        exp = int(e)
    else:
        mant, exp = s, 0
    if "." in mant:
        whole, _, frac = mant.partition(".")
        exp -= len(frac)
        mant = whole + frac
    return int(mant), exp, neg


def to_fraction(coef, exp, neg):
    v = Fraction(coef) * (Fraction(10) ** exp if exp >= 0
                          else Fraction(1, 10 ** (-exp)))
    return -v if neg else v


def value_fraction(s):
    """Exact Fraction of a finite decimal output string, or None for
    non-finite renderings (Inf / NaN)."""
    t = s.strip().lstrip("+")
    if any(k in t for k in ("Inf", "inf", "NaN", "nan")):
        return None
    return to_fraction(*parse_decimal_triplet(t))


# The exp family's in_domain slack deliberately stops the corpus one
# decade short of the overflow boundary; the S1 probe targets exactly
# that band, so the certifier bypasses the slack (solve's additive
# domain_check=False path) for these functions only.
EDGE_FAMILY = {"exp", "exp2", "sinh", "cosh"}


def certify_row(func, xs, ys, outs, fmt):
    """Yield (mode, verdict, arb_str, got_str) per mode."""
    xt = parse_decimal_triplet(xs)
    yt = parse_decimal_triplet(ys) if ys not in (None, "-") else None
    if any(value_fraction(o) is None for o in outs):
        # A non finite production output means the true result sits at
        # the overflow boundary: §7.4 disposition territory, judged by
        # the special value gates, not Arb TMD certification. Named,
        # counted, never silently dropped.
        for mode, got in zip(MODES, outs):
            yield (mode, "overflow-boundary", "-", got)
        return
    for mode, got in zip(MODES, outs):
        before = sum(len(v) for v in gtv._CAP_HITS.values())
        if yt is None:
            r = gtv.solve(func, gtv.FUNCS[func], xt[0], xt[1], xt[2],
                          fmt, mode, cap_bits=gtv.CAP_BITS,
                          record_cap_hits=True,
                          domain_check=func not in EDGE_FAMILY)
        else:
            r = gtv.solve_binary(func, gtv.BIN_FUNCS[func], xt, yt,
                                 fmt, mode, cap_bits=gtv.CAP_BITS,
                                 record_cap_hits=True)
        if r is None:
            after = sum(len(v) for v in gtv._CAP_HITS.values())
            verdict = "tmd-hard" if after > before else "oor"
            yield (mode, verdict, "-", got)
            continue
        out_s = r[0]
        arb_v = value_fraction(out_s)
        got_v = value_fraction(got)
        if got_v is None or arb_v is None:
            # Arb decisive value vs a non-finite production rendering
            # (or vice versa): a mismatch by definition.
            verdict = "ok" if (got_v is None and arb_v is None) else "MISROUND"
        else:
            verdict = "ok" if arb_v == got_v else "MISROUND"
        yield (mode, verdict, out_s, got)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("inputs", nargs="+")
    ap.add_argument("--format", default="d128", choices=sorted(gtv.FORMATS))
    ap.add_argument("--verdicts", default=None,
                    help="write per-mode verdict lines here")
    args = ap.parse_args()
    gtv._require_flint()
    fmt = gtv.FORMATS[args.format]

    counts = {"rows": 0, "ok": 0, "MISROUND": 0, "tmd-hard": 0,
              "oor": 0, "overflow-boundary": 0, "parse-error": 0}
    vout = open(args.verdicts, "w", encoding="ascii") if args.verdicts else None
    for path in args.inputs:
        with open(path, encoding="ascii") as f:
            for line in f:
                if not line or line[0] not in "SDA":
                    continue
                parts = line.rstrip("\n").split("\t")
                if len(parts) != 13:
                    counts["parse-error"] += 1
                    print("parse-error: %s" % line.rstrip(), file=sys.stderr)
                    continue
                tag, func, idx, xs, ys = parts[0], parts[1], parts[2], parts[3], parts[4]
                outs = [p.rpartition("#")[0] for p in parts[8:13]]
                counts["rows"] += 1
                for mode, verdict, arb_s, got_s in certify_row(
                        func, xs, ys, outs, fmt):
                    counts[verdict] += 1
                    if vout or verdict != "ok":
                        rec = "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s" % (
                            verdict, tag, func, idx, xs, mode, arb_s, got_s)
                        if vout:
                            print(rec, file=vout)
                        if verdict != "ok":
                            print(rec)
    if vout:
        vout.close()

    print("certify: rows=%d ok=%d misround=%d tmd_hard=%d oor=%d "
          "overflow_boundary=%d parse_error=%d"
          % (counts["rows"], counts["ok"], counts["MISROUND"],
             counts["tmd-hard"], counts["oor"],
             counts["overflow-boundary"], counts["parse-error"]))
    bad = counts["MISROUND"] + counts["tmd-hard"] + counts["parse-error"]
    if counts["MISROUND"]:
        print("MISROUND WITNESSES FOUND: the shipped correctly rounded "
              "claim is falsified on the rows above. Surface to Parnell "
              "before any disclosure edit (ADR-0059 S1 protocol).",
              file=sys.stderr)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
