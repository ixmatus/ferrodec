# Architecture Decision Records

This directory holds the record of *why* ferrodec is the way it is. Each significant choice — feature scope, encoding, verification posture, performance tradeoffs — gets one Architecture Decision Record (ADR). Together they form the audit log a future reviewer would otherwise have to reconstruct from commit messages and release notes.

## Conventions

- **Filenames**: `NNNN-short-slug.md`, four-digit zero-padded sequence number, lowercase slug. Numbers are never re-used; superseded ADRs keep their slot and link forward.
- **Format**: see `template.md`. Each ADR is short — a single page is the target. The form is more important than the length.
- **Status lifecycle**:
  - `proposed` — drafted, not yet acted on. Avoid this for retroactive ADRs.
  - `accepted` — the decision is in effect.
  - `superseded by ADR-NNNN` — replaced; keep the file as a historical record, link forward.
  - `rejected` — considered and decided against. Document for the next person who wonders the same thing.
- **Plans**: approved planning artifacts (the inputs to /plan output) archive under `plans/` with a date prefix (`YYYY-MM-DD-slug.md`). They're snapshots — the *state at decision time*, not living documents. ADRs reference the plan that produced them when applicable.

## Writing a new ADR

1. Pick the next available number.
2. Copy `template.md` to `NNNN-your-slug.md`.
3. Fill in: status, date, context, decision, consequences, related references.
4. If the decision supersedes a prior one, edit the prior ADR's status line to `superseded by ADR-NNNN`.

Decisions that are reversible or local in scope don't need an ADR — these are for choices that matter to future contributors deciding whether to revisit a path.

## Index

The ADRs in number order:

- [0001 — BID-128 over DPD-128](0001-bid-over-dpd.md)
- [0002 — Per-op `(value, Status)` over global flag word](0002-per-op-status.md)
- [0003 — Method-only API; `core::ops` opt-in via feature flag](0003-method-only-api.md)
- [0004 — Skip Verus pilot graduation](0004-skip-verus-graduation.md)
- [0005 — Will-not-fix `half_down` / `05up` rounding directives](0005-half-down-05up-wontfix.md)
- [0006 — Defer wholesale perf optimization until profile data exists](0006-defer-perf-pass.md) *(superseded by 0007)*
- [0007 — Performance baseline (1.10.1 + bench expansion)](0007-perf-baseline.md)
- [0008 — Performance pass results (1.11.0)](0008-perf-results.md)
- [0009 — DPD interchange behind the `dpd` feature (1.12.0)](0009-dpd-interchange.md)
- [0010 — Testing strategy after the 6-agent correctness review](0010-testing-strategy-after-six-agent-review.md)
- [0011 — Cargo workspace for sibling decimal-precision crates](0011-workspace-for-decimal-siblings.md)
- [0012 — Extract `ferrodec-ieee` after three concrete consumers](0012-extract-ferrodec-ieee.md)
- [0013 — Conformance harness consolidation across the ferrodec family](0013-conformance-harness-consolidation.md)
- [0014 — `Display` notation divergence between Decimal128 and the siblings](0014-display-notation-divergence.md)
- [0015 — Kani scope policy (1.15.0)](0015-kani-scope-policy.md)
- [0016 — Kani harness shim-routing rule (1.15.0)](0016-kani-harness-shim-routing.md)
- [0017 — Decimal64 conformance coverage gap (1.15.0)](0017-decimal64-conformance-coverage-gap.md) *(superseded by 0018)*
- [0018 — Decimal64 H-tier correctness train closing (1.4.0)](0018-decimal64-correctness-train-closing.md) *(supersedes 0017)*
- [0019 — Decimal32 correctness train closing (1.4.0)](0019-decimal32-correctness-train-closing.md)
- [0020 — Decimal128 FMA dynamic sub-ULP trigger (1.15.1)](0020-decimal128-fma-dynamic-subulp-trigger.md)
- [0021 — Exact correctly-rounded oracle supersedes the ULP envelope](0021-exact-oracle-supersedes-ulp-envelope.md)
- [0022 — decimal64 / decimal32 FMA exact-oracle remediation](0022-sibling-fma-exact-oracle-remediation.md)
- [0023 — decimal64 / decimal32 roundToIntegral (§5.9 completion)](0023-sibling-round-to-integral.md)
- [0024 — Faithful sibling transcendentals on one shared Extended kernel](0024-faithful-transcendentals-shared-kernel.md) *(superseded by 0032)*
- [0025 — Metamorphic identity tests with condition-number-derived bounds](0025-metamorphic-identity-tests.md)
- [0026 — Independent transcendental oracles (Arb frozen vectors, MPFR gate, mpmath differential)](0026-independent-transcendental-oracles.md)
- [0027 — rem / % semantic asymmetry across the decimal family (rem_near bridge, 2.0 rename)](0027-rem-semantic-asymmetry.md)
- [0028 — IEEE 754-2019 §9.6 magnitude minimum and maximum](0028-section-9-6-magnitude-min-max.md)
- [0029 — The ferrodec 2.0 major, a consolidated breaking-change plan](0029-ferrodec-2-0-breaking-change-plan.md)
- [0030 — decimal64 / decimal32 FMA subnormal single-rounding (sibling fd-42l port)](0030-sibling-fma-subnormal-single-rounding.md)
- [0031 — GDA decNumber extension operations (1.18.0)](0031-gda-decnumber-extensions.md)
- [0032 — Correctly rounded §9.2 transcendentals via Lefèvre / Muller fixed precision bounds (2.1.0)](0032-correctly-rounded-transcendentals.md) *(supersedes 0024)*
- [0033 — Worst case margin completeness via exhaustive decimal32 enumeration](0033-worst-case-margin-completeness.md) *(extends 0032)*
- [0034 — Empirical coverage extension to decimal32 sqrt and the Kani unreachable identity residue](0034-empirical-coverage-extension.md)
- [0035 — Decimal128 parity train and conformance oracle hardening](0035-decimal128-parity-train.md)
- [0036 — Decimal128 from_f64 / from_f32 signature unification (breaking)](0036-decimal128-from-f64-signature.md)
- [0037 — Compile time decimal literal constructors](0037-compile-time-decimal-literals.md)
- [0038 — Arbitrary precision decimal (`ferrodec-decimal`)](0038-arbitrary-precision-decimal.md)
- [0039 — General decTest conformance for ferrodec-decimal](0039-general-dectest-conformance.md)
- [0040 — Arbitrary-precision transcendentals for ferrodec-decimal](0040-arbitrary-precision-transcendentals.md)
- [0041 — GDA miscellaneous-operation surface for ferrodec-decimal](0041-gda-miscellaneous-operations.md)
- [0042 — Hash-pinned vendored-fixture integrity](0042-vendored-fixture-integrity.md)
- [0043 — DecBig and transcendental performance baseline](0043-decbig-perf-baseline.md)
- [0044 — DecBig performance pass results](0044-decbig-perf-pass-results.md)
- [0045 — ferrodec-decimal public API settle (1.0)](0045-decimal-api-settle.md)
- [0046 — ferrodec-decimal performance follow-ups (post-1.0)](0046-decimal-perf-followups.md)
- [0047 — Exact-result detection suppresses spurious INEXACT on cbrt and pow](0047-exact-flag-cbrt-pow.md)
- [0048 — §7.4 CLAMPED fidelity, and the BID structural residual](0048-clamped-fidelity-and-bid-residual.md)
- [0049 — Closing the GDA decNumber extension residue (compareSignaling, nextToward, Decimal64 DPD)](0049-gda-extension-residue-closure.md)
- [0050 — Anchor band reformulations restore the kernel's relative error model](0050-anchor-band-reformulations.md) *(amends 0032, 0033)*
- [0051 — A signed residual crosses the kernel's rounding seam for grid-exact small arguments](0051-residual-across-rounding-seam.md) *(completes 0050)*
- [0052 — Reference registry under docs/references/ with a default-on schema guard](0052-reference-registry.md)
