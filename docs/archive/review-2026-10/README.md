# Deep review 2026-10: working artifacts

Companion directory to `docs/archive/REPORT-deep-review-2026-10.md`
(written at the arc's close) and the plan at
`docs/decisions/plans/2026-10-08-deep-review-plan.md`. Everything here
is either a script that regenerates a number the report cites, the
captured output of such a script at the review tip, a reader brief, or
a reproducer for a confirmed finding. A maintainer in 2040 should be
able to rerun the review's measurements from this directory and the
tree alone.

## Phase 1 artifacts

| file | what | regenerate with |
|---|---|---|
| `counts.py`, `counts.md` | declared tests, Kani harnesses, proptest blocks, ignored tests, `as` casts, panic sites in production source per crate; corpus rows per vector directory; registry and ADR sizes | `python3 -I docs/archive/review-2026-10/counts.py` |
| `adr_density.sh`, `adr_density.md` | ADR citation counts in production source (reader D3's sample of ten earlier ADRs comes from here) | `bash docs/archive/review-2026-10/adr_density.sh` |
| `mirror_diff.sh`, `mirror_summary.md` | normalized diff sizes for every mirrored file pair (root vs d64, root vs d32, d64 vs d32, sibling wrappers vs root shims, the extended trio); the full diffs go to an output directory and are not committed | `bash docs/archive/review-2026-10/mirror_diff.sh <out-dir>` |
| `s2_coverage.py`, `s2_coverage.md` | S2 campaign rows pinned per (function, format, input decade), the sweep families the launcher actually ran, and what S2 never swept (reader A5's uncovered list) | `python3 -I docs/archive/review-2026-10/s2_coverage.py` |
| `splice/` | the six D3 agent worktrees' uncommitted deliverables, captured before the worktrees were removed (reader A1's splice check); see `splice/README.md` | not regenerable |

Cargo reported suite totals (passed, ignored, per lane) come from the
battery log at the review tip and are appended to `counts.md` under
"Battery totals" when the battery completes.

## Briefs

`briefs/common.md` is the binding frame every reader receives;
`briefs/<reader>.md` adds the scope, governing documents, checklist,
and extra deliverable for that reader. A launch fills `{{BASELINE}}`
(the tip hash the reader must verify first), `{{SCRATCH}}` (the
session scratchpad), and `{{READER}}`. The plan's wave tables name
the readers: Wave 1 C1 C2 C5 D2 D3; Wave 2 A1 to A5; Wave 3 B1 to B5;
Wave 4 C3 C4 D1 D4.

## Verification records and reader reports

| file | what |
|---|---|
| `phase1b-derivations.md` | the coordinator's own derivations (budgets, the Newton table, the adjudicator read, the generator pins), written before any reader's version existed |
| `wave1-verification.md`, `wave2-verification.md`, `wave3-verification.md`, `wave4-verification.md` | the coordinator's verdict on every reader finding, with the rerun result, the ruling and the closure; the trust table, the contradiction list and the per wave handoffs live here |
| `readers/<reader>.md` | each reader's report saved verbatim from its hand-back (nineteen readers: C1 C2 C5 D2 D3; A1 to A5; B1 to B5; C3 C4 D1 D4) |

## Reproducers

`reproducers/<finding-id>/` holds the script or scratch crate and the
captured output lines for every finding the report marks confirmed,
with a `README.md` naming the oracle environment it needs. Added as
each wave's verification session closes. `P1B-*` are the coordinator's
Phase 1b reproducers; `W2-*`, `W3-*`, `W4-*` are the readers' scratch
crates and scripts, copied with relative path dependencies and the
coordinator's rerun logs beside the readers' outputs.
