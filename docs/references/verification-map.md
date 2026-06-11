---
slug: verification-map
category: verification
citation: "ferrodec verification map: claim to mechanism to artifact, one row per load bearing claim."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - docs/testing.md
  - README.md
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The how-is-this-known appendix in table form. docs/testing.md keeps the narrative (the correlated failure surface argument, the oracle independence story); this map only points: claim, mechanism, artifact paths, governing ADR. When a claim gains or loses an artifact, this row moves in the same slice."
---

# Verification map: claim to artifact

The narrative home is [docs/testing.md](../testing.md); this is the
pointer table.

| Claim | Mechanism | Artifacts | ADR |
|---|---|---|---|
| decTest conformance, 0 fail, per-file pass counts pinned | vendored vectors + per-file expectation table | `tests/conformance.rs`, `ferrodec-decimal64/tests/conformance.rs`, `ferrodec-decimal32/tests/conformance.rs`, `ferrodec-decimal/tests/conformance.rs` | 0010 |
| vendored fixture bytes are the vetted bytes | SHA256SUMS manifests + default-on re-hash | `tests/vendored_integrity.rs` and the three sibling copies | 0042 |
| arithmetic is correctly rounded per direction | exact integer oracle, bit-for-bit equality | `ferrodec-test-support/src/oracle.rs`, `tests/oracle_soundness.rs`, `tests/property_fma_oracle.rs` | 0021 |
| §9.2 transcendentals correctly rounded on all three formats | fixed working precision with margin over empirical worst case | `tests/transcend_vectors.rs`, `tests/vectors/transcend/` (`.txt` + `.prov`) | 0032 |
| Decimal32 §9.2 + sqrt margins are exhaustive, not sampled | offline Arb sweep over every canonical input | `tests/vectors/transcend/*_d32_exhaustive.prov`, `tools/d32_exhaustive_sweep.py` | 0033, 0034 |
| near-anchor relative error model holds | reformulations + committed band corpus | `tests/transcend_anchor_bands.rs`, `tests/vectors/transcend/anchor_bands/`, `tools/gen_anchor_band_vectors.py` | 0050 |
| directed modes for grid-stuck small arguments | signed residual across the rounding seam | `tests/transcend_anchor_bands.rs` (band corpus directed rows) | 0051 |
| exact-result flag fidelity (cbrt, pow, exp2, log2, log10) | exact detection before the inexact path | `tests/transcend_vectors.rs` | 0047 |
| transcendental results within 1 ulp (hard-defect net) | faithful astro-float oracle property suites | `tests/property_exp.rs`, `tests/property_hyperbolic.rs`, `tests/property_derived_transc.rs` and siblings | 0021, 0032 |
| algebraic identities hold at any magnitude | metamorphic tests with condition-number bounds | `tests/property_metamorphic.rs` | 0025 |
| agreement with independent implementations | libmpdec + mpmath differential (opt-in) | `tests/differential.rs` and siblings, `tools/diff_oracle.py` | 0026 |
| Arb corpus independently re-derived | MPFR gate (opt-in, conjunctive accept rule) | `ferrodec-test-support/tests/mpfr_gate.rs` | 0026 |
| widening Decimal64 to Decimal128 is value-exact | cross-precision round-trip | `tests/d128_crosscheck.rs` | n/a |
| encode/decode and rounding keystones hold for all inputs | Kani proof harnesses | `src/verify/`, `ferrodec-ieee/src/round.rs` | 0015, 0016 |
| parser survives adversarial input | fuzz targets | `fuzz/` | n/a |
| registry entries stay schema-complete and unrotted | this registry's own guard | `ferrodec-test-support/tests/references_integrity.rs` | 0052 |

Known verification debt, named rather than hidden: Decimal64 and
Decimal128 transcendental margins remain sampled (the exhaustive
sweep covers Decimal32 only; the README disclosure carries this as a
named failure mode), the ADR-0051 sub `10^-100` directed corrections
are pinned only at nearest modes pending a higher precision oracle
pass, and the self-computed transcend corpus has no content-hash
guard (ADR-0042 scoped it out; a future extension).
