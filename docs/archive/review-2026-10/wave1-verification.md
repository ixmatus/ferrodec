# Wave 1 verification: instruments and documents

Coordinator's verdicts on the Wave 1 readers (C1, C2, C5, D2, D3),
launched at `864e561`. Every finding below was opened at its cited
line; reproducers were rerun; severity and closure wording are the
coordinator's. Reader reports are saved verbatim under `readers/`.
Verdict vocabulary: confirmed (runtime witnessed or, for a document
claim, the cited text says what the reader says and the fact it is
checked against holds), refuted (with the reason), engineering
(P2/P3, verified by reading), hypothesis (what is still missing).

## D3: ADR coherence

Seventeen findings, all `traced`; one computation. The reader kept the
tree clean and ran one cargo command. Two line citations were wrong
and are corrected below; every substantive claim held.

| id | verdict | ruling | note |
|---|---|---|---|
| D3-1 | confirmed | P1 (record) | ADR-0033:150-164 bounds the kernel error against the SIZE of a half ULP ("10^12 of headroom against the format's half ULP"), which proves faithful rounding only; 0033:218-221 calls that argument "the load bearing fact" that makes the unscanned decades a coverage gap. ADR-0059 S1 refuted it with 1,819 certified misrounds; 0033's status line marks only ADR-0050. The unsound argument is still presented as load bearing. Closure: status line and addendum on 0033 citing the S1 corpus. |
| D3-2 | confirmed in substance | P2 | The ADR-0032 status line does name ADR-0059's amendment (the reader said it says only "amended"), but the body's honesty amendment still closes with "The bound holds across every decade the table covers" (0032:142-143), which S1 falsified. Closure: a falsified banner in the ADR-0045 style on the body. |
| D3-3 | confirmed in substance | P2 | ADR-0051 is 143 lines; the reader's 201-221 does not exist. The content is at 0051:51-68 and 106 ("anchor equality test", "rather than approximated by a decade threshold"). The code ships a `10^-47` relative snap band (`sticks_to`, extended.rs:366-374) whose derivation lives only in the rustdoc at extended.rs:339-364 and in commit 31f1ac8. Closure: ADR-0051 addendum carrying the derivation. |
| D3-4 | confirmed | P2 | The root file is `src/ops_traits.rs` (the reader wrote `src/ops/`). ADR-0027's Decision names bare `rem` and `%` as the hazard to be "removed or reserved" at 2.0; 2.0 kept `%` with per format mathematics and the rustdoc at ops_traits.rs:61-63 (root) and the sibling twins says "ADR-0027 records the per-format choice", which it does not: it records the 1.x meaning as the ambiguity to retire. The deviation is recorded only in CHANGELOG [2.0.0]. Closure: amend ADR-0027/0029 with the `num-traits` reason, or remove `Rem` at the next major; fix the three rustdoc pointers either way. |
| D3-5 | confirmed | P2 | All six anchors say "proved" or "exhaustively verified" without ADR-0050's scope note; ADR-0050:124-125 claims the rustdoc no longer does. README.md:73 is inside the disclosure section and is handled only as a diff proposal to Parnell (D2 carries it). Closure: the five non disclosure anchors, or the deferred Decimal32 replay (which is also the fd-k9t closure). |
| D3-6 | confirmed | P2 | `ferrodec-decimal` `to_f64` sets INEXACT unconditionally (to_float.rs:72) and its doc justifies that "as with `Decimal128::to_f64`" (to_float.rs:37-39); the root decides INEXACT exactly since fd-aqs.12 (binary.rs:140-155). A contract divergence between crates with a false doc premise. Closure: decide INEXACT exactly in `ferrodec-decimal` (the string is already exact), test `ONE.to_f64` reports no INEXACT, and an ADR-0055 addendum. |
| D3-7 | engineering | P3 | Index stops at 0054; no "amended" lifecycle state in README or template. Confirmed by the planning map. |
| D3-8 | engineering | P3 | Eleven one sided marks; the propagation list is accepted on the reader's reading. |
| D3-9 | engineering | P3 | ADR-0047 unmarked after M7 moved the proofs test side. |
| D3-10 | engineering | P3 | Thirteen ADRs cite plans outside the tree (`~/.claude/plans/` or bare slugs). A durability defect in the record: the cited plans cannot be read by a 2040 maintainer. |
| D3-11 | engineering | P3 | Pairs with D2-11/D2-12/D2-27. |
| D3-12 | confirmed | P3 | Reproduced by exact integer arithmetic (`5^k` digit counts); ADR-0059's tie list omits two formats' entries and says 17 where 20 shipped. |
| D3-13 | engineering, escalated to Wave 2 | P3 pending | ADR-0059 states the Tier 2 figure as an expected count over the whole domain per function; four restatements call it a per call rate, off by about `10^38`. The coordinator's Phase 1b arithmetic hit the same confusion. Handed to A3 to settle which quantity the claim should carry; the report's claim wording depends on it. |
| D3-14 | engineering | P3 | Tag naming changed to `vX` at 4.1.0; identity bearing, so the closure is Parnell's decision (the version cut memo carries it). |
| D3-15 | engineering | P3 | `telemetry` as a public feature versus the `--cfg` lanes; pairs with D1. |
| D3-16 | engineering | P3 | ADR-0011's rationale predates the generic kernel; pairs with D1 and the coordinator's synthesis. |
| D3-17 | engineering | P3 | Stale plan status lines. |

Leads from "least sure", dispositioned: the adjudicator `Equal` panic is the Phase 1b design question (A2); `ferrodec-decimal/src/arith.rs:248` `u32::try_from(...).expect` panics at precision `u32::MAX` with a one digit operand, legal under `NonZeroU32` and acknowledged in the comment: engineering P3 for B4 (closure: a documented maximum on `Context` or saturation); the NaN `to_f64` doc overclaim goes to B3; `trig-pi` implying `binary-float` on the siblings goes to D1; the unreconciled rates go to A3 with D3-13.

Blind spot of this reader: no Decision versus code check for ADR-0001 to 0049 outside its ten.

## D2: claims inventory

Thirty findings and one hypothesis. Twenty five cited lines were
opened by the coordinator; all say what the reader says. The three
reproduced findings reran from the reader's scratch crate with the
same output. The reader kept the tree clean.

| id | verdict | ruling |
|---|---|---|
| D2-1 to D2-30 | engineering | P3, every one a documentation claim that is false or stale at the cited line. Grouped for remediation: (a) README overview and quick start (D2-2, 4, 5, 13, 15, 16, 17); (b) README verification list and accuracy prose (D2-1, 3, 6, 7, 8, 9, 10, 14, 28); (c) `docs/testing.md` Part I and blind spots (D2-10, 11, 12, 26, 27); (d) the three sibling and decimal `KNOWN_ISSUES` and READMEs (D2-20 to D2-23, D2-29); (e) CHANGELOGs (D2-18, 19, 30) and the registry (D2-24); (f) `ferrodec-transcend/src/lib.rs:44-50` (D2-25). D2-18 and D2-19 are numeric errors in release notes; D2-23's 1152 versus 1154 and the 27,591 versus 27,593 totals are the hand rendered table drifting from the pins, which is the design note's point. |
| D2-H1 | refuted | The coordinator's Phase 1b probe (`reproducers/P1B-1`) is exactly the splitting observation the reader named: the minimum of `||x·2/π||` over every Decimal128 input with `|x| ≥ 0.1` is `1.317·10^-38`, so the 76 digit reduced fraction is never zero, `cos_ext.is_zero()` is unreachable at rung 1, and the 143 digit rung 2 fraction likewise. The reader's own measurement (the inputs nearest `π/2` and `3π/2` return finite values near `2·10^33`) is consistent. Not a P0; D2-3 (the README and sincos.rs doc that say `±∞` is returned) stands as P3. |

Also confirmed from the reader's leads: `ferrodec-multiword/src/bigconst.rs:437-487` has `inv_pi_digits` inserted inside the doc comment written for `inv_ln10_digits`, so the public `inv_ln10_digits` carries no rustdoc and `inv_pi_digits` carries the wrong one (engineering P3, closure doc; B5 confirms the generators themselves are correct, as Phase 1b measured). `force_rung3` runs in no workflow (C5). The verification map lacks rows for every ADR-0059 instrument.

The disclosure section diff proposal (five items) is carried in the reader's report verbatim and goes to Parnell unedited: the "754-2008" primary source sentence, the CI sentence, the crate list missing `ferrodec-campaign`, the Decimal32 "exhaustively verified" scope, and the ladder premise sentence omitting Decimal32.

Blind spots of this reader: sizes and benchmarks not rebuilt; Kani names not run; the conformance suite not run (the battery covers it).

## Contradiction list handed to Waves 2 and 3

From D2 and D3 together, the falsification targets by owner:

- A3/A4 (ladder, reduction): the ADR-0032 envelope still stated as live (README.md:301, testing.md:12-27, 442-443, verification-map.md:33); the Tier 2 figure's units (D3-13); the Phase 1b cancellation depth premise (P1B-1).
- A5 (kernels): tan `±∞` doc (D2-3); the sibling wrapper rustdoc still saying "faithful ≤ 1 ULP" (d64/d32 `ops/{exp,hyper,pow,trig}.rs`); `POW`'s itemization (Phase 1b).
- B2 (siblings): the wrappers' public docs; d64/d32 KNOWN_ISSUES and CHANGELOG numbers (D2-18, 19, 21, 22).
- B3 (I/O boundary): NaN `to_f64` doc; `%` per format routing (D3-4).
- B4 (decimal): `to_f64` INEXACT (D3-6); arith.rs:248; base.decTest 1152 versus 1154 (D2-23).
- B5 (multiword): the bigconst doc merge.
- C5: `force_rung3` in no workflow; verification.yml "four lanes"; MANIFEST has no seed key (D2-15).
- D1: `telemetry` as a feature (D3-15); sibling `trig-pi` implies `binary-float`; ADR-0011 (D3-16).

<!-- C1, C2, C5 sections follow when their reports arrive. -->

## C1: test quality, the transcendental surface

Thirty five findings; the reader used three forks of itself for the
exact family, the pi family, and the inline modules (the roster
records them). Its report file was written (41 KB, saved under
`readers/C1.md`); the reproducers live in the session scratchpad
under `C1/` and the ones the coordinator reran are copied to
`reproducers/`.

| id | verdict | ruling | note |
|---|---|---|---|
| C1-1 | confirmed | P1 (instrument) | `ferrodec-test-support/src/transcend_oracle.rs:172-187` `cmp_approx` calls astro-float `abs_cmp`, which astro-float-num 0.3.6 documents as an absolute compare and implements as `v1.cmp(v2)` (a signed compare; `ext.rs:352-355`, read in the registry source). So `within_faithful_ulp`, `faithful_side_ok`, and `assert_faithful` accept any result BELOW the true value; the reader's reproducer, rerun by the coordinator, shows `exp(1)` accepting 0, 1.5, and −7 as faithful, and a `1 + 1e-50` anchor accepting 1 under TowardPositive. Every `assert_faithful` consumer (the `property_*` transcendental suites on all three formats) is blind on one side. No shipped wrong value is witnessed; the frozen corpora (exact equality) cover what they cover. This is also an upstream astro-float defect worth reporting. Closure: compare absolute values (or the signed difference against ±tol), scale the `1e-300` floor to the format's smallest subnormal, and negative control tests that must fail. Blast radius probe below. |
| C1-2 | confirmed | P2 | `tools/diff_oracle.py` returns NaN for a negative cube root (mpmath's complex result); the coordinator reran the printf probe. Every negative `cbrt` differential case is silently skipped as "mpmath absent". |
| C1-3 | engineering | P2 | Sibling differentials keep a two step band and a floor; accepted on reading. |
| C1-4 | engineering | P2 | A driver crash turns the differential green (None treated as skip); accepted on the reader's trace, the control flow is visible at `differential.rs:142`. |
| C1-5 | engineering | P2 | Eleven functions have no directed mode rows outside the anchor bands at Decimal64 and Decimal32, and `acosh` has none anywhere below Decimal128; combined with C1-1 a mid range directed misround there is invisible to every committed instrument. The coordinator's spot check of `acosh.txt` was inconclusive (comment lines); the reader's per (function, precision, mode) scan is accepted and the remediation pins the coverage matrix (design note 4). |
| C1-6, C1-7, C1-8 | engineering | P2 | Pi family tests: directed values pinned only on closed form rows; the sweep skips rows the kernel calls exact and asserts `checked > 50` (coordinator confirmed the floor at `tests/transcend_sinpi.rs:391`); kernel sharing identities. The reader's mutants M1/M2 passing all 60 d64 tests is the witness. |
| C1-9 | engineering | P3 | Module doc declares the faithful contract and a directed-neighbour claim C1-1 falsifies. |
| C1-10 | confirmed, coordinator's own regression | fixed in 70adcca | The transcend inline tests failed to compile at default features (86 errors) and at every single cluster at `864e561`; they compiled at the Phase 0 base `0a5be4c`. Cause: the Phase 0 commit 57906c8 removed catalog items by cluster. Fixed by converting the gates to conditional dead-code allows; every selection now builds and the suites are unchanged. The reader caught a defect the arc itself introduced, which is the review working as designed. Pre-existing and separate: with `exp-log` or `trig-pi` alone, `cargo clippy --lib -D warnings` already failed at `0a5be4c` on dead items in `exact.rs` and `adjudicate.rs` (reader D1 seed; no lane builds those selections). |
| C1-11 | confirmed | P2 | `mock_format.rs` has no rounder (`round_and_pack_finite` is `unreachable!`, lines 98-128 region), so no inline transcend test reaches a final rounding; the reader's rsqrt and rootn special value mutants passing is the witness. |
| C1-12, C1-13, C1-14 | engineering | P2 | Test local copies of gate constants compared with themselves; witness tests that never call the classifier they name; a `u128` tautology. Accepted on the reader's mutation runs (`fork-transcend/mutations.diff`, 170 passed under every mutant). |
| C1-15 | engineering | P2 | Only 38 of 6,408 table digits pinned; the table is correct today (the coordinator's Phase 1b generator pins agree). |
| C1-16 | confirmed | P2 | `tests/ladder_telemetry.rs:236-248` pins a per file aggregate of 20; compensating row swaps pass. |
| C1-17, C1-18, C1-19 | engineering | P2/P3 | No sibling replay of the GDA transcendental files (the only externally authored directed rows); `margin_lockstep` and d32 `ladder_telemetry` run in no CI lane (the Phase 0 battery `members` lane does not include `ferrodec-campaign`; C5 pairs); only `*.txt` corpus files are hashed. |
| C1-20 | engineering | P3 | Pairs with D2-11. |
| C1-21 to C1-25 | engineering | P2/P3 | Sibling directed gate drift, dropped reflection tests, "bitwise" tests using `partial_cmp`, a mislabelled closed form, "1 ULP" checks that are really ten. Accepted on reading. |
| C1-26 | confirmed | P2 | `ferrodec-decimal64/tests/property_transcendentals.rs:33` tolerates about `1e-13` relative on functions claimed correctly rounded; the header's libm premise is false. |
| C1-27 | confirmed | P2 | `tests/transcend_exact.rs:333-357`: the fd-clc saturation test accepts any finite value on the overflow side under TZ/TN and `is_zero() || is_finite()` on the underflow side (coordinator read the lines). |
| C1-28 to C1-30 | engineering | P2 | The powering arm never reached; a directed bracket admitting a coherent misround; sibling drift in the exact family. Accepted on the reader's reproducer sections R1 to R4. |
| C1-31, C1-32, C1-33, C1-34, C1-35 | engineering | P2/P3 | Floors (coordinator confirmed `>= 19`, `> 800`, `>= 400`, `> 2000` at the cited lines), a discarded §9.2.2 column, a composition check called independent, a hard coded gate copy. |

Leads from "least sure": the C1-1 blast radius probe is the coordinator's (below); the siblings DO commit `.proptest-regressions` (the brief's item 6 premise was wrong; corrected in the record); the root `--all-features` push lane was run by the Phase 0 battery (`members`, PASS); CLAMPED masked in the hypot oracle comparison goes to B1; the stale libm prose in the d64 verify modules goes to C4.

Found clean, accepted with the reader's stated blind spots: the frozen pin tables (exact per function and mode; 25,354 rows scanned with zero anomalies), the three corpus replays, the S1 and S2 replays, the anchor bands, `transcend_directed`'s constants (re-derived by the reader with mpmath), `conformance_transcend`, `margin_lockstep`'s integers, `within_n_ulp_band`, and the inline suites the forks confirmed. The d128 and d64 cross checks contain no transcendentals; the June report and the brief described a cross check that does not exist for them.

The reader's instrument trust table is adopted as the Wave 2 and 3 input (in `readers/C1.md`).

## C5: tooling, scripts, continuous integration

Twenty two findings; the reader kept the tree clean, built the fuzz
crates from scratch copies, and ran every CI selection the battery
lacks. Report saved verbatim under `readers/C5.md`.

| id | verdict | ruling | note |
|---|---|---|---|
| C5-1 | confirmed | **P0** | `asinh` panics on every format once `x²` overflows the format (`1E3073`, `1E193`, `1E50`): the Newton square root seeds through `to_format`, gets an infinity, and `from_format` panics. Reader's reproducer rerun by the coordinator with identical output. |
| C5-2 | confirmed | **P0** | `atan2(y, x)` panics on every format once `1/x` overflows the format: the reciprocal seed takes the same path. The coordinator reproduced the siblings too (`d64 atan2(1, 1E-398)`, `d32 atan2(1, 1E-101)`). |
| coordinator's widening (P1B-2) | confirmed | **P0**, same class | `acosh` panics at the same thresholds on all three formats, and `atan2_pi` on the siblings at `(1E-390, 1E-398)` and `(1E-95, 1E-101)`; `rsqrt`, `hypot`, `rootn`, `cbrt`, `atan`, `atan_pi`, `powi`, `pow`, `powr`, `tanh`, `atanh`, `asin`, `acos` survive the same edges (20 of 116 probed calls panic; `reproducers/P1B-2`). On the calculator target a panic is a reset. The Phase 1b Newton read noted the format round trip for precision and missed it for range; the fuzz target that exists to catch exactly this has not built since 2026-05-09. Closure: seed from a rescaled operand, or dispatch the out of range ratios before the kernel; threshold rows per format and mode; a fuzz lane that builds. Filed as a P0 bead at review time rather than waiting for the remediation epic. |
| C5-3 | confirmed | P2 | The three fuzz crates are orphans (no `[workspace]` table, no root `exclude`) since the workspace conversion at d87ae0a; the reader's `cargo metadata` failure and scratch-copy builds are the witness. The dead instrument behind C5-1 and C5-2. |
| C5-4 | confirmed | P2 | Parity residue after Phase 0, with the reader's matrix adopted: selections that run nowhere are d32 `--all-features` tests, `ferrodec-campaign` tests, thumbv8m builds, and fuzz; the reader ran the 19 CI only selections and the two orphan suites and all pass today. |
| C5-5 | confirmed | P2 (process) | Phase 0's CI fixes have never run on GitHub: the branch has no upstream (the push needs the YubiKey) and `ci.yml` fires only on push or PR to main. The question goes to Parnell with the worktree prompt. |
| C5-6 | engineering | P2 | Launchers run `$ROOT/target/release/sweep` while the build honours `CARGO_TARGET_DIR`; accepted on reading. |
| C5-7, C5-8 | confirmed | P2 | Resume restarts the histogram and footer counters (reader's two step run); `decade_of` assumes 34 digits so the d64 histograms sit 18 decades high (the archived d64 shards bucket 33..388). Neither affects the frozen corpus rows; both affect the campaign's own bookkeeping. |
| C5-9, C5-10, C5-11 | confirmed | P2, P3, P2 | The S2 certification fan-out has no in tree driver and the sweep outputs exist only on this machine (a durability defect in the review's strongest evidence); the freeze is otherwise byte identical from the archive apart from the recorded commit; 940 trig rows tagged `D` inside the threshold are excluded from the candidate set without a MANIFEST note. |
| C5-12 | engineering | P2 | Shard provenance (no bounds or build id in the header, stream key independent of format and bounds, resume trusts any checkpoint); accepted on reading, pairs with design note 3. |
| C5-13 | confirmed | P2 | `gen_argred.py` and `gen_bigconst_oracle.py` die on this machine's only interpreter (3.9.6, unguarded `set_int_max_str_digits`); nothing pins Python, mpmath, or python-flint. The coordinator's own Phase 1b constant checks ran with `/usr/bin/python3` and the user site mpmath, which is exactly the unpinned dependency. |
| C5-14, C5-15 | confirmed | P3 | Generator docstrings point at a shim and omit the second regeneration step; the reader regenerated byte identically with the corrected recipes. |
| C5-16, C5-17 | engineering | P3 | Pairs with D2-11 and D3-11; the `verification.yml` "mirror" comment is false. |
| C5-18 | confirmed | P3, fixed in the wave close | The Phase 0 preflight printed both the version line and the "unavailable" fallback in the coordinator's own battery log, because `rustup check` exits 100 when an update exists. |
| C5-19 | confirmed | P3 | Pairs with C1-19. |
| C5-20 | confirmed | P3 | The Verus pilot's recovery recipe points at a path no ref ever held. |
| C5-21 | engineering | P3 | README item 5 presents a harness that cannot run. |
| C5-22 | hypothesis | P3 pending | Platform libm in the pow-edge steering; the splitting observation needs a Linux host and is deferred to remediation. |

Found clean, accepted: the battery and launcher script discipline (pipefail, own root, completeness audit), the stateless PRNG, the byte identical regeneration of the planted, lockstep, LSZ, and anchor band corpora, the value identical regeneration of the argred table and the Extended2 constants, every SHA256SUMS directory, and `verification.yml` against the README. The environment manifest is adopted as the durability baseline: a fresh machine needs seven things and the tree pins none of them.

## Wave 1 summary

- P0: one class, the Newton seed through the format (`asinh`, `acosh`, `atan2`, `atan2_pi`), found by the instrument the arc revived.
- P1: the one sided faithful oracle (C1-1), ADR-0033's unsound backstop still presented as load bearing (D3-1), and the coordinator's own P1B-1 (the cancellation depth premise).
- One regression introduced and fixed inside the arc (C1-10, 70adcca).
- The fleet's refutation rate: one hypothesis refuted (D2-H1), two line citations corrected (D3-3, D3-4), one brief premise corrected by a reader (the siblings' `.proptest-regressions`), and no finding refuted outright. Clean verdicts are adopted with their stated blind spots.
