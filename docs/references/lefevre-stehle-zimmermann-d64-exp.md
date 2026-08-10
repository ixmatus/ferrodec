---
slug: lefevre-stehle-zimmermann-d64-exp
category: conformance
citation: "Lefèvre, V., Stehlé, D., Zimmermann, P. Worst Cases for the Exponential Function in the IEEE 754r decimal64 Format. Dagstuhl Seminar Proceedings 06021 (Reliable Implementation of Real Number Algorithms: Theory and Practice), Schloss Dagstuhl, 2006. Refereed version: LNCS 5045, pp. 114-126, Springer, 2008."
canonical: "https://drops.dagstuhl.de/entities/document/10.4230/DagSemProc.06021.11"
doi: "10.4230/DagSemProc.06021.11 (Dagstuhl); 10.1007/978-3-540-85521-7_7 (LNCS)"
archived: "https://web.archive.org/web/20260217115028/https://drops.dagstuhl.de/entities/document/10.4230/DagSemProc.06021.11 (page); https://web.archive.org/web/20251210020530/https://drops.dagstuhl.de/storage/16dagstuhl-seminar-proceedings/dsp-vol06021/DagSemProc.06021.11/DagSemProc.06021.11.pdf (PDF); https://web.archive.org/web/20251119101245/https://inria.hal.science/inria-00068731 (HAL author deposit, the pre-certification capture)"
archive-date: "2026-08-09 (both captures verified live at citation time)"
retrieved: "2026-08-09"
sha256: 3b364d18082f9e90f65c48514adc69390ad3079d5f7c6b6e4d9409506b271324
license: "CC BY 4.0 (the Dagstuhl deposit). The Springer LNCS version is paywalled and cited as a pointer only."
vendor-status: "values-transcribed: Tables 2-3 and the §3.2 pattern cases live in tools/gen_lsz_d64_exp.py under the CC BY attribution; no PDF copy is vendored (pointer plus archive)."
rot-risk: institutional-archive
provenance: primary
consumers:
  - tools/gen_lsz_d64_exp.py
  - tests/vectors/transcend/external/lsz_d64_exp.txt
  - ferrodec-decimal64/tests/transcend_lsz_exp.rs
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - tests/corpus_integrity.rs
  - ferrodec-decimal64/tests/transcend_lsz_exp.rs
notes: "The only externally certified worst-case table in any IEEE decimal format. Every transcribed row's claimed digit expansion is reproduced in Arb ball arithmetic by the generator before a corpus line is emitted, so the committed vectors are certified against the paper rather than trusted from it."
---

# Lefèvre–Stehlé–Zimmermann: decimal64 exp worst cases

The paper reports a lattice-reduction (BaCSeL) search for bad cases
of the exponential function in decimal64: inputs whose exp lies
within `1e-15` ulp of a rounding breakpoint, over `[1e-9,
ln(10^385 − 10^369)]` (completely checked below `6.907755278982137`,
subintervals above) and `[c+, 1e-9)`. Its Tables 2 and 3 plus four
mantissa-pattern cases give 34 inputs; the reported worst case,
`exp(9.407822313572878e-2)`, sits about `1.8e-19` ulp beyond its
nearest-mode midpoint. No comparable externally certified table
exists for any other decimal format or function.

## What it grounds here

`tools/gen_lsz_d64_exp.py` transcribes the tables, reproduces each
row's claimed digit expansion in Arb ball arithmetic (a
transcription error in either column cannot produce agreement),
derives all five modes' correctly rounded outputs rigorously, and
freezes `tests/vectors/transcend/external/lsz_d64_exp.txt` (170
lines). `ferrodec-decimal64/tests/transcend_lsz_exp.rs` replays them
against `Decimal64::exp` with exact per-mode pins: agreement from a
fully independent lineage (their search, their table, our Arb
recertification, this kernel), which is the highest information per
dollar in the verification program — zero compute beyond one offline
certification pass.

## Calibration note (the second external datum)

ADR-0034's Decimal32 exhaustive program supplied the first external
calibration of the sampled-margin model. This table is the second:
the true decimal64 exp worst case (`~1.8e-19` ulp from a breakpoint,
i.e. `~1.8e-35` relative at 16 digits) sits far inside what any
fixed rung must resolve, and the ladder's rung 2 budget for `exp`
(2.5e5 units of `1e-110`) clears it by some seventy decimal orders.
When the S2 deep campaign runs, its sampled Decimal64 exp minimum
gains this row as its truth anchor: a sampled minimum falling below
the LSZ worst case would falsify the sampler, exactly as ADR-0034's
exhaustive Decimal32 data disciplined the Decimal32 model.

## Coverage gaps

Positive arguments only: the paper's search covered them; the
negative-argument runs were still executing at publication and no
follow-up table for decimal64 exp negatives is published. The tables
sample the checked domain (all bad cases below the quality threshold
in the completely-checked interval, a selection elsewhere); the
completely-checked interval is `[1e-9, 6.907755278982137]`. Nothing
here certifies any function other than exp or any format other than
decimal64.
