# Wave 4 verification: architecture, clarity, the record

Coordinator's verdicts on the Wave 4 readers (C3, C4, D1, D4), launched
at `576f7bb`. This wave reads for principled engineering and the
durability of the record, so most findings are engineering (P2) or
documentation (P3) verified by reading; every cited line was opened,
and every `reproduced` claim was rerun. Reader reports are saved
verbatim under `readers/`.

## C3: clarity and pragmatism, `ferrodec-transcend`

Twelve findings; the reader quantified the extended trio's drift with
`git log -L`, measured dead code by stripping the allows in a scratch
copy, and built the in crate real format test the crate says it cannot
have. Every cited line says what the reader says; the three
reproducers (the cohort probe, the stripped allow build, the in crate
probe) were rerun by the coordinator with identical output.

| id | verdict | ruling | note |
|---|---|---|---|
| C3-1 | confirmed | P2 (the structure behind fd-k9t and the Newton class) | extended.rs:786 and :823 loop `0..2`, extended2.rs:654 and :683 loop `0..3`, and `newton_steps` exists only in extended_dyn.rs:1167; the rung 1 bodies are unchanged since 4479043 (2026-05-17) while the derivation landed in 4422333 (2026-08-01). The files' own rule (extended2.rs:23-24, extended_dyn.rs:44-48: "any behavioural divergence beyond width is a defect") makes this a known defect of two months. The trio table (27 functions, 4 behavioural divergences, 2 carrying an open defect, one doc fix in one copy) is adopted. Closure: one generic `newton_recip`/`newton_sqrt` over `ExtNum` with the derived step count; the class fix of finding 2 lands there once. |
| C3-2 | confirmed | P2 | argred.rs:228 `FRAC_DIGITS = 76` and :271 `FRAC2_DIGITS = 143` fold the false depth 33 invisibly; :234, :275 and :909 carry a true `33` that is the coefficient overlap (`PRECISION − 1`), documented at :887-892. A search and replace closure of P1B-1 would corrupt the window offsets. Closure: `CANCEL_DEPTH` per format and `COEF_OVERLAP` named apart, with const assertions. The literal family table is adopted. |
| C3-3 | confirmed | P3 | Ten `#[allow(dead_code)] // reached at M8` attributes (grep count 10) suppress nothing: M8 landed at 221950c and the stripped copy builds clean under `trig`; lib.rs:77 and format.rs:4 still tell the "until M8" and "Decimal64 / Decimal32 later" stories. Doc and cfg. |
| C3-4 | confirmed | P2 | Rerun in the reader's copy with the three file wide `#![allow(dead_code)]` (extended.rs:56, extended2.rs:31, extended_dyn.rs:56) removed: `E_EXT2_STR`, `e_ext2`, `ExtNum::{e, saturate_overflow, saturate_underflow}` and `Extended2::{saturate_*, from_components}` are dead in the shipped `transcendentals` build; the kernels call `Extended::saturate_*` directly on every rung. Closure: item level gates, the dead trait members deleted. |
| C3-5 | confirmed | P2 | exact.rs:1461, :1854, :2072, :2553 carry the identical `du > 127 || dv > 55` assembly; 6ad73ff (fd-clc) added pow's exponent window that powi "carried from day one", and 114ddda the same day introduced a second window constant (`POWER_OF_TEN_EXPONENT_LIMIT` 1,000,000 against `EXACT_EXPONENT_WINDOW` 99,999); `rootn` relies on `i32::try_from`. One shipped defect already came from the copies. Closure: one assembly function, one window constant, the fd-clc regression extended. |
| C3-6 | confirmed | P3 | 32fe07c touched extended.rs only (17 lines), citing the 1.8e-11 margin A3-2 refuted; extended2.rs:324 cites ADR-0033; extended_dyn.rs says "many orders"; the rung 1 header still calls rung 1 "the proof envelope". Joins A3-2's closure. |
| C3-7 | confirmed | P3 | The Newton rustdoc is false differently in each copy (one iteration; budgets that account for the seed; three iterations; a `PRECISION` digit seed). Rewritten after C3-1. |
| C3-8 | confirmed | P2 | The `expect` at extended.rs:170, extended2.rs:146 and extended_dyn.rs:464 ("NaN / Inf are dispatched at the public-API boundary") is byte identical in three copies and false by fd-0o9; `Extended::from_format` is `pub`. The recurring shape: a prose precondition the type could carry (`Option<Self>`). |
| C3-9 | confirmed | P3 | Rerun: 82 of the shim test's 144 round trips change the cohort (`1E+1 → 10`) while the test's comment says it guards against "a silent cohort shift" and compares with `partial_cmp`. Doc plus a `#[cfg(test)]` on the re-export. |
| C3-10 | confirmed | P3 | ladder.rs:582-585's `expect("Ziv doubling overflowed the precision counter")` cannot fire first: `DynArena::new` asserts at the ninth doubling (112,640). Control flow or doc. |
| C3-11 | confirmed | P2 (instrument) | Rerun in the reader's copy: a `ferrodec-transcend/tests/real_format_probe.rs` with a dev dependency on Decimal32 prints `atan2 = 0.00006619787` (A3-1's wrong value) and `sinh(224.01) = Infinity` (A5-1) from inside the crate, so the "cyclic dependency" rationale for `mock_format` (34 `unreachable!()` methods) holds for unit tests only; nine kernel modules have no inline test. Closure: a `tests/` directory holding the Wave 2 witnesses as regression rows. This is also the in tree closure for C1-11 and C1-13 that A1 built as an adapter. |
| C3-12 | confirmed | P3 | 230 plan milestone labels (`M8` 68, `M8b` 60, `M4` 52, …) in the crate's rustdoc, defined only in a plan file; ADR-0059 uses three without defining them. Closure: a glossary. |

Found clean, adopted: the 73 conditional allows in ladder.rs are honest
(each names its cluster; a full build still catches a dead budget); the
remaining `#[allow]` sites justified; a 48 cast sample with 77%
bounded by a visible invariant, 17% by a comment, 6% (three `i32`
exponent arithmetic sites) with no stated bound because the working
types' `exp: i32` field has no documented range; 27 of 40 panic
messages are proofs, with the exceptions catalogued (`parse_str` public,
`from_format` false, the Ziv `expect` unreachable). The reader's cast
regex found 189 production casts against counts.md's 241, which
includes test code (C4-13 names the same counting defect).

Design notes adopted into the report: kernel bodies as a typed pipeline
with a per function anchor table (five bodies over 80 lines mix all
five stages; `atan2` has no anchor arm where `atan2_pi` has two, which
is fd-gu75); `exact.rs` split along the eight seams it already marks,
with `strip_trailing_zeros` the only item the pi family uses (A1-1's
structural cause); totality where prose carries the precondition
(`from_format`, `trunc_to_i32`, `parse_str`, the top rung's `Option`);
a gate vocabulary beyond `exp`'s on the `DecimalFormat` seam; `run3`
and `ESCALATES` named for what they are.

## C4: clarity and pragmatism, the format crates and substrates

Sixteen findings; the reader ran every allowed clippy lint against
every crate and counted sites, measured function lengths, grouped the
`expect` messages, built and ran every example and bench, and
reconstructed seven fixes that landed in one copy from `git log -S`.
Every cited line says what the reader says; the coordinator reran the
`missing_panics_doc`, cast wrap, `float_cmp` and forced dead code
probes.

| id | verdict | ruling | note |
|---|---|---|---|
| C4-1 | confirmed | P2 (instrument) | Rerun: `-W clippy::missing_panics_doc` on `ferrodec-decimal` flags interop.rs:38, :45, :52 (B4-7, P0) and arith.rs:104 (`fma`, B4-6) among 34 library sites; Cargo.toml:75 allows the lint with no stated reason. Writing the `# Panics` section would have forced the false precondition into words. Closure: drop the allow; a `# Panics` section or totality per site. |
| C4-2 | confirmed | P2 | The five cast allows hide about 820 library hits (the coordinator's rerun of `cast_possible_wrap` over the dependency closure gave 477 against the reader's 472 total); enabling them flags every wrap site B4-6 names and B3-14's sign extending visitors, whose twelve local `cast_sign_loss` allows all sit on the defect lines. The Cargo.toml rationale holds for the root core (B1's 140 casts all bounded) and not for `ferrodec-decimal`. Closure: per crate warns, `try_from` or a local allow stating the bound; the per lint counts as a per bucket regression floor. |
| C4-3 | confirmed | P3 | Rerun: `float_cmp` has zero sites anywhere in the workspace; 21 local allows duplicate workspace allows. Tooling. |
| C4-4 | confirmed | P2 | d64 bid.rs:224-232 `Class::Finite { biased_exp: u32, coefficient: u64 }` while the crate has `BiasedExp` and `Coefficient` newtypes; 21 `expect("… from classify_bits")` per sibling re validate what `classify_bits` proved. Type. |
| C4-5 | confirmed | P2 | 33 `expect("finite")` (coordinator grep) plus 6 variants after per operation special dispatch in `ferrodec-decimal`; a `kind()` view matched once would make the `None` arm unreachable. Type. |
| C4-6 | confirmed | P3 | fma.rs:426-433's bullet says the in range effective subtraction "defers to legacy mul-then-add"; since 7183717 it calls `fma_ab_dom_in_range_eff_sub`, and `a`, `b`, `c` are read nowhere (four `let _ = (a, b, c…)` lines, grep 4); the :494-497 comment's "no observable disagreement" is B1-2. Doc and control flow. |
| C4-7 | confirmed | P2 | d64 quantum.rs:593-600 `propagate_nan2` gives sNaN priority and says it mirrors the root; transcend_impl.rs:131-137's `DecimalFormat::propagate_nan2` lets the first NaN win; two rules under one name in one crate, the structure behind B2-4. Control flow. |
| C4-8 | confirmed | P2 (record) | ADR-0030 declined the full drop fast path on the premise "worst case `excess ≈ BIAS` (≤ 398)" (text confirmed); ADR-0057 later fed the rounder a `2·10^9` sentinel without citing it; B3-1 is the consequence. ADR-0057's "byte-identical-parser convention" is also false (87 and 23 changed lines between the copies). Closure: addenda naming each other. |
| C4-9 | confirmed | P3 | d32 div.rs:203-206 says the zero quantum is "handled by `round_and_pack_finite`'s zero clamp"; the code packs directly with `Status::OK` (B2-6); the H9 fix 1b8c159 landed on Decimal64 only. Doc with B2-6. |
| C4-10 | confirmed | P3 | d64 parse.rs:15 "≤ 15 digits" and d32 "≤ 6 digits" against code accepting `2^50 − 1` and `2^20 − 1` (B3-5); fd-aqs.13's "single source of the bound" was true of the root only. Doc with B3-5. |
| C4-11 | confirmed | P3 | work.rs:12-15 names `normalize_to` as the only sticky fold site; work.rs:243 (`div`), :142-152 (`add`'s zero) and exp.rs:125 fold or write it too; B4-3's `2` came through the zero case. Type (private field) and doc. |
| C4-12 | confirmed | P3 | The sibling examples format with `{e:>10}` (grep) and the crates ignore width (B3-8), so the example demonstrates the defect; the root example says "≤ 1 ULP" where the README promises correct rounding; no sibling has a bench. Doc and tests. |
| C4-13 | confirmed | P3 (tooling) | counts.py:45 stops only at a column zero `#[cfg(test)]`, so `#[cfg(all(test, feature = …))]` modules count as production: the root's one `unwrap` is a test helper and the siblings' are 12 each, not 13 and 21. Fix the script and rerun counts.md at Phase 6. |
| C4-14 | confirmed | P2 (one way door) | context.rs:108-120 has public fields, so B4-5/6's validated constructor cannot hold without a breaking change; two rounding vocabularies (`Rounding`, `RoundingMode`) with no conversion. Type; a major version decision for `ferrodec-decimal`. |
| C4-15 | confirmed | P2 | d64 pow.rs:55-62 keeps a 2026-05-10 private `pow_special_cases` table "byte-identical to before" ahead of the generic one the kernel runs again at transcend pow.rs:231; hand written dispatchers: root 8, d64 30, d32 30. The structure behind A5-4, A5-5, C2-3. Control flow. |
| C4-16 | confirmed | P3 | `TYPE_MASK`, `EC_*` and `COEFFICIENT_FIELD_LIMIT` dead in all three `bid.rs` under "consumed by arithmetic modules"; d64 bid.rs:36-44's blanket allow promised its own removal; the multiword blanket allows silence nothing. Tooling with B1-1's rewrite. |

Found clean, adopted: every example and bench builds without warnings
and the examples demonstrate their claims except the three in C4-12;
`verus/` is a recorded dead end, not vestigial; the `src/multiword.rs`
shim is honest; the `nonminimal_bool` allow on the three `dpd.rs`
earns its place; the `ops` contract is stated once per crate; 37 of 48
sampled numeric comments true, the 11 false ones named (plus a class of
25 "§7.4" Clamped citations, B5-7). The allow list table (five lints
hide debt, four silence nothing, the rest earn their place), the large
function table (thirteen functions over 120 lines, five of them long
because the algorithm is), the mirror drift receipts (seven fixes that
landed in one copy, with dates and the defect each produced) and the
comment sample are adopted into the report; the drift receipts are the
evidence base for the ADR-0011 synthesis.

Design notes adopted: one decode and one `align_with_residue` for
addsub, fma and rem (B2-1 is the defect this shape produced: the
overlap path landed one day after the helper it did not use); one
lexer per crate behind `parse_str_inner` and `parse_literal`, or one
in `ferrodec-ieee` parameterised by format; `#[must_use]` on `Status`
itself; the twin naming mismatches that cost grep misses when fixes
are ported (`finalize`/`finalise`, `math/*` versus `ops/*`,
`PRECISION` twice per crate, `U128_DIGIT_CAP` eight times); one
rounding vocabulary in `ferrodec-decimal` before it is published.

## D1: architecture across the workspace

Nine findings; the reader built the feature matrix (70 single feature
and 32 pair builds on thumbv6m, clippy per selection on the host), the
MSRV check per crate, the public API table from rustdoc JSON, and three
probe crates. Every cited line says what the reader says; the allocator
probe and the seam probe were rerun by the coordinator, and the narrow
clippy failure spot checked.

| id | verdict | ruling | note |
|---|---|---|---|
| D1-1 | confirmed | P2 (a false claim on the published surface) | Rerun: a `#![no_std]` thumbv6m binary that enables `fmt,serde` and never serializes fails to link ("no global memory allocator found"); the `fmt` only control links. src/serde_impls.rs:29-46 has `extern crate alloc` and `to_string()` at module level (sibling copies the same); Cargo.toml:86 and the sibling manifests declare `no-std::no-alloc`, and the README's serde row says nothing. Closure: `Serializer::collect_str`, no `alloc`; a lane that links a binary without an allocator. Why missed: every thumbv6m lane builds an rlib, which never resolves the allocator symbol. |
| D1-2 | confirmed | P2 (the fd-z26 residue, mapped) | `cargo clippy --lib -D warnings` fails on `exp-log`, `hyperbolic`, `trig-pi` alone and on their `trig` pairs, for all four crates that carry the features, every error being `ferrodec-transcend` dead code (A1-1's 8 and 46; coordinator spot check 9 errors on the root at `exp-log`); every selection with `pow` passes; the README documents `exp-log` alone as a configuration (+84 KB). Closure: A1-1's cfg gates plus a narrow selection clippy lane. |
| D1-3 | confirmed | P2 (semver bearing) | d64 Cargo.toml:73-76 and the d32 twin make `exp-log`, `trig`, `trig-pi` imply `binary-float`, a leftover of the retired `libm` route the same manifest records as gone; the root implies `fmt` (Cargo.toml:107-117); the reader's scratch copy builds all four selections for thumbv6m with `fmt` in its place (the wrappers parse constants). Removing an implied feature is a breaking change; closure at the next major. |
| D1-4 | confirmed | P2 | Rerun of the seam probe: `DecimalFormat::nan_from(F::ONE)` panics on Decimal128 in debug (nan_propagate.rs:22), returns `NaN1` in release and the canonical `NaN` on both siblings; format.rs:93-95 states the precondition in prose. The `status` argument of `round_and_pack_finite` is documented as honoured (format.rs:118-122), passed as `Status::OK` at every kernel call site, and discarded by both sibling impls (`let _ = status` at transcend_impl.rs:185). The trait carries no `E_MAX`/`E_MIN`, so the gates are literals per impl (A5-1, A5-2's structure). Ruled P2, not P0: the panic is a debug assertion on a documented precondition no tree caller violates. Closure: remove the dead parameter; a NaN witness; a sealed trait (the crate is published and an external impl can break the `sqrt_seed` contract `rootn` relies on). |
| D1-5 | confirmed | P2 | `Extended` has public writable fields (extended.rs:66-71) that bypass the 50 digit invariant; 25 of its 28 public items and 11 of 69 public kernel functions are used by no format crate's production code (the campaign crate and root integration tests are the only consumers); two of four telemetry readers are read by nothing. Closure: private fields, `pub(crate)` or a `campaign` feature; joins A3-9 and A5-6. |
| D1-6 | confirmed | P3 | `rust-version = "1.84"` holds for every library with all features (the reader's `cargo +1.84 check --locked`), but no crate's tests, benches or examples build on 1.84 with the tracked lock file (`getrandom 0.4.2`, `clap_lex 1.1.0` need `edition2024`); ci.yml:60 builds only the root library. Tooling plus a sentence on what the MSRV covers. |
| D1-7 | confirmed | P3 | ADR-0011:78 names "a fifth or sixth crate" as the trigger for a layout ADR; the workspace has nine members and no follow up. Doc, with the three crate synthesis. |
| D1-8 | confirmed | P3 | `PRECISION` as `usize` in digits.rs:21 and as `u32` in bid.rs, two literals with no link, per crate. Constant. |
| D1-9 | confirmed | P3 | `telemetry` is a public feature on four crates whose counters live in `ferrodec-transcend` and read `0` where 64 bit atomics are missing; two readers unread. Type (`Option<u64>`) or cfg; pairs with D3-15 and A4-4. |

Found clean, adopted: every published crate builds for thumbv6m at
every single feature, every documented pair and every shipping feature
at once (102 cells), and for thumbv8m.main at default and one wide
selection (which no lane builds, C5); clippy with `-D warnings` on all
nine members at full selection; rustdoc `-D warnings` on the crates the
battery omits; the `no_std` posture per crate with `alloc` only where
documented (D1-1 the exception); MSRV 1.84 for every library; `Status`
and `RoundingMode` as closed types; public API name and shape parity
across the three formats (142 inherent items each, 132 identical after
width substitution, the 10 differences driven by storage width or
exactness); `ParseDecimalError` identical and `#[non_exhaustive]`;
dependency hygiene (workspace crates only at runtime plus optional
`serde` and `num-traits` with default features off; astro-float dev
only; both lock files tracked); the multiword tests run with
`--all-features` in every lane (the coordinator's green probe was the
probe's own selection, a local footgun rather than a lane gap). The
API consistency table (shape rows with the Wave 1 to 3 behaviour
findings as its notes), the invariant table (23 rows: invariant,
current mechanism, the type that would carry it), the selection
parity table (33 Decimal32 tests run in no lane: 17 dpd, 9 num-traits,
3 ops traits, 4 serde) and the three crate facts (sizes, mirror diff
totals, divergence classes, one crate fixes, constants per crate,
thumbv6m `.text` with LTO off, what a generic `Decimal<const P, const
EMAX>` would need on stable Rust) are adopted into the report. Blind
spots the reader named: sizes are object sizes with LTO off and no
linked application; compile times are single runs; `rug` under 1.84
untested.

Design notes adopted: derive the gate table from `E_MAX`/`E_MIN` on
the seam; seal the seam and narrow the crate (the next `DecimalFormat`
method is a breaking change for `ferrodec-transcend` today); public
dependencies are semver surface (`DecBig` in `ferrodec-decimal`'s API,
`ferrodec-transcend` in the formats'); namespace the test cfgs
(`ferrodec_force_escalate`), since a dependent's `--cfg ladder_audit`
would arm the shipped panics; return the Newton seed as `Extended` or
`Option<F>` so the fd-0o9 class cannot be typed.

## D4: the reference registry, licensing and provenance

Twelve findings; the reader audited all 53 entries (schema, 26 archive
captures opened, 12 hash manifests verified, 140 vendored decTest files
byte compared with the archived zip), traced the generated registry
text, and drafted the missing entries. All network use was read only
and no capture was saved. Every cited line says what the reader says;
the coordinator confirmed the packaging, the attribution count and the
duplicate pairs by command.

| id | verdict | ruling | note |
|---|---|---|---|
| D4-1 | confirmed | P3 (a class of 64 sites) | `git grep "§7\.4"` outside docs/archive: 64 lines in 46 files beside "clamp"; ADR-0048's title and body are the source ("§7.4 CLAMPED fidelity"), while the same tree uses §7.4 for overflow elsewhere, and B5-7's closure "cite ADR-0048" would point at the error. Clamped is the GDA condition (its page is the capture to cite). Closure: the ADR-0048 amendment, the 46 files, the registry gloss literal, and a checked in rule failing "§7.4" within a line of "clamp". |
| D4-2 | confirmed (reader's pdftotext of the archived N3220) | P3 | c23-draft.md:4 cites F.10.4.6 to F.10.4.9 for the pi functions; in the archived draft the inverse pi functions are F.10.1.8 to F.10.1.11 and cospi/sinpi/tanpi F.10.1.12 to F.10.1.14, while F.10.4.x holds power and absolute value. Doc, with the revision recorded. |
| D4-3 | confirmed | P3 | zeilberger-zudilin-2020 and salikhov-zudilin-pi-irrationality both carry `canonical: arxiv.org/abs/1912.06345` (one with `doi: none`, one with the DOI); the two Shidlovskii entries register one 1989 book (one "pending verification", one verified). Merge each pair; the guard rejects duplicate canonical sources. |
| D4-4 | confirmed | P3 | failure-min-positive-normal.md:44 and failure-d64-quantize-pow10.md:43 say "Open at write time" for defects fixed in June; their `verification` points at runners with no check; comparator masking is called fixed while the d64 runner keeps the lenient policy (C2-5); fd-clc (a shipped P1 panic) has no failure entry despite the ritual. Doc plus a guard rule. |
| D4-5 | confirmed | P3 (disclosure input) | cowlishaw-dectest.md's coverage gaps omit: no decTest file pins BID bits (dq/dd/dsEncode are DPD, the blind spot behind B1-1); upstream ships no Decimal32 arithmetic files; ten d64 files never dispatch; 99 Conversion_syntax rows skip; the d64 comparator is NaN lenient; four general files are not vendored; the entry counts four vendored directories where there are five. The README disclosure's named failure mode is fed from this list (D4-12). |
| D4-6 | confirmed | P2 (Parnell's decision on redistribution) | Coordinator rerun of `cargo package --list`: no LICENSE file in the ferrodec-ieee, -multiword, -transcend and -decimal64 packages (the reader found the same for -decimal32 and -decimal); the Decimal64 package carries 42 decTest files ("All rights reserved" licence text) and the root package the whole docs/archive tree (243 paths at the review tip). Closure: `license-file` or LICENSE symlinks plus `include` lists per crate, checked by a `cargo package --list` lane; the registry records the crates.io redistribution decision. |
| D4-7 | confirmed | P3 | verification-map.md has no row for any ADR-0059/0060/0061 instrument or this review; its fuzz row names crates that had not built since May; its "within 1 ulp" row names the one sided instrument; its declared consumers never link it. Doc plus a guard check that named artifacts exist. |
| D4-8 | confirmed | P3 (tooling) | Linkage is checked in one direction: files citing a source are mostly unlisted as consumers (astro-float 60 of 63, arb-flint 42 of 46 including the S2 certifier, payne-hanek 28 of 31, cowlishaw-dectest 63 of 72); seven external entries list the schema guard itself as their `verification`; five forward links are false. Closure: a machine readable `ref:` tag harvested in both directions. |
| D4-9 | confirmed | P3 (tooling) | references.rs:356-365 skips every `vendored-at-path` outside `docs/references/vendor/`; a three URL `archived` scalar and a non capture `archive-date` pass; the "generated" blocks derive only the variant set and bit positions from the types while every string is a test literal (the propagation path of D4-1). Closure: the guard checks named above; render names with `stringify!`. |
| D4-10 | confirmed | P3 | cowlishaw-gda-arith.md:7 archives the table of contents page only (2,263 bytes); the clauses the code cites live on subpages. The reader found the PDF capture (decarith v1.70, sha256 recorded) to cite instead; decnumber.md has the same shape. Accretion at Phase 6. |
| D4-11 | confirmed | P3 (accretion) | No entry for Cowlishaw's DPD summary that three `dpd.rs` files say they transcribe "verbatim", for the Intel BID library that three `bid.rs` files and three tests cite as "the Intel reference" (and that would have caught B1-1), or for the oracle versions behind the certified corpora (astro-float 0.9.5 / astro-float-num 0.3.6 with the `abs_cmp` defect, mpmath 1.4.1, python-flint 0.6.0, libmpdec 2.5.0); no `.prov` header records an oracle version. The reader's drafts (in its report) are the accretion input; `failure-pow-exponent-window` (fd-clc) and `failure-one-sided-faithful-oracle` (C1-1) are overdue failure entries. |
| D4-12 | confirmed | P3 (diff proposal only) | The disclosure's named failure mode list does not name the two classes this review's disciplines structurally miss (an interchange layout that round trips through its own codec yet disagrees with the standard; panics on finite inputs at a format's range edges), and the Decimal32 arithmetic gap. The reader's three proposals (an added sentence at README.md:65-73; the gap inventory kept in the registry; a true BID sentence for README.md:78 until fd-joi3 lands) go to Parnell unedited with D2's five. |

Found clean, adopted: all 140 vendored decTest files byte identical to
the archived 2.62 zip whose sha256 matches the pins; all 12 SHA256SUMS
manifests verify; all 26 recorded Wayback captures return 200 and show
the cited work; the four binary hashes match; the LSZ CC BY 4.0 licence
and attribution confirmed (the licence URI missing); `vendor/` empty
per ADR-0052 and the IEEE entries pointer only; the guard 7/7 green and
behaving as written (its blind spots are D4-8 and D4-9); Knuth, Lauter
and Lefèvre, Niven, and Payne and Hanek already have sound entries (the
brief's premise that they were missing was wrong; payne-hanek's notes
still say "6300 digit", D2-13). Provenance spot check: 16 of 20
functions cite the algorithm only; the DPD codec's self declared
verbatim transcription from an "All rights reserved" page, the Intel
mask identifiers in `bid.rs`, `DecBig::div_rem`'s identifier shape
against Hacker's Delight (recalled, not fetched), and `ln.rs`'s "§9.3"
clause are questions for Parnell to record, not defects. The reader's
RECALLED markers (the salikhov-2008 and matveev-2000 venues, the
Cowlishaw 2002 DOI, the Intel library's version and licence) stay
unverified in the drafts.

Design notes adopted: citations as data (`ref: <slug> §<clause>` tags
harvested by the guard); "generated" means derived; failure entries
keyed to beads with `bead:` and `fix:` fields; `.prov` headers stamp
the oracle version; an `include` list per crate so what ships is a
reviewed surface.

## Wave 4 summary

- No P0 or P1; 49 engineering and documentation findings (C3 12, C4 16,
  D1 9, D4 12), every one verified by line and every reproduced claim
  rerun (the stripped allow build, the in crate real format test, the
  cohort probe, the clippy counts, the allocator link, the seam probe,
  the packaging list). No finding refuted; two reader premises
  corrected by the readers themselves (the brief's missing registry
  entries exist; the multiword tests do run with `--all-features` in
  every lane).
- The wave's recurring result is the one the inputs sheet predicted: a
  fact stated in prose where a type could stand, found false by the
  first reader who measured it. D1's invariant table (23 rows) and
  C4's `expect` grouping (39 `finite` expects, 42 `from classify_bits`
  expects, the three interop expects that are B4-7) are the remediation
  program's type level backbone.
- The three crate question has its facts: 17,906 and 18,758 changed
  lines root against each sibling, 5,921 between the siblings; seven
  fixes that landed in one copy with the defect each produced; 142
  inherent items per format with 132 identical; a generic
  `Decimal<const P, const EMAX>` needs a format trait with associated
  storage types and array lengths, which stable Rust cannot derive
  from `P`. The synthesis is in the report's Verdict and the decision
  is Parnell's.
- The record: the CLAMPED misattribution is a class of 64 sites with
  its source in an ADR title; the failure museum says "Open" for June's
  fixed defects; six published crates ship no licence text and
  redistribute "All rights reserved" vectors; the GDA specification the
  code cites as its semantic authority is archived as a title page.

## Handed to Phase 6

- Report: the Verdict's architecture paragraph (D1), the design and
  documentation tier (C3, C4, D1, D4), the roster rows, the remediation
  items 5 and 6 extended (narrow clippy lane, no allocator link lane,
  MSRV lane, `cargo package --list` lane; the `binary-float`
  implication and the `Context` fields as major version items; the
  registry guard rules).
- Registry accretion: D4's entry drafts (cowlishaw-dpd-2002,
  intel-dfp-math-library, the two failure entries) and amendments
  (oracle versions, the GDA PDF capture, the arb-flint consumers), with
  Wayback saves at citation time.
- Disclosure: D2's five items plus D4's three as one diff proposal to
  Parnell; never applied by the coordinator.
- Tooling in the archive: counts.py's `cfg(all(test, …))` rule (C4-13)
  fixed and counts.md regenerated.

