# Plan: testing-surface extension (metamorphic, cross-precision, differential)

Snapshot of the approved plan at decision time (2026-05-17). Living
rationale moved to ADR-0025 (metamorphic) and the per-track ADRs as
the later tracks land.

## Context

Revisiting prior testing-strategy feedback against the v1.16.0 tree
(shared `ferrodec-transcend` kernel, recorded oracle sound-magnitude
domain). Three tracks, oracle unchanged (astro-float; `rug`/MPFR
rejected as a binary, C-FFI oracle that does not close the
decimal-spec gap and cannot serve as the arithmetic oracle; the
authoritative independent decimal reference is the Python/libmpdec
differential instead). Mutation testing out of scope.

## Track 1 (this slice): metamorphic identities

Algebraic identities as the correctness backstop in the oracle's
skip regions. Shared `transcend_oracle::within_n_ulp_band` (O(1)
structural band); per-crate `tests/property_metamorphic.rs`.

Two design corrections made during implementation, both promoted to
ADR-0025:

1. **Tautology audit.** A kernel-call-graph audit removed identities
   whose two sides route through the same shared-kernel helper
   (`log_b·ln(b)≈ln`, `tanh≈sinh/cosh`, `exp2==pow(2,x)`,
   `asinh`/`atanh` vs their own ln-forms). The shipped set is the
   non-degenerate remainder, in three categories (A independent
   cross-computation; B independent inverse round-trip; C
   cancellation, weak).
2. **Condition-number bounds.** A flat `N+2` ULP budget is unsound for
   ill-conditioned compositions (`exp(ln 1e300)` ≈ 700 ULP). Each band
   is the analytic condition number expressed in ULP-of-x units (the
   magnitude-ratio term a naive `1+|cot x|` misses; the
   `acos(cos 0.05)` underestimate).

## Track 2: cross-precision arithmetic D64 to D128

`tests/d128_crosscheck.rs` mirroring the existing D32→D64 file, plus
`fma`. Arithmetic only (independent per-crate impls);
transcendentals excluded (shared kernel ⇒ self-referential).

## Track 3: Python/libmpdec differential (local-only, opt-in)

`differential` Cargo feature; `tools/diff_oracle.py` + shared
harness. Exact-equality for arithmetic, faithfulness-set check for
the `exp/ln/log10/sqrt/pow` subset Python `decimal` supports.
Nightly CI to run it is a deferred follow-up.

## Execution order

Track 1, then 2, then 3 (priority and risk; mutually independent).
One bead and one signed-at-merge commit per track.
