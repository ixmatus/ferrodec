# Plan: dedicated ferrodec-decimal64 correctness slice

> **Status**: planned 2026-05-11 (kickoff).
>
> Carved out of the 1.15 cycle by ADR-0017 after Slice D's first hour
> disproved the assumption that decimal64 was conformance clean enough
> for a same slice dispatcher buildout. Three H tier correctness bugs
> in `ferrodec-decimal64` mirror Decimal128's pre 1.13 H tier shapes
> and were never propagated. This slice closes them, then wires the
> conformance dispatch arms so the suite becomes the regression guard
> for the fixes.

## Context

`ferrodec-decimal64` 1.2.0 shipped 2026-05-10 with the parse magnitude
back port (CHANGELOG 1.14.5 for the parallel decimal128 case) but no
deeper correctness review. Slice D of the 1.15 cycle wired a probe
dispatcher for `add` / `subtract` / `multiply` / `divide` / `fma`
against the vendored `dd*.decTest` corpus and produced:

- 58 failures in `ddAdd.decTest` (roughly 5.3 % failure rate).
- 2 failures in `ddDivide.decTest`.
- 1 failure in `ddMultiply.decTest`.
- A `debug_assert` panic in `ferrodec-decimal64/src/bid.rs:216`
  (`biased_exp <= BIASED_EXP_MAX`) on some `ddFMA.decTest` case,
  before the dispatcher reached its compare step.

ADR-0017 carved this work out, deferred the dispatcher buildout, and
landed `ferrodec-decimal64/KNOWN_ISSUES.md` listing the three open
bugs with case ID reproducers (where pinned). ferrodec 1.15.0,
ferrodec-decimal32 1.3.0, and ferrodec-decimal64 1.3.0 shipped
2026-05-11 without these fixes; the published artifact is no worse
than 1.2.0 on the affected cases but no better either.

The methodology is the 2026-05-09 six agent correctness review (the
one that produced Decimal128's 1.13.x H tier fix train), applied to
Decimal64. ADR-0010 captures the methodology; ADR-0017 specifies that
decimal64 should be run through the same shape rather than triaged
case by case.

## Phase 0 — narrow H3 (FMA debug_assert panic)

A `debug_assert` panic blocks any clean dispatcher pass rate
measurement (CI dies before the compare step), and the agents in
Phase 1 need a pinned reproducer to start from rather than "panics
somewhere in roughly 1378 cases."

**Steps.**

1. Read `ferrodec-decimal64/tests/conformance.rs::dispatch_op` and
   `ferrodec-decimal64/src/bid.rs:216` (the assertion site).
2. Temporarily extend `dispatch_op` with an `fma` arm. Keep the
   change in the working tree only; do not commit the dispatch
   extension itself.
3. Run `cargo test --release --package ferrodec-decimal64 --test
   conformance` to confirm the corpus reaches clean failures rather
   than panicking (release skips the `debug_assert`).
4. Run debug to reproduce the panic. Capture which `ddFMA` case ID
   is in flight when the assertion fires (the runner logs the case
   ID before invoking the op).
5. If the panic site does not log the case ID, bisect by case index:
   subset the corpus to a half, narrow until the offending case is
   pinned.
6. Revert the dispatch extension (working tree only).
7. Update `ferrodec-decimal64/KNOWN_ISSUES.md`'s H3 entry with the
   pinned case ID and (if visible) the operand values.
8. Commit the KNOWN_ISSUES update:
   `test(decimal64, conformance): pin ddFMA case triggering pack_finite biased_exp precondition panic`.

No code fix in Phase 0. The fix lives in the slice that owns
Decimal64's FMA biased exp arithmetic.

## Phase 1 — six agent decimal64 correctness review

Mirror the 2026-05-09 decimal128 review. Six concurrent Explore tier
agents over the decimal64 op surface; each gets the three seed bugs
(H1, H2, H3 with the Phase 0 pin), the analogous Decimal128 H tier
findings as reference, the Decimal128 fix commits as oracle for the
spec correct shapes, and a directive to find more.

**Agent allocation (provisional; rebalance during kickoff).**

1. `addsub` — `ferrodec-decimal64/src/ops/addsub.rs`,
   `ferrodec-decimal64/src/ops/round.rs`. Owns H1 and H2.
2. `mul`, `div`, `rem` — `ferrodec-decimal64/src/ops/{mul,div,rem}.rs`.
3. `fma` — `ferrodec-decimal64/src/ops/fma.rs`. Owns H3.
4. `sqrt`, `quantum` — `ferrodec-decimal64/src/ops/{sqrt,quantum.rs}`.
5. `parse_str`, `Display`, conversions —
   `ferrodec-decimal64/src/{decimal,bid}.rs`,
   `ferrodec-decimal64/src/convert/`.
6. `transcendentals` and Kani harnesses —
   `ferrodec-decimal64/src/ops/{exp,pow,trig,hyper}.rs`,
   `ferrodec-decimal64/src/verify/`.

**Output.**

- Findings doc committed at
  `docs/decisions/plans/2026-05-11-decimal64-correctness-findings.md`.
- One entry per finding, classified H / M / L by tier (severity rubric
  same as ADR-0010).
- Each entry includes: file:line citation, a one line reproducer (a
  decTest case ID where available, otherwise a property test seed or
  unit test input), and a one paragraph hypothesis on the cause.
- Findings are *not* fixed in Phase 1. The slice's deliverable is the
  prioritized list.

### Provenance discipline for Phase 1 agents

H1, H2, and H3 fix neighborhoods sit on top of decades of canonical
reference work. The two implementations most likely to surface in
training memory when fixing BID alignment, FMA biased exp arithmetic,
or round half even at the 16 digit boundary are Intel's Decimal
Floating Point Math Library (`BSD-3-Clause`) and IBM's decNumber (ICU
license). Both are permissive, but lifting code shape silently
(variable names, helper decomposition, line ordering) breaches license
preservation even for permissive sources. The new global CLAUDE.md
provenance rule forbids this without explicit citation and license
compatibility check.

The agent rules:

1. Oracles are **IEEE 754-2019 spec text** plus **Decimal128's
   behavior** on the affected cases (output equivalence, not code
   shape). Do not consult or recall Intel's library or decNumber while
   drafting fixes.
2. If a draft starts to feel like recall rather than derivation, stop.
   Name the source the draft seems to be tracking and either cite it
   openly with a license compatibility check, or re derive from spec.
3. Each fix commit body documents the derivation path: spec section
   citation, the operand neighborhood cross checked, and any novel
   derivation steps. "Drafted from spec, cross checked against
   Decimal128's behavior" is the honest framing.
4. Decimal128's own fix commits are an oracle for *behavior* on the
   H tier cases, not a template for code shape in decimal64. Re derive
   identifiers, helper decomposition, and file ordering from idiomatic
   decimal64 patterns.

### Security audit in Phase 1

The new global CLAUDE.md security posture names integer overflow in
length and index arithmetic as a vulnerability family to notice at
write time. Several decimal64 ops compute biased exponents, digit
counts, and shift amounts from operand values; H3 is one instance of
the class, but it is unlikely to be alone. Agents include a security
audit pass in their sweep.

- **Every agent.** Audit `debug_assert!` sites that gate on input
  derived arithmetic. `debug_assert!` is a no op in release builds; in
  production the failed invariant produces garbage bits rather than
  panicking. Any such site needs either the invariant lifted into the
  type system (preferred) or the check converted to a release safe
  form (saturating clamp + status flag, or hard `assert!`).
- **Agent 5 (parse_str, Display, conversions).** Parser takes attacker
  supplied bytes. State the threat model up front in the findings:
  who can supply the input, what the worst outcome could be (panic
  for DoS, silent incorrect parse for downstream contamination), and
  which entry point answers the question. The doc comment on the
  entry point carries the answer.
- **Agent 6 (transcendentals).** Iteration bounds in `exp`, `ln`,
  `pow`, `sin`, `cos` should be independent of attacker controlled
  operand magnitude. Audit for any bound that grows with the input;
  flag for the prioritized list if found.

These directives complement the correctness review; they do not
replace it. Security family findings land in the findings doc with
the same H / M / L tier rubric.

## Phase 2..N — fix per tier (H, then M, then L)

One finding per commit. The cadence is the 1.13.x to 1.14.3 Decimal128
train, applied to Decimal64.

**Per commit.**

- Implement the fix in the smallest possible diff.
- Add or extend a property test (astro-float oracle, per
  `feedback_oracle_choice`) that asserts the spec answer over a seed
  set covering the reproducer plus the neighborhood.
- For any H or M tier fix with a tractable Kani harness shape
  (bounded operand space, no transitive `decimal_digit_count` blow up
  through the call graph), add a verify harness via the
  `*_special_only_for_kani` shim pattern per ADR-0016. Skip the
  harness for fixes whose call graph triggers the CBMC loop unrolling
  budget; record the reason in the commit body.
- Update the per file expected pass count table in
  `ferrodec-decimal64/tests/conformance.rs` to the new exact match
  for the affected `dd*.decTest` file. No global PASS_FLOOR.
- `cargo fmt --all` and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean before staging.
- Commit body follows the structured shape (intro paragraph, one
  section per area touched, final verification block).
- Co-Authored-By trailer.

Tier order: H first (the three known plus whatever Phase 1 adds),
then M, then L. Each tier ends with a per tier verification pass over
the full conformance corpus.

### H3 fix shape (constraint)

The H3 bug is `debug_assert!(biased_exp <= BIASED_EXP_MAX)` at
`pack_finite`'s caller maintained precondition. In release builds the
check is a no op; `pack_finite` continues with the out of range value
and packs garbage bits. Under the global security posture this is not
acceptable as a final state, and the type design rule "make illegal
states unrepresentable" applies directly.

The fix lifts the invariant out of debug runtime. The H3 commit
chooses one of:

1. **Preferred**: introduce a typed `BiasedExp` newtype in
   `ferrodec-decimal64/src/bid.rs` (or a sibling module). Its
   constructor proves `0 <= value <= BIASED_EXP_MAX` and returns
   `Option<BiasedExp>` or `Result<BiasedExp, Overflow>`. `pack_finite`
   takes `BiasedExp` directly; the precondition disappears from
   `debug_assert!` and becomes a compile time guarantee. The FMA
   path's biased exp computation either returns the typed value or
   propagates the overflow up the call stack, surfacing via
   `Status::Overflow` to the caller.
2. **Fallback**: if (1) does not fit the existing call sites cleanly,
   convert the `debug_assert!` to a release safe check. Saturating
   clamp with `Status::Overflow` raised, or hard `assert!`. No
   `debug_assert!` on input derived arithmetic survives the slice.

The choice between (1) and (2) gets made at fix time after reading
the FMA call graph. The commit body documents the choice and the
reason.

## Phase N+1 — wire conformance dispatch arms

With Decimal64's ops now spec correct, wire the dispatcher's
`add` / `subtract` / `multiply` / `divide` / `fma` arms in
`ferrodec-decimal64/tests/conformance.rs`. Set `expected_per_file` to
the actual measured pass counts for each newly wired `dd*.decTest`
file. The 99 skip taxonomy (rounding directives outside the IEEE set)
carries over from Decimal128 unchanged.

Commit cadence: one phase per logical group of arms, not all five at
once. Likely shape:

- Phase N+1a: `add` and `subtract` (the addsub arm).
- Phase N+1b: `multiply` and `divide`.
- Phase N+1c: `fma`.

Each phase verifies the wired arm's `expected_per_file` matches the
measured count exactly; mismatches surface immediately rather than
silently hiding behind a one sided floor.

## Phase N+2 — release ferrodec-decimal64 1.4.0

H1 and H2 change observable answers on previously wrong inputs (the
returned value of `Decimal64::add` on the magnitude loss case shifts
from `0E+50` to `1.0000E+5`, and the half even rounding direction
flips on the 16 digit boundary cases). Both are bug fixes against the
spec, but downstream consumers may have implicitly depended on the
old behavior. Minor bump (1.3.0 to 1.4.0) honors that.

**Steps.**

- Bump `ferrodec-decimal64/Cargo.toml` to `1.4.0`.
- `ferrodec-decimal64/CHANGELOG.md` entry detailing each fix with
  spec citation (IEEE 754-2019 § references), reproducer case ID, and
  before and after value where the diff is illustrative.
- Cross link from workspace `CHANGELOG.md`.
- ADR-0018 closes ADR-0017 (change ADR-0017's status to
  `superseded by 0018`). Title: "Decimal64 H tier correctness train
  closing." Body: lists every fix, the conformance pass count delta,
  and the methodology lesson (six agent review applies cleanly to
  sibling crates with shared idioms).
- Verify clean across the full workspace matrix:
  `cargo build`,
  `cargo test --workspace --all-features`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo build --target thumbv6m-none-eabi -p ferrodec-decimal64 --no-default-features`,
  `cargo kani --workspace`.
- Signed merge into main. YubiKey cache prompt before
  `git merge -S decimal64-correctness`.
- Signed tag `ferrodec-decimal64-v1.4.0`. YubiKey cache prompt before
  `git tag -s`.
- `cargo publish --package ferrodec-decimal64`.

## Critical files

To read before Phase 0:

- `/Users/parnell/Development/ferrodec/ferrodec-decimal64/tests/conformance.rs`
  (dispatcher; H3 bisect site).
- `/Users/parnell/Development/ferrodec/ferrodec-decimal64/src/bid.rs:216`
  (the asserting `pack_finite`).
- `/Users/parnell/Development/ferrodec/ferrodec-decimal64/src/ops/fma.rs`
  (the path producing the out of range `biased_exp`).
- `/Users/parnell/Development/ferrodec/ferrodec-decimal64/KNOWN_ISSUES.md`
  (current entry; gets updated at Phase 0's commit).

To read before Phase 1:

- `/Users/parnell/Development/ferrodec/docs/decisions/0010-testing-strategy-after-six-agent-review.md`
  (methodology; same shape gets applied here).
- `/Users/parnell/Development/ferrodec/docs/decisions/0017-decimal64-conformance-coverage-gap.md`
  (the carve out ADR that named the three seed bugs).
- `/Users/parnell/Development/ferrodec/CHANGELOG.md` 1.13.0 through
  1.13.1 entries (the Decimal128 H tier fix train; an oracle for the
  spec shapes Decimal64 is missing).

## Verification per phase

- **Phase 0**: H3 case ID pinned in KNOWN_ISSUES.md;
  `cargo test --workspace --all-features` still green on the rest of
  the corpus.
- **Phase 1**: findings doc lands; no source changes.
- **Phase 2..N (per commit)**: full workspace test green; clippy
  clean at `-D warnings`; conformance per file table updated for the
  fixed file; the new property test fails on `main` and passes on the
  fix commit; Kani harness added where the call graph permits.
- **Phase N+1**: dispatcher arms wired; `expected_per_file` exact
  match for every newly wired arm; aggregate pass count reported in
  the commit body.
- **Phase N+2**: full workspace matrix green; Kani 70 of 70 (or larger
  if Phase 2..N adds harnesses); `cargo publish --dry-run` clean for
  `ferrodec-decimal64`.

## Discipline (carried forward)

- Phase = commit. Structured body. Co-Authored-By trailer.
- `cargo fmt` and `cargo clippy -D warnings` clean before every
  commit.
- One concern per commit; never mix behavior change and refactor.
- Exact match per file conformance counts; never a one sided floor.
- astro-float oracle for property tests; not MPFR.
- Prompt before signed merge or signed tag (YubiKey).
- Strict revert stop loss on perf shaped patches (none expected this
  slice).
- Garner and Gopen prose style: no hyphens or em dashes in prose;
  identifiers in backticks and CLI flags may keep them. Applies to
  the plan doc, CHANGELOG, ADR-0018, KNOWN_ISSUES, and every phase
  commit body.

## Out of scope for this slice

- decimal32 KNOWN_ISSUES (separate follow up slice).
- Routing decimal32 and decimal64 transcendentals through Decimal128's
  `Extended` kernel (1.16 era; recorded in ferrodec 1.15.0 CHANGELOG
  `Deferred` section).
- Decimal128's missing `fma` Kani harness (separate follow up).
- Decimal64 transcendentals beyond what the review surfaces (the
  review is correctness only; the transcendental rewrite tracked
  separately).
- Decimal64 DPD interchange (the `dd*Encode` and `dd*Canonical`
  conformance buildout follows the BID dispatch buildout; out of
  scope here).
