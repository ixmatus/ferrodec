# Plan: independent correctness evidence for ferrodec-transcend derived transcendentals

- **Date**: 2026-05-17
- **Tracking**: bead `fd-cb6` (epic); phases `fd-syf` (0), `fd-clf` (1), `fd-x3u` (2), `fd-i4e` (3), `fd-12v` (4)
- **ADR**: 0026
- **Follow-on to**: `plans/2026-05-17-testing-surface-extension.md` (ADR-0025)

## Problem

`ferrodec-transcend` is a correlated-failure surface by design: the Extended
(50-digit, U256) kernel derives most transcendentals from a small set of
primitives, so a defect in a primitive propagates coherently into every
derivative, and any check whose two sides both flow through that primitive is
structurally blind to it. The astro-float faithful oracle is itself fixed at
256 bits and unsound outside a bounded argument-magnitude domain. ADR-0025
pruned the tautological metamorphic identities; what remains is genuinely
independent verification.

Load-bearing primitives, identified first (kernel recon, `ferrodec-transcend/src/`):

- **Primitives** (direct series/reduction): `exp` (`exp.rs:130`), `ln`
  (`ln.rs:118`), `sin`/`cos` (`sincos.rs:112`, Payne-Hanek `argred.rs`),
  `atan` (`inverse_trig.rs:207`).
- **Derived**: `exp2`,`cbrt`,`pow`,`sinh`,`cosh`,`tanh`,`asinh`,`acosh`,
  `atanh` route through `exp`/`ln`; `log2`,`log10` are `ln·const`;
  `tan`,`asin`,`acos`,`atan2` route through `sin/cos`/`atan`.

`exp` and `ln` carry the widest blast radius (11 derived functions) and are
scrutinised first; `sin`/`cos` second; `atan` third.

Honest v1.x target: strongly-corroborated faithful + frozen worst-case
vectors, **not** proven correct rounding (decimal Table-Maker's-Dilemma; a
research problem per ADR-0021/0024). No overclaiming.

## Acceptance criterion (first-class)

Every added check must be structurally independent of the primitive it
exercises: it must not route through the Extended kernel *or* through
astro-float. mpmath, Arb, and MPFR all satisfy this; metamorphic identities
do not and are out of scope here (ADR-0025 owns them).

## Oracle roles (ratified 2026-05-17; mechanisms user-confirmed)

- **Frozen vectors ← Arb/FLINT (proof tier).** Offline generator
  (`tools/gen_transcend_vectors.py`, `python-flint`), output checked in;
  Arb/FLINT never enter the Cargo graph. Certified ball enclosure decisive
  ⇒ establishes the correctly-rounded value.
- **Correctly-rounded gate ← MPFR via `rug` (corroboration tier).**
  Feature-gated dev-dep (`mpfr-gate`), local opt-in only, never default /
  CI / no_std. Exposes the ternary flag (faithful-vs-CR sign).
- **Broad differential ← mpmath (breadth tier).** Extends the Track-3
  Python subprocess; decimal rounding done on our side. Cheapest, widest,
  least rigorous.

Vector-accept rule: **Arb enclosure decisive AND MPFR agrees**.

## Phases

0. ADR-0026 + provenance/licensing assessment (gates 1–3). `fd-syf`.
1. mpmath special-function differential, exp/ln first, skip-region sweeps,
   behind the existing `differential` feature. `fd-clf`.
2. Arb frozen hard-to-round corpus + decimal TMD search; checked into
   `tests/vectors/transcend/`; default-on Rust vector test. `fd-x3u`.
3. MPFR `rug` cross-validation + ternary-flag CR probe behind `mpfr-gate`.
   `fd-i4e`.
4. Reconcile ADR honest-level to what shipped; close `fd-cb6`; prompt
   before the signed merge (Parnell's YubiKey). `fd-12v`.

## Constraints

- No production kernel source changes (test/tooling/docs only).
- Default `cargo test` and the CI feature matrix stay byte-identical
  (no Python, no `rug`, no new default features).
- One commit per phase, unsigned; signed `--no-ff` merge is Parnell's hand.
- fmt + clippy `--workspace --all-targets -D warnings` + rustdoc
  `-D warnings` before every commit.
- Documentation raft (`fd-au6`, `fd-xpb`) is **not** written here; it is
  blocked on this engagement and authored after it lands.
- README "How ferrodec is developed" disclosure: do not edit.

## Out of scope / sequenced after

`fd-au6`, `fd-xpb` (content-dependent docs), `fd-7f8`, `fd-1ml`, `fd-zf0`,
`fd-tvg`, the `fd-pvu` resolution. `cargo publish` (Parnell's hand).
