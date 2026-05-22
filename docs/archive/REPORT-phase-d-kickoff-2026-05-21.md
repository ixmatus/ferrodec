> **Archived 2026-05-22.**
>
> Operational kickoff prompt for the Phase D scoping spike,
> executed and closed at signed merge `9fc0a08` on 2026-05-21 with
> ADR-0032 (correctly rounded §9.2 transcendentals via Lefèvre /
> Muller fixed precision bounds) the outcome. The "no code lands"
> shape stated in the prompt was relaxed mid engagement to "ADR
> plus per function rustdoc proof annotations plus strengthened
> corpus assertion" (12 commit slice, parent 2.0.0 → 2.1.0,
> sibling 2.0.0 → 2.1.0, three signed 2.1.0 tags). Retained as a
> template for the prompt style engagement kickoff; not maintained
> against current state.

---

# Phase D kickoff prompt (paste into a fresh /plan session)

This is the kickoff prompt for the next engagement on the ferrodec
2.0 roadmap. Phase B (the ADR-0029 three-item major release) closed
2026-05-21 at signed merge `77b71de` on `main` with three signed
tags (`ferrodec-v2.0.0`, `ferrodec-decimal64-v2.0.0`,
`ferrodec-decimal32-v2.0.0`). Phase A (`cargo publish` 2.0) stays
Parnell's hand and is sequencing-orthogonal. Phase D is next.

---

Load memory.

We continue the ferrodec industrial-usability roadmap. Phase B
(ADR-0029 three-item 2.0 release) closed and tagged at `77b71de`
on `main`; the live state is in
`[[ferrodec-current-state-2026-05-21]]`. Phase A (`cargo publish`)
remains deliberately on my hand, not yours.

This engagement is **Phase D — correctly-rounded transcendentals
scoping spike**. Read `[[ferrodec-current-state-2026-05-21]]` first
for state. Then read `[[phase-b-2-0-closed]]` for the Phase B
execution lessons that carry forward (the design-fork discipline,
the pre-flight gate routine, the Kani entry-point name-stability
pattern). Then read `[[phase-b-d-kickoff]]` (the Phase D section
specifically) for the original triage-roadmap framing. Then read
`[[phase-d-engagement-plan]]` for the execution-level detail: open
design forks, source files to explore, primary-source citations the
ADR must include, bead workflow, slice shape, and the ADR template.
The four memories together are the orientation; the work is one
`/plan` pass and one ADR.

# What Phase D is

A scoping spike. **No code lands.** The deliverable is a `/plan`
output plus an ADR (the next free number in `docs/decisions/`,
working-assumption ADR-0032) that supersedes or extends ADR-0024
on the faithful (≤ 1 ULP) §9.2 transcendental contract.

The ADR commits to a strategy for tightening to *correctly-rounded*
(the single nearest representable value, ties to even at
`RoundingMode::NearestEven`) across the §9.2 surface on all three
formats. Per-function rollout and sibling-first sequencing are
open design questions to settle in the ADR; surface them to me
before locking. The §9.2 surface: `exp`, `ln`, `exp2`, `log2`,
`log10`, `cbrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`.

# The three candidate strategies (pre-triaged)

The triage roadmap pre-evaluated three strategies; the ADR's
Decision section picks one and the Rejected alternatives section
explicitly defends rejecting the other two. The expected accept is
named below, but I want the ADR to derive the choice from the
embedded-posture constraints rather than recall my expectation.

  - **Ziv adaptive precision with arbitrary-precision fallback.**
    Compute at `p + k` extra digits, doubling `k` on residue
    ambiguity. Expected REJECT on unbounded worst-case latency
    (incompatible with the STM32U-targeted embedded posture).

  - **CRlibm-style precomputed worst-case-rounding tables.** Per-
    function tables of the worst-case argument-residue pairs. Per-
    function code-size and table-generation cost is poor for
    embedded; the Cargo feature matrix would be unmanageable.
    Expected REJECT.

  - **Lefèvre / Muller wider fixed working precision with rigorous
    a-priori error bounds.** Pick a working width `p + k` provably
    sufficient for correct rounding on every input; the per-
    function correctness proof is the cost, discharged once.
    Expected ACCEPT. Fits the existing `ferrodec-transcend`
    Extended kernel posture (U256 at parent precision); widening
    `k` is incremental.

# Open design forks (surface via AskUserQuestion before writing the plan)

These five forks materially shape the ADR. Lock them in with
`AskUserQuestion` before writing the plan file:

  1. **Per-function rollout vs all-at-once.** ADR-0024 is family-
     wide faithful; the new ADR could extend it per-function
     (each transcendental moves independently, with the ADR
     amended each time) or replace it wholesale (a single
     transition). Per-function matches the proof-effort
     granularity in (c); all-at-once is a cleaner user-facing
     contract but a larger single-PR proof burden.

  2. **Sibling-first sequencing.** Decimal32 has 7-digit precision;
     the working width to guarantee correct rounding is much
     smaller than Decimal128's 34 digits. Decimal32 might be the
     natural pilot. The Arb corpus already validates all three
     formats; the validator does not constrain the sequencing.

  3. **Versioning impact.** Faithful → correctly-rounded is a
     *compatible* tightening (every correctly-rounded value is also
     a faithful value). The change could be SemVer-minor in
     principle. The worst-case-latency shift (wider working
     precision is slower per call) is the question. Probably no
     major; surface it.

  4. **`pow` ordering caveat.** `pow(x, y) = exp(y · ln x)` has the
     largest cumulative error envelope; its working width `k` is
     bounded by the worst of `exp` and `ln`. If per-function rollout
     is chosen, `pow` is NOT the first target. The ADR must record
     this dependency.

  5. **`tan` near-asymptote bound.** `tan(x)` at odd multiples of
     π/2 returns ±∞ today (no `DIV_BY_ZERO`; transcendental
     asymptote, not literal IEEE division). The Lefèvre bound for
     `tan` past the Payne-Hanek argument reduction at large
     magnitudes is a per-function concern. Surface this in the
     per-function proof table the ADR commits to.

# What you should do first

1. **Run the full pre-merge gate on `main` at 77b71de before
   anything else.** Phase B surfaced two pre-existing Phase C
   residue defects this way; Phase D inherits the same discipline.
   The gate:

   ```
   cargo fmt --check --all
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features
   cargo test --workspace --all-features
   ```

   Any residual failure becomes a pre-flight fix commit before the
   ADR work starts. No bundling.

2. **Launch one Explore agent on `ferrodec-transcend`.** The
   question to answer: what is the current working precision of
   the Extended kernel (the per-function `coef`, `exponent`, and
   `sign` shape, the U256 width), and what would widening it cost
   in code size and latency per function? Skim the per-function
   modules (`exp.rs`, `ln.rs`, `sin.rs`, `cos.rs`, `pow.rs`, etc.)
   and the parent-vs-sibling shim sites
   (`src/transcend_impl.rs` on the parent,
   `ferrodec-decimal{64,32}/src/transcend_impl.rs` on the
   siblings). Report file paths, the working width, the per-
   function kernel boundary, and any obvious per-function
   complexity differences.

3. **Read ADR-0024 and ADR-0026.** The ADR Phase D writes amends
   ADR-0024 and leans on ADR-0026's oracle stack. Confirm the
   contract Phase D is changing matches the kickoff memory's
   summary; flag any divergence.

4. **Skim the primary sources before drafting.** The ADR must cite
   primary sources, not training recall. The non-negotiable
   citations:

      - Lefèvre, V. 2000, "Moyens arithmétiques pour un calcul
        fiable" — the worst-case-rounding-bound results.
      - Muller, J.-M. "Elementary Functions: Algorithms and
        Implementation" (3rd edition, Birkhäuser 2016) — the
        textbook covering the wider-fixed-precision technique.
      - IEEE 754-2019 §9.2 — the optional correctly-rounded
        transcendental clause this ADR commits to.

   Cite the works; do not paste algorithm pseudocode from them.
   The working-width derivation per function is yours to write
   from first principles plus the cited bound.

5. **Surface the five design forks via AskUserQuestion** before
   writing the plan file. The questions are listed above; phrase
   them concretely with the expected-accept option named first per
   the CLAUDE.md AskUserQuestion convention.

6. **Write the plan file.** Per CLAUDE.md plan mode, write to the
   plan file the harness assigns. Include the strategy choice with
   its defended rationale, the per-function rollout plan (if
   chosen), the proof obligations the implementation slices will
   discharge, the versioning impact decision, and the bead plan.

7. **`/plan` workflow ends with `ExitPlanMode`.** After the plan
   file is final and the design forks are settled, exit plan mode
   so we can execute.

# Slice shape (docs-only)

Phase D is a `/plan` + ADR slice. Concrete shape:

  1. One Explore agent on the `ferrodec-transcend` kernel (above).
  2. One Plan agent for the ADR design (use the existing Plan
     agent pattern from CLAUDE.md plan mode).
  3. Surface the five forks via `AskUserQuestion`.
  4. Write the plan file.
  5. Exit plan mode.
  6. Branch `adr-0032-correctly-rounded-transcendentals` (or
     whatever ADR number is free; check
     `docs/decisions/README.md`).
  7. Write the ADR. One commit: `docs: ADR-0032 commit to
     <strategy> for correctly-rounded §9.2 transcendentals (fd-1pv)`.
  8. Update `docs/decisions/README.md` forward-pointer in the same
     commit or a separate one per one-concern-per-commit.
  9. Per-commit gate is minimal (docs only): fmt over any source
     touched (none expected), rustdoc -D warnings (because the ADR
     may add a forward-pointer from `lib.rs` rustdoc to the new
     ADR).
 10. Signed `--no-ff` merge into main. **No version bump. No
     signed tag.** The spike is docs-only.
 11. Bead state: claim `fd-1pv` on start, leave open with the ADR-
     landed status note plus per-function children captured
     separately (or close it, depending on the rollout decision).
     File the new ADR-0032 bead on commit-1, close on merge.

# Working disciplines (unchanged)

- Beads is the live tracker. `bd ready` drives execution.
- One concern per commit. Phase D resolves trivially: one for the
  ADR, optionally one for the index forward-pointer.
- Unsigned commits on the branch (`--no-gpg-sign`); signed merge
  into main; no tag.
- Prompt before every YubiKey boundary; the gpg-agent IS the SSH
  agent; verify HEAD moved + ls-remote after push; the Phase C
  lesson (a Bad-PIN first sign attempt asks before retry) holds.
- Per-bucket conformance pin per ADR-0010: no movement expected on
  a docs-only slice, but verify on the first commit.
- Garner / Gopen prose style in the ADR. No hyphens or em dashes
  in prose; compound-adjective hyphens fine inside backticks.
- Disclosure invariants preserved (Phase D does not touch the
  README disclosure).
- Claude never runs `cargo publish`. Phase A stays my hand.

# Current git state

main = 77b71de (Good-sig by Parnell, origin in sync,
ls-remote-verified) after the Phase B signed merge.

Versions: ferrodec 2.0.0, ferrodec-decimal64 2.0.0,
ferrodec-decimal32 2.0.0. Signed tags ferrodec-v2.0.0 /
ferrodec-decimal64-v2.0.0 / ferrodec-decimal32-v2.0.0 all on
77b71de, pushed, ls-remote-verified.

Shared-infrastructure crates unchanged: ferrodec-ieee 0.1.4,
ferrodec-multiword 0.1.0, ferrodec-transcend 0.1.0,
ferrodec-test-support unpinned.

Working tree clean except untracked `docs/REPORT*.md` (the noted
out-of-history report artifacts; this file is one of them).

`bd ready` shows only `fd-4gq` (a P3 pre-existing rustdoc
default-features dpd defect, out of scope for Phase D). `fd-1pv`
is the Phase D umbrella bead (P4, deferred 2027-06-01); claim it
on slice start.
