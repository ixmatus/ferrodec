# ADR-0004: Skip Verus pilot graduation

- **Status**: accepted
- **Date**: 2026-05-06

## Context

A 2026-05-06 pilot introduced [Verus](https://verus-lang.github.io/verus/) — a Rust-native SMT-backed verifier — as a sibling proof crate at `verus/`. The proof-level work succeeded: `Status` flag pairwise disjointness and `pow10` correctness against an inductive `power_of_ten` spec verified, 26 ensures clauses across two phases.

The pilot's stated end goal was *graduation* — folding the Verus annotations into `src/` so the verified code and production code are the same code. The pilot's sibling-crate shape was an explicit risk-mitigation tactic: if graduation worked, eliminate the duplication; if it didn't, abort cleanly.

Two upstream walls blocked graduation:

1. `cargo verus verify` panics inside `rust_verify/src/erase.rs:405` while compiling the published `vstd-0.0.0-2026-04-20-1748` against the prebuilt `verus-arm64-macos` binary at `0.2026.05.03.8b81855`. Reproduces on the canonical `cargo verus new --lib` hello-world; not ferrodec-specific.
2. Direct `verus --extern ferrodec=...rlib` rejects external-crate constants without `assume_specification` axioms. The axioms are the duplicated values re-expressed as trusted statements — same information, different syntax. That's not graduation; it's the duplication wearing a different hat.

`pow10` specifically can't graduate without a Verus stdlib lemma about `u128::pow` (or a parallel recursive mirror, which is what the pilot wrote — verified, but proves a different function from production's `10u128.pow(k)`).

## Decision

Abort the pilot. Roll back all main-crate changes (the `verify-internals` Cargo feature, doc cross-references). Replace `verus/src/lib.rs` and `verus/Cargo.toml` with a single `verus/EXPERIMENT.md` documenting what was tried, what verified, and what specifically blocked graduation. Do not ship 1.8.0 with a Verus pilot in it.

## Consequences

**Wins:**

- The published 1.7.1 stays clean — no half-shipped `verify-internals` feature surface, no documented-but-unused proof crate.
- `verus/EXPERIMENT.md` records the toolchain pins (Verus binary version, Rust 1.95 nightly, `vstd` version) and the specific failure modes. The next contributor or the next Verus release window picks up from a written dead-end map instead of redrawing it.
- The existing five-stack verification (unit tests, property tests, Kani, conformance vectors, fuzz) does the heavy lifting unchanged. No regression in correctness assurance.

**Costs:**

- Two days of pilot work (Phase 0 install, Phase 1 Status disjointness, Phase 2 `pow10`) shipped only as documentation.
- Downstream observers reading "ferrodec considered Verus" might not realize the toolchain blocker is upstream-fixable. The CHANGELOG's 1.8.0 entry was retracted; the pilot doesn't appear in any released ferrodec version.

**Why this isn't reconsidered without an external trigger:**

A legitimate retry needs at least one of:

- A `vstd` release that doesn't panic under `cargo verus verify` against the available prebuilt binary.
- A Verus stdlib lemma library for `u32::pow` / `u128::pow` so the proof can apply to ferrodec's actual one-liner instead of a parallel recursive body.
- An `assume_specification`-only graduation path the user explicitly accepts, with the understanding that the trusted axioms ARE specifications and the values are still effectively duplicated.

Without one of those, the same conclusion will recur. The audit-log convention for this kind of "tried, didn't work, here's why" is to commit `EXPERIMENT.md` next to the orphaned code and add an ADR; we did.

## Related

- `verus/EXPERIMENT.md` — the on-disk record of what the pilot tried and where it stopped.
- `feedback_verus_pilot_aborted` memory entry (per-user) — short-form note for future Claude sessions to avoid re-suggesting the same path.
- Plan: original Verus pilot plan was overwritten by this perf plan after abort.
