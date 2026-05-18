# ADR-0027: rem / % semantic asymmetry across the decimal family (rem_near bridge, 2.0 rename)

- **Status**: accepted
- **Date**: 2026-05-18

## Context

The method named `rem` carries different mathematics across the three
sibling crates, and so does the `%` operator. The divergence has three
layers.

**Named methods.** `Decimal128::rem` is the IEEE 754-2019 §5.3.1 remainder:
`r = x − n·y` where `n` is `x/y` rounded to the nearest integer, ties to
even, so `|r| ≤ |y|/2` and the result is always exact. `Decimal128` also
exposes `rem_trunc`, the General Decimal Arithmetic truncated remainder
(`n = trunc(x/y)`, sign of the dividend, `|r| < |y|`, the C99 `fmod` and
decTest `remainder` rule). The siblings expose only `rem`, and their `rem`
*is the truncated one*: `Decimal64::rem` and `Decimal32::rem` compute what
`Decimal128::rem_trunc` computes, not what `Decimal128::rem` computes. The
siblings have no `rem_near`; the nearest-even remainder was deferred at
their 1.4.0 slice (Phase 1 Decision 2).

**The `%` operator.** `core::ops::Rem` is implemented for every format, so
idiomatic `%` silently changes its mathematics by type: `Decimal128 % _`
is the nearest-even remainder, `Decimal64 % _` and `Decimal32 % _` are the
truncated remainder. The two differ exactly at the half-quotient boundary
(`5 % 2` is `1` truncated, `-1` nearest-even).

**Signature.** `Decimal128::rem(self, rhs)` takes no rounding mode;
`Decimal64::rem(self, other, _rm)` takes an unused `RoundingMode` for
binary-operation signature parity. A caller porting code reads two
different shapes for one name.

This is not a value bug. Each operation is individually spec-correct and
exhaustively tested (the parent against the exact integer oracle and
decTest `remainder` / `remaindernear`; the siblings' truncated `rem`
against decTest and the H5 regression pins). It is an API-surface hazard:
generic code over a decimal abstraction, or code ported between the
formats, silently gets a different remainder from `.rem()` or `%` with no
type error and no diagnostic. A same-named method with divergent semantics
across sibling crates is a sharp footgun, sharper than the informational
`fd-61r` divergence because the surprising state is freely representable
and reachable through the most idiomatic spelling.

The cross-precision oracle already had to route around this: the
`d128_crosscheck` `rem` reference is `Decimal128::rem_trunc`, the variant
that matches the siblings, precisely because plain `Decimal128::rem` does
not (fd-pvu, recorded there).

## Decision

**Destination (2.0, breaking): unambiguous names everywhere.** The bare
`rem` name and the `%` operator are the hazard, because a reader cannot
tell from the call site which remainder runs. The 2.0 surface exposes
`rem_near` (IEEE §5.3.1 nearest-even) and `rem_trunc` (GDA truncated) on
all three formats, with bare `rem` and the `core::ops::Rem` (`%`)
implementation removed or reserved so that no unqualified spelling can
silently pick a rule. This is recorded now as the committed direction so
the 1.x work builds toward it rather than away from it; it is not done in
1.x because removing `rem` and `%` is a breaking change.

**Bridge (1.x, non-breaking, done in this slice): add sibling
`rem_near`.** `ferrodec-decimal64` and `ferrodec-decimal32` gain a
`rem_near` method computing the IEEE 754-2019 §5.3.1 nearest-even-quotient
remainder, mirroring `Decimal128::rem`. After this the explicit-name set
is symmetric across the family: every format has both `rem_near` and a
truncated remainder (`Decimal128::rem_trunc`; the siblings' existing
`rem`, which is the truncated op). Code that wants a guaranteed rule can
already migrate to the explicit name today, before the 2.0 break lands,
and the porting hazard is removed for any caller that adopts the explicit
spelling.

In 1.x the bare `rem` and `%` keep their current per-format meaning. They
are not changed, because silently changing the math of an existing
operation (alternative (c)) is the worst outcome: it corrupts downstream
data with no error, exactly the failure the explicit names exist to
prevent. They are instead documented prominently as the ambiguous
spellings, with the divergence and the migration path spelled out in
rustdoc and the planned "Porting between the ferrodec formats"
documentation (the fd-pvu-adjacent docs raft: `fd-7f8`, `fd-1ml`,
`fd-zf0`, cross-referencing this ADR).

Rejected alternatives. **(a) alone** (add `rem_near`, never rename) leaves
the bare-`rem` / `%` footgun permanently; it is the bridge, not the
destination. **(c)** (redefine bare `rem`/`%` to one rule across the
family in 1.x) is a silent behavioural change to a shipped, data-bearing
operation on whichever side changes, with no compiler signal to callers;
rejected outright. The chosen path is **(a) as the 1.x bridge toward (b)
as the 2.0 destination**.

## Consequences

- 1.x: `Decimal64::rem_near` and `Decimal32::rem_near` exist, computed by
  a half-even-quotient kernel mirroring `Decimal128::rem`, asserted
  bit-for-bit against the exact integer oracle (ADR-0021) and the decTest
  `remaindernear` conformance vectors, which the sibling conformance
  dispatch now runs instead of skipping. Additive, SemVer-minor; no
  existing behaviour changes, so no existing test moves except the
  conformance per-file expectation counts that rise as `remaindernear`
  (and the truncated `remainder`) stop being skipped, recomputed from
  the actual run per the ADR-0010 exact-match discipline.
- The siblings' `rem` docstring no longer says the nearest-even variant
  is "deferred"; it points to `rem_near` and this ADR, and names itself
  the truncated remainder plainly.
- A follow-up bead tracks the 2.0 rename (reserve or remove bare `rem`
  and the `%` `Rem` impl family, settle the final explicit names). It is
  not scheduled here; it is the recorded destination, to be relitigated
  only against this ADR.
- The `rem_near` addition lands after the 1.5.0 / ferrodec-ieee 0.1.4
  tags were already cut (release-engineering step 1). It is new public
  API, so the siblings need a version above 1.5.0 before publish; the
  CHANGELOG entry sits under `## [Unreleased]` and the version decision
  is surfaced to Parnell at the release-engineering pass, not taken here.
- `fd-61r` is unaffected: that is an informational-flag divergence, this
  is a value-operation divergence; different axes, both named rather than
  hidden.

## Related

- Tracking: bead `fd-pvu` (this decision); the docs raft `fd-7f8`
  (cross-format `rem` rustdoc cross-references), `fd-1ml` (porting
  section), `fd-zf0` (cohort / value-stability note) cross-reference
  this ADR; a new bead carries the 2.0 rename.
- Other ADRs: ADR-0021 (faithful contract, the exact integer oracle that
  validates `rem_near`), ADR-0010 (conformance exact-match-per-file
  discipline, the count guard touched here), ADR-0023 (the sibling
  §5.9 completion, the precedent for adding a missing spec operation to
  the siblings). Supersedes none.
- Code: `src/ops/rem.rs` (`Decimal128::rem` / `rem_trunc`, the
  `rem_finite` half-even kernel mirrored), `ferrodec-decimal64/src/ops/
  rem.rs` and the `ferrodec-decimal32` mirror, the `d128_crosscheck`
  `rem_trunc` routing note.
