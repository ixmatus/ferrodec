# Deep review arc 2026-10: plan

> Status: accepted 2026-10-08. Supersedes the 2026-09-21 draft at
> `docs/decisions/plans/2026-09-21-deep-review-plan.md` (its witnessed
> facts and derivation list are folded in; the file is removed in the
> arc's first commit). On acceptance this plan is copied to
> `docs/decisions/plans/2026-10-08-deep-review-plan.md` and becomes
> the beads epic.

## Context

The last whole-codebase review ran 2026-06-09 at `3b9f063`
(`docs/archive/REPORT-rigorous-review-2026-06-09.md`): six parallel
readers, every critical claim re-verified at runtime, five confirmed
critical families, one refuted claim, a remediation epic closed by
2026-07-03. Since then `main` has taken 173 commits and about 118k
lines: the whole correctly rounded Decimal128 lane (ADR-0059 ladder,
ADR-0060 algebraic group and adjudicator, ADR-0061 pi family, the S2
campaign corpus), `ferrodec-decimal` 3.0, the formats at 4.1.0,
`ferrodec-multiword` `U1024`. None of it has had an independent read.
`ferrodec-transcend` is now 22.9k source lines with no `tests/`
directory of its own; `exact.rs` alone is 3,800 lines.

Two facts fix the order of work. Push CI on `main` has failed every
run since 2026-07-26 (seven of 27 jobs: `clippy`, `ferrodec-transcend`,
`cross-compile (thumbv6m-none-eabi)`, `ferrodec-decimal64`,
`ferrodec-decimal32`, `ferrodec-decimal`, `ferrodec-test-support`)
from four causes reproduced locally on 2026-09-21 and still unfixed at
`0a5be4c`. The README's CI sentence has been false for ten weeks. Six
dirty D3 agent worktrees (21 GB, all at `c27b773`) and two orphan
branches remain. The honesty of the record outranks new review work,
so Phase 0 restores it.

The documentation map already shows about fifteen places where the
documents contradict each other or the code (README "production
ready" beside "published versions are yanked"; "five overlapping
stacks" above a list of eleven; `docs/testing.md` Part I still stating
the pre-ADR-0059 correctness mechanism its own frontier section says
was refuted; `docs/testing.md` saying the MPFR gate and the
differential never run in CI while `verification.yml` runs both; the
ADR index stopping at 0054; `KNOWN_ISSUES.md` counts "as of 4.0.0";
`ferrodec-decimal64/src/ops/exp.rs:3-13` still calling the kernel
"faithful, ≤ 1 ULP"). Coherence is a full reader, not a sweep, and it
runs early so its contradictions seed the correctness readers.

Parnell's decisions (2026-10-08): about twenty Opus 5.5 reading agents
in waves of at most five; the four CI causes are fixed as Phase 0 of
this arc; scope is the whole project under three lenses (correctness,
architectural correctness, principled engineering: clear pragmatic
code and meaningful accurate tests); findings are recorded, not fixed,
and remediation becomes a beads epic after the report.

Outcome: `docs/archive/REPORT-deep-review-2026-10.md` in the June
format with every confirmed finding runtime witnessed and its
reproducer committed, a filed remediation epic whose every child has a
stated closure, a version cut readiness memo, and a `main` whose CI
and documents tell the truth.

## Posture

**Fable coordinates and holds every verdict; Opus 5.5 agents hold the
reading.** No agent claim enters the report without re-verification at
the cited line, and no correctness finding is "confirmed" without a
runtime witness. June refuted one reviewer hand trace this way;
delegated agents have misreported gate results repeatedly
([[feedback-verify-agent-claims]]). Each reader's clean verdicts are
weighted by what that reader could not see, and that blind spot is
named in the report beside the verdict.

**Fable re-derives the proofs before any reader's version exists.**
The four budget derivations, the fd-k9t reached precision table, the
adjudicator widths, and the `bigconst` generator pins are done in a
coordinator session (Phase 1b) that runs while Wave 1 reads, so
"before reading any agent's version" is a fact of the schedule, not of
willpower. Readers extract dossiers on these; they never deliver the
derivation.

**Instruments before kernels.** Wave 1 reads the test suites, the
oracles, the tooling, and the documents. Its output is a verified
instrument trust table (which tests are tautological, which pins are
floors, which oracles assert faithful only, which corpora cover which
decades) and a contradiction list. Waves 2 and 3, the correctness
readers, receive both, so their "why the stack missed it" answers rest
on a verified table instead of five private guesses, and their
falsification targets start from witnessed contradictions. Wave 1 also
calibrates the fleet's refutation rate cheaply, before the expensive
wave.

**Waves, not a swarm.** Four waves (5, 5, 5, 4) plus one reserve. In
one wave no two readers own a file. Across waves a file is read under
different lenses by design (the extended trio: A3/A4 for correctness,
C3 for clarity). The one same-lens duplication is the adjudicator: the
coordinator and A2 read it with no sight of each other's notes.
Readers run through the Agent tool, `model: opus`, general purpose
type. A Workflow script with its own verify stage was considered and
rejected: the verification is the coordinator's, and a per finding
verify fan out would double the fleet.

**Record only.** Confirmed defects get a reproducer and a report
entry, never a fix on the review branch. Phase 0 is the sole exception
and is scoped to the four CI causes plus battery parity.

**Branch and commits.** `review-2026-10` off `main`. Unsigned commits
throughout, one concern per commit, each phase ending with its report
section committed and a one paragraph memory update. Recommendation,
Parnell's call: merge Phase 0 to `main` as its own signed boundary at
the Phase 1 exit so push CI is green during the arc.

**Stop losses.** Per probe: thirty minutes without new information,
write it down, move on. Per reader: a hypothesis enters the findings
table only with its splitting observation and exact command already
named, at most five per reader; everything else goes to "least sure",
which the coordinator reads but is not obliged to act on. Per wave: if
verification refutes more than a third of a reader's "reproduced"
findings, that reader's clean verdicts are discarded and its scope
goes to the reserve.

## The binding agent brief

Every reader receives the same frame; only scope, checklist, and the
artifacts attached change.

1. **Baseline gate, first action.** `git -C <repo> rev-parse HEAD`
   equals the hash in the brief, else stop and report. Readers work in
   the main checkout at the review branch tip; no worktree isolation
   (readers do not write, and worktrees cut from `main` would miss
   Phase 0).
2. **Build isolation.** `CARGO_TARGET_DIR=<scratchpad>/<reader-id>`
   for every cargo invocation, as `tools/gate_battery.sh` does per
   lane; five readers on one `target/` serialize on the lock and hit
   the stop loss waiting.
3. **Read only on the repository.** No edits, no `git add`, no
   `git commit`, no `cargo publish`. Scratch reproducers are a separate
   crate in the scratchpad with path dependencies on the repo crates.
   Readers may run `cargo test`, `cargo clippy`, `cargo kani`, and the
   Python oracles. The coordinator checks `git status` after every
   wave.
4. **Read first.** The files in scope, the ADRs that govern them, the
   relevant `docs/testing.md` section, the June report's entries for
   the area, and for Waves 2 and 3 the Wave 1 instrument trust table
   and contradiction list.
5. **Output schema.** Findings table first. Per finding: id; file and
   line; claim in one sentence; evidence (code path trace, or
   reproducer command with actual output); proposed severity (P0
   shipped wrong value or panic reachable from the public API; P1 spec
   divergence or unsound argument with no wrong value yet witnessed;
   P2 engineering quality; P3 documentation); confidence (reproduced,
   traced not run, hypothesis); **closure** for P1 to P3: the file, the
   edit class, and the test or derivation that would discharge it. A
   finding without a closure is report prose, never an epic child.
   Then "least sure" (what could not be checked and why), "found
   clean" (what was read, by what method, so the verdict carries its
   blind spot), and "design notes" (shape opinions, at most five, each
   with the evidence that would make it a finding).
6. **Falsify, do not confirm.** Oracles: `fractions.Fraction` and
   CPython `decimal` for rational cases; mpmath at 120 dps; Arb
   (python-flint) through `~/.cache/pfloat-arb-oracle/venv` for
   transcendental near boundary cases; decTest vectors for GDA
   semantics; the frozen corpora with their `.prov` files. A
   correctness finding without a reproducer is a hypothesis and
   subject to the cap.
7. **Provenance.** Reproducers derive from the spec or the failing
   case, never from another implementation.
8. **Why the stack missed it.** Every P0 or P1 names the instrument
   that should have caught it and why it did not, citing the Wave 1
   trust table where it applies.
9. **Exclusions.** Never read under `.claude/worktrees/` or `target/`.

## Coordinator verification protocol (after every wave)

1. `git status` clean. Open every cited line; a finding whose line does
   not say what the reader says is refuted on the spot.
2. Rerun every reproducer against the branch tip, output to a file
   with the exit code in band (`cmd > log 2>&1; echo EXIT=$? >> log`),
   never through a pipe ([[feedback-gate-scripts-pipefail]]).
3. For each capped hypothesis, make the named splitting observation
   and classify.
4. Classify: confirmed (runtime witnessed), refuted (with reason),
   hypothesis (what is still missing), engineering (P2/P3, verified by
   reading). Severity and closure wording are the coordinator's.
5. Commit the wave's report section and reproducers under
   `docs/archive/review-2026-10/` (scripts plus the key output lines).
   June's reproducers lived in `/tmp` and did not survive.

## Phase 0: restore the record (half session, coordinator only)

On `review-2026-10`, one concern per commit:

- **Clippy on 1.97.** `debug_assert!` with an equality comparison at
  four transcend sites becomes `debug_assert_eq!`; `rustup update` so
  local stable matches CI; one recorded decision on whether
  `rust-toolchain.toml` pins a version that CI reads, or both float.
- **`AtomicU64` on thumbv6m.** `telemetry` counters gate on
  `target_has_atomic = "64"` or narrow to `AtomicU32`, reason stated;
  `--all-features` thumb builds compile.
- **Dead ladder items under per package clippy.** `ladder_run`,
  `round_guarded`, `*_budget_dyn` get `cfg` gates that follow the
  function clusters; `cargo clippy -p ferrodec-decimal --all-features`
  is clean.
- **Registry guard.** `ferrodec-test-support` `references_integrity`
  passes: frontmatter to the ADR-0052 schema, the duplicate
  `matveev-2000` INDEX line removed, five consumer paths fixed.
- **Battery parity.** `tools/gate_battery.sh` gains the three CI
  selections it lacks: per package `--all-features` clippy for every
  member, the `--all-features` thumbv6m build, the
  `ferrodec-test-support` suite.
- **Worktree hygiene (prompt first).** Show each dirty worktree's
  `git status`, then remove the six D3 worktrees and the prunable
  battery worktree. The eight `worktree-agent-*` branches stay until
  Phase 1 has captured the splice artifact from them; then a second
  prompt deletes them.
- Remove the 2026-09-21 draft; add this plan under
  `docs/decisions/plans/`.

Exit: push the branch; `gh run list --branch review-2026-10 --limit 1`
shows all 27 jobs green; the battery is green with the new lanes.

## Phase 1: baseline and artifacts (half session, coordinator only)

In a dedicated worktree at the Phase 0 tip, with scripts committed
under `docs/archive/review-2026-10/` so every number reruns:

- The full battery in the background and the Kani suite for the four
  harness carriers.
- **Counts table:** tests per crate; conformance pass and skip per
  decTest file; Kani harness totals per crate and feature; corpus row
  counts per vector directory; registry entry count; `as` cast count
  per crate; `unwrap`/`expect`/`panic!`/`unreachable!` in non test
  source per crate. Readers receive these numbers; no reader recounts.
- **Mirror diffs,** type names normalized: `extended.rs` vs
  `extended2.rs` (to A3); `extended2.rs` vs `extended_dyn.rs` (to A4);
  every root module against its d64 and d32 twin, and d64 against d32
  (to B2). The twins are not mechanical (`fma.rs`: 776 diff lines d64
  vs d32; root 1,235 lines against 776 and 1,010).
- **S2 coverage table** from the 103 `.prov` files: per (function,
  format, decade) rows certified, and the complement (to A5 as the
  uncovered list).
- **Splice artifact:** for each of the eight `worktree-agent-*`
  branches, the diff of every file it touched against `main`
  (the D3 landing is `c27b773..8959007`, sixteen commits), so A1 can
  check the spliced `exact.rs` body by body against what each agent
  delivered. Captured before the branches are deleted.
- **ADR citation density** in `src/` across the workspace, to name
  D3's ten earlier ADRs.

## Phase 1b: coordinator derivations (one session, runs while Wave 1 reads)

Committed as a report section before any Wave 2 output is opened:

- Four budgets from scratch, then compared with the catalog in
  `ladder.rs`: `sin` (the calibration case, S2 covered), `atan` (no
  campaign coverage), `compound` (repaired in arc), `log10p1` (D1
  anchor family).
- The fd-k9t table: reached precision per (format, rung) for `recip`,
  `div`, `sqrt` from the fixed Newton step counts (two at rung 1,
  three at rung 2) with a 34, 16, and 7 digit seed; which shipped
  claims the gap touches (the 18 d32 exhaustive tables close the unary
  functions and sqrt; the D1 to D4 groups at d32 have no table).
  Whether the derivation alone discharges the bead.
- The adjudicator (`adjudicate.rs`), read in full with a defect brief:
  the widths table against `U1024`'s 308 digits (the module claims at
  most 244), exponent alignment with negative `E`, sign handling, the
  decade guard in `cmp_aligned`, the boundary set at both neighbors,
  `Equal` reachability from the public API, and the design question
  of panic versus delivering `b` under the tie rule on an embedded
  target.
- `two_over_pi_digits` and `pi_digits` from `ferrodec-multiword`
  against mpmath at several depths; the 50 and 115 digit constants in
  `consts.rs`.

## Phase 2: Wave 1, instruments and documents (one and a half sessions)

Verification by reading, mostly; this wave produces the trust table
and the contradiction list the later waves consume.

| Reader | Scope | Checklist |
|---|---|---|
| C1 | Test quality, transcendental surface. Ownership rule: every `tests/*.rs` in root, d64, d32 whose name contains `transcend`, `exact`, `pi`, `ladder`, `campaign`, `anchor`, `planted`, `lsz`, `directed`, `crosscheck`, or `differential`; `property_transcendentals`; transcend's inline tests and `mock_format.rs`; `ferrodec-test-support/src/{frozen,transcend_oracle,differential}.rs`; `ferrodec-campaign/tests/` | Tautologies (metamorphic identities sharing the kernel they test, ADR-0025 creep); pins that are floors rather than exact per bucket; oracle correlation (astro-float asserts faithful, so a 1 ULP directed misround passes); tests asserting only `is_ok()` or no panic; sampled proptest that cannot reach the band it names; `#[ignore]` discipline; test bodies duplicated across formats that drifted; `.proptest-regressions` committed only at root; `mock_format.rs` rounding versus the real formats (if it differs, every inline transcend test verdict is hollow); the planted "36 × 6" versus "36 × 30" count. Output includes the trust table rows for this surface. |
| C2 | Test quality and the oracle instruments, everything else: all remaining `tests/*.rs` in root, d64, d32, `ferrodec-decimal`, `ferrodec-multiword` (`decbig.rs`, 122 lines against 4,210 source); the three `conformance.rs` runners and `conformance_transcend.rs`; `verify/*` Kani harnesses on root, d64, d32, ieee; `regression_*`; **`ferrodec-test-support/src/{oracle,round_dec,conformance,vendored}.rs`, `tests/mpfr_gate.rs`, root `tests/oracle_soundness.rs`** under a correctness lens | The exact integer oracle (`oracle.rs`, 973 lines) is the keystone of every arithmetic claim: re-derive its rounding at each mode on constructed cases against `Fraction`, including ties, subnormal results, and CLAMPED; `oracle_soundness.rs` adequacy. Kani: vacuous `assume`, `cover!` where `assert!` was meant, ADR-0016 shim routing actually exercising the production path, what each harness proves (the count comes from Phase 1; D2 adjudicates the README claim), the d64 DPD "Kani verified" phrasing. Conformance: every skip class justified in prose; `strip_comment` fixed and tested. Regression tests pinned to the fixed value, not live behavior. |
| C5 | Tooling and infrastructure: `tools/*` (18 scripts), `.github/workflows/*`, `tools/gate_battery.sh` after Phase 0, `ferrodec-campaign/src/{sample,sweep,prng}.rs` and `src/bin/sweep.rs`, `fuzz/` crates, `verus/` | Script discipline (`set -o pipefail`, `cd "$ROOT"`, resumability audits, deterministic generators); corpus reproducibility (`freeze_campaign_corpus.py` rerun on the archived S2 outputs byte identical; `gen_argred.py` targeting a shim path); Python environment durability (no `requirements.txt` or nix shell; `verification.yml` installs unpinned mpmath; python-flint only via a sister project's venv); CI and battery selection parity after Phase 0 (`force_rung3` and `telemetry` in no CI lane; `ferrodec-campaign`'s test in neither; battery `tests` lane skipping four crates; Kani absent from the battery; `thumbv8m` in the toolchain file but never built; `ci.yml` labels d32/d64 "in development"; the `verification.yml` comment claiming it mirrors the battery); the fuzz crate manifest (no `[workspace]` table, no root `exclude`: build it and report); fuzz targets on current nightly; SHA sums per vector directory (s1 corpus absent from `corpus_integrity.rs`, `margin_lockstep.txt` unsummed). |
| D2 | Claims inventory: `README.md` and the three sibling READMEs, `docs/testing.md`, the four `KNOWN_ISSUES.md`, the seven `CHANGELOG.md` Unreleased sections, the ADR-0059/0060/0061 landing notes, `docs/references/{INDEX,verification-map}.md`, `ferrodec-transcend/src/lib.rs` module docs, `CLAUDE.md`, `docs/PLAN.md` | Every checkable sentence against the Phase 1 numbers, the code, and the workflows; verdict true, false, or stale, with corrected text. Seed list: the fifteen contradictions from the planning map, plus the d64 `ops/exp.rs` "faithful" doc. The "How ferrodec is developed" section is read for truth and never edited; any needed change is a diff proposal to Parnell. Output: the contradiction list handed to Waves 2 and 3 as falsification targets. |
| D3 | ADR coherence: ADR-0050 to 0061 in full; the ten earlier ADRs with the highest citation density in `src/` (from Phase 1); `docs/decisions/README.md`; every plan under `docs/decisions/plans/` | Does the code do what each Decision says; is each named unresolved risk still open, closed, or silently abandoned (0050 d32 replay left open; 0053 prose only bracketing; 0057 per crate constant drift; 0059 model claims, fd-k9t, S5; 0060 not specialist verified; 0061 narrow feature CI blind spot); status propagation on both sides of supersede and amend; the index stopping at 0054; decisions living only in CHANGELOGs or memory with no ADR; plan status lines stale. Output: ADR findings handed to Waves 2 and 3. |

**Coordinator verification:** by reading, plus reruns for C2's
oracle probes. Assemble the instrument trust table and the
contradiction list as a committed artifact.

## Phase 3: Wave 2, transcend correctness (three sessions: launch, verify A1 to A3, verify A4 to A5)

The core of the review. The ADR-0059 tiered claim rests on budget
soundness and classification completeness; everything else is
evidence about those premises.

| Reader | Scope (lines) | Checklist |
|---|---|---|
| A1 | `exact.rs` (3,800), `exact_pi.rs` (559); the Phase 1 splice artifact | Per classifier: the theorem, every bail site's stated proof with line numbers (the dossier), then attack value soundness (a wrong `Some` ships a wrong value) and completeness (a wrong `None` sends an exact value into the ladder, where `Equal` panics) with constructed inputs on three formats and five modes against `Fraction`: width gate edges, ties at `PRECISION + 1`, trailing zeros, negative exponents, subnormal and out of range results, `pow` at the Lauter Lefèvre boundary, `compound` at `|n|·w` = 196 and 197, `hypot` with operands decades apart, the pi family no ties theorem at its edges. Splice check: each function body in the merged `exact.rs` against what the agent branch that wrote it delivered; any body lost or interleaved is a finding. |
| A2 | `adjudicate.rs` (610), `rootn.rs`, `rsqrt.rs`, `hypot.rs`, `compound.rs`, the `powi` arm of `pow.rs` (about 2,300) | The algebraic group end to end with a defect brief. Adjudicator: widths against `U1024`, negative `E` alignment, sign, the decade guard, the boundary set at both neighbors, `Equal` reachability. Kernels: Newton rsqrt convergence, the powering arm's range, the three regime rootn bound, the compound huge `x` repair, §9.2.2 preferred exponents, at all five modes. The coordinator's independent adjudicator read (Phase 1b) is compared with this list afterwards. |
| A3 | `ladder.rs` (1,728), `extended.rs` (2,401), `extended2.rs` (1,441), `ferrodec-campaign/src/margin.rs`, the margin definitions in `tools/gen_transcend_vectors.py` and the straddle inequality in `tools/{campaign_certify,certify_anchor_floor}.py`; the Phase 1 `extended` vs `extended2` diff | **The certification chain.** What makes an S2 row evidence is three margin definitions asserted equal (generator, `margin.rs` with its locally pinned `EXT_PRECISION`, `ladder.rs::near_rounding_boundary`) and the Arb straddle inequality; re-derive the arithmetic at normal and subnormal widths, then plant one wrong side row through `campaign_certify.py` and confirm rejection. The escalation predicate: fractional ULP distance, both boundary families, exponent alignment at the subnormal and overflow edges, the tenfold pad. Which budgets have only their derivation behind them (S2 covers nine functions at d128, three at d64; the other 27 operations rest on six planted rows each). fd-k9t: a dossier only (every fixed step count, every seed precision, with lines); the coordinator holds the derivation. The three `parse_str` panics on invalid characters: reachable from the public API or not. Diff hunks classified: width parameterization, deliberate with citation, suspected defect. |
| A4 | `argred.rs` (1,713), `consts.rs` (496), `extended_dyn.rs` (2,284), `telemetry.rs`; the Phase 1 `extended2` vs `extended_dyn` diff | Three readers of one table (rung 1 at 78/38 digits, rung 2 at 182/143, the dynamic reader computing both at run time): window indexing at `1E+6144`, `9.99…E+6144`, the largest subnormal; the naive to Payne Hanek handover; inputs within one ULP of `k·π/2` at each format; the 6,408 digit table sized against the widest rung 2 window. Dynamic rung: `run3` termination and the depth cap's panic, `DynArena` borrow discipline, allocation growth per doubling, `reduce_dyn` (129 lines). Telemetry after the Phase 0 change. Diff hunks classified as for A3. Constants: extract claimed precisions and pin depths; the coordinator runs mpmath. |
| A5 | `exp.rs`, `ln.rs`, `sincos.rs`, `sincospi.rs`, `inverse_trig.rs`, `inverse_trig_pi.rs`, `hyperbolic.rs`, `pow.rs` (less `powi`), `powr.rs`, `cbrt.rs` (about 5,500); root `src/math/*.rs` (3,018, the public wrapper layer and its rustdoc theorems); the Phase 1 S2 uncovered list | The June directed mode and anchor band checklist per function and mode: small argument misrounds across the `to_format` seam (ADR-0051 residual channel on every path), exact result handling and INEXACT fidelity (ADR-0047 extension), overflow and underflow gates mode aware, negate after round, `1 ± tiny` absorption (ADR-0050 reformulations on every path that needs one), `pow` fast path boundary at `|y| ≤ 256`, `powr` at pow's tier, pi family gates (cosPi `⌈(P+4)/2⌉`, tanPi `±1` arm). Wrapper rustdoc (e.g. `src/math/exp.rs:40-52` expm1's exact set, `src/math/trig_pi.rs` no ties) checked against the kernel. Probes against Arb at the June anchor bands and at the decades the uncovered list names. |

## Phase 4: Wave 3, format crates correctness (two sessions)

June found the arithmetic pipelines, multiword carry and divide, the
ieee kernel, codecs, and total order clean; readers re-derive there
only where the diff since `3b9f063` touched or where the Wave 1
contradiction list points, and spend the rest on the lens.

| Reader | Scope | Checklist |
|---|---|---|
| B1 | Root `ferrodec` core: `bid`, `classify`, `decimal`, `digits`, `cmp`, `status`, `multiword.rs`, `ops/*` (addsub, mul, div, divide_integer, fma, rem, sqrt, round, integral, quantum, reduce, logical, rotate, shift, nan_propagate). Excludes `convert/` (B3) and `math/` (A5). | Casts in exponent and width arithmetic; tables sized per format; named constants pinned by value (the June `MIN_POSITIVE_NORMAL` class: is every constant pinned by a test that reads its value?); §9.2.2 preferred exponent arithmetic (`i128`, saturation); CLAMPED and BID residual (ADR-0048); `rem_near` ties; `ops/quantum.rs` (12 panic sites). |
| B2 | `ferrodec-decimal64` and `ferrodec-decimal32`: the full module set less `convert/` (B3), read side by side with the root using the Phase 1 diffs; all ten sibling `ops/{exp,hyper,trig,trig_pi,pow,powr,rootn,rsqrt,hypot,compound}.rs` wrappers (2,268 per crate) against root `math/*`; `transcend_impl.rs` against `format_impl.rs` | Every non mechanical divergence classified: width parameterization, deliberate with citation, suspected defect. Sibling specific: every table sized for the format (the June d64 `POW10_U64` class), ADR-0030 subnormal single rounding, the ADR-0057 sentinel duplicated per crate, the d32 removal of `ExponentOutOfRange`, the wrappers' public docs against the kernel's actual tier. Panic hotspots: d64 `ops/quantum.rs` (19), `ops/rem.rs` (18). |
| B3 | The I/O boundary on all three formats: `convert/{parse,format,binary,int,mod}`, `dpd`, `serde_impls`, `num_traits_impls`, `ops_traits`, `iter` | Threat model first: attacker supplied strings and bytes. `parse_str_inner` (about 210 lines) at every length and exponent edge; NaN payload canonicality (June's `[10^33, 2^110)` class); ADR-0057 saturation; ADR-0058 engineering notation; ADR-0055/0056 `from_f64`/`to_f64` fidelity and the "deliberate under report"; DPD against the dqEncode vectors and the boolean equations; serde under non self describing formats; `Num::from_str_radix` radix handling; operator trait rounding defaults; `convert/format.rs` (14 panic sites). |
| B4 | `ferrodec-decimal` (7,008), including `src/transc/*` (1,941) | `DecBig` normal form (`from_ascii_digits` debug_assert only, June); ADR-0053 exponent gap guards on every path that materializes `10^gap`; ADR-0054 `NonZeroU32`; `i128` cohort arithmetic; `arith.rs` (13 panic sites); `interop`; the 99 skipped GDA vectors each justified. Seeded P1 hypothesis: `transc/strategy.rs` reaches the Ziv cap and returns through `faithful_round` with the same status shape as a decided result, so a caller cannot tell a faithful fallback from a correctly rounded value; check the module's own stated contract and whether the cap is reachable. `transc/{exp,pow}.rs` changed since June. |
| B5 | `ferrodec-multiword` (4,210) and all of `ferrodec-ieee` (914, including `binary.rs`, changed since June) | `U256..U1024` carry, shift, divide (June's Algorithm D add-back branch with zero coverage: covered now or not); the `bigconst` module's claimed bounds and pin depths extracted (the coordinator ran mpmath in Phase 1b); `DecBig` substrate; the ieee rounding kernel and `Status` bit algebra; `binary.rs` semantics. |

**Coordinator's own probes:** read the multiword divide and the
preferred exponent changes in full regardless of B1 and B5;
falsification probe for one new pin per crate (break a constant,
confirm which test catches it).

## Phase 5: Wave 4, architecture, clarity, registry (one and a half sessions)

| Reader | Scope | Checklist |
|---|---|---|
| C3 | Clarity and pragmatism, `ferrodec-transcend` | `exact.rs` as one 3,800 line file; the extended trio in the form the coordinator can verify: for each function present in all three `extended*.rs`, the line count triple, whether bodies differ beyond width constants, and `git log -L` evidence of a fix landing in one copy and not the others (the drift cost); function length; rustdoc versus code (stale: `lib.rs:77` "until M8 wires the ladder", `format.rs:4` "Decimal64 / Decimal32 later"); every `expect(` message; the 241 `as` casts with no bound comment (all cast lints allowed workspace wide); dead code; `#[allow]` attributes; naming across rungs. Shape opinions to design notes with evidence. |
| C4 | Clarity and pragmatism, root, d64, d32, `ferrodec-decimal`, `ferrodec-multiword`, `ferrodec-ieee`; `examples/*` and `benches/*` | The large functions (`add_inner` about 215 lines, `parse_str_inner` 210, `fma` 205, `quantize` 183, `finalise_finite` 167): long because the algorithm is, or because concerns mix; the workspace clippy allow list (24 allowed pedantic lints, each with the evidence it earns its place or not); shim only root modules (`src/math/{argred,extended}.rs`, `#[allow(unused_imports)]`); comment accuracy against code; mirror drift as maintenance burden with concrete cost examples from the Wave 3 diffs; `ferrodec-decimal` `expect(` density (60); examples compile and demonstrate what they claim. |
| D1 | Architecture across the workspace | The feature graph, verified not argued: per crate, per feature singly, `cargo build --target thumbv6m-none-eabi --no-default-features --features X` with the log (this is also the fd-z26 residue check); `telemetry` test only yet public; `unbounded-ladder` pulling `alloc`; siblings imply `binary-float` where the root does not. no_std and alloc posture per crate. Public API consistency across the three formats (names, `Status` semantics, preferred exponents, operator defaults). Type level invariants (canonical encodings, `NonZeroU32`, `Status` as a set; where a runtime check stands in for a type). The `DecimalFormat` seam (`format.rs`). Dependency hygiene; MSRV 1.84 checked for the root only; `pub` surface that should be `pub(crate)`; semver exposure of test lane cfgs. Drift cost evidence for the three sibling crates collected here; the ADR-0011 judgment (one generic crate or three) is the coordinator's synthesis over verified Wave 3 verdicts and a decision for Parnell. |
| D4 | Reference registry, licensing, provenance | 53 entries against `SCHEMA.md` after Phase 0; Wayback capture present and dated for every load bearing URL; license recorded before every vendored copy (decTest ICU license, LSZ CC BY 4.0 attribution in `gen_lsz_d64_exp.py`, the campaign corpora's own license); `cowlishaw-dectest.md` coverage gaps 1, 4, and 5 stale; consumer paths resolve; the six `failure-*` entries current against KNOWN_ISSUES; `verification-map.md` rows each pointing at a live artifact; the entries this arc must add for every source the report cites. |

**Reserve reader (R).** Held for a second independent read of
whichever module Waves 2 or 3 make hot (a refuted third of a reader's
output, or a confirmed P0 whose neighbors share the pattern), or for
the adjudicator if the coordinator's and A2's lists diverge sharply.
If never needed the fleet closes at nineteen.

## Phase 6: report, epic, memo (one and a half sessions)

- Assemble `docs/archive/REPORT-deep-review-2026-10.md` from the per
  phase sections in the June format: Method (with an agent roster
  table: reader, scope, method, blind spot), Verdict, Confirmed
  findings (runtime witnessed), Refuted, Verification infrastructure,
  Design and documentation tier, Found clean, Why the stack missed it,
  Remediation program.
- Reproducers under `docs/archive/review-2026-10/` with a `README.md`
  naming the oracle environment each needs.
- The remediation epic in beads: one child per confirmed finding and
  per P1 to P3 finding that carries a closure, `discovered-from`
  links, remediation order as in June (constants and tables, then
  premise repairs, then infrastructure, then documents). Findings
  without a closure stay report prose. Close the review epic with a
  pointer to the report.
- Version cut readiness memo as a report section: the standing
  proposal is 4.2.0 lockstep for the formats, `ferrodec-transcend`
  0.4.0, `ferrodec-multiword` 0.3.0, `ferrodec-ieee` unchanged; each
  Unreleased section checked for breaking changes; transcend 0.3.0
  and multiword 0.2.0 are untagged and `ferrodec-decimal`'s
  Unreleased section is empty; the tag naming changed from
  `ferrodec-vX` to `vX` at 4.1.0. Parnell publishes.
- Registry accretion for every source the report cites.
- Memory update: the arc record, the fleet lessons, the reader blind
  spots that mattered.
- Stop before the signed merge; present the slice state and the draft
  merge message, and ask.

## Out of scope

- Fixing anything beyond Phase 0; remediation is its own arc.
- Performance.
- Executing the version cut or any publish.
- The fd-k9t AWS replay (analytic route only) and the S5 specialist
  verification of the μ(π) route.
- Editing the "How ferrodec is developed" disclosure.

## Session estimate and outside view

Phase 0 half; Phase 1 half; Phase 1b one; Wave 1 one and a half;
Wave 2 three (the only wave whose verification is numeric reruns
rather than line reads, split across two sessions); Wave 3 two;
Wave 4 one and a half; Phase 6 one and a half: eleven and a half, so
eleven to thirteen sessions. June's review took one session with six
agents and a single verification pass; the September draft estimated
six to eight for a narrower scope with eight agents. The added width
is the two engineering lenses, the whole project correctness scope,
and the reproducer discipline; the upper bound is the honest one.

## Verification of the plan's own execution

- Phase 0 exit: `gh run list --branch review-2026-10 --limit 1` green,
  all 27 jobs; `tools/gate_battery.sh` green with the new lanes;
  `cargo test -p ferrodec-test-support` green.
- Phase 1 exit: the counts script, the three diff artifacts, the S2
  coverage table, the splice artifact, and the ADR density list all
  committed under `docs/archive/review-2026-10/`.
- Every wave exit: `git status` clean apart from report and reproducer
  files; every "confirmed" row has a reproducer script under
  `docs/archive/review-2026-10/` that reruns from the tree with the
  documented environment; every epic bound finding has a closure; the
  wave's report section is committed.
- Phase 6 exit: `bd list` shows the review epic closed and the
  remediation epic open with one child per closure bearing finding;
  the report's counts table matches a rerun of the Phase 1 script;
  the memory index carries the arc entry; push CI on the branch green
  at the tip.

## Critical files

- `ferrodec-transcend/src/{exact,adjudicate,ladder,extended,extended2,extended_dyn,argred}.rs`
- `ferrodec-campaign/src/margin.rs`, `tools/gen_transcend_vectors.py`, `tools/campaign_certify.py`
- `ferrodec-test-support/src/{oracle,frozen,vendored,references,conformance}.rs`
- `ferrodec-decimal/src/transc/strategy.rs`
- `tools/gate_battery.sh`, `.github/workflows/{ci,verification}.yml`
- `docs/testing.md`, `README.md`, `KNOWN_ISSUES.md`, `docs/decisions/README.md`
- `docs/archive/REPORT-rigorous-review-2026-06-09.md` (format precedent)
