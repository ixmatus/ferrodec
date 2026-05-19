# ADR-0029: The ferrodec 2.0 major, a consolidated breaking-change plan

- **Status**: accepted
- **Date**: 2026-05-18

## Context

The 1.x line is API-stable and, after the §9.6 work, spec-complete for
the IEEE 754-2019 mandatory surface across all three formats. Over the
1.x cycle three distinct breaking changes were each identified, judged
correct, and deferred to a future major rather than forced into a minor.
Each was recorded where it arose:

- ADR-0027 chose, as decision (b), to retire the ambiguous bare `rem`
  name and the `core::ops::Rem` (`%`) implementation across the family;
  bead `fd-9n5` carries it, deferred to 2.0.
- `fd-7f1` (Agent 5 B9, restated in ADR-0018) defers a richer
  `parse_str` error enum; the current `ParseDecimalError` cannot
  distinguish an empty string from a misplaced sign from an invalid
  character, and widening it is a breaking error-surface change.
- ADR-0014 fixed the `Display` notation divergence in place for 1.x and
  recorded that harmonizing `Decimal128` onto the General Decimal
  Arithmetic `toSci` rule (which the siblings already use) is a 2.0
  concern, because it changes printed output for any value with a
  non-negative adjusted exponent.

Scattered like this, the deferrals have three failure modes. They get
relitigated from zero each time someone rediscovers one. They get
dribbled across successive majors, and every major is an ecosystem cost
the whole dependent tree pays. Or 2.0 accretes unrelated scope because
no document says what 2.0 is and is not. The deferrals are decisions
already taken; what is missing is a single place that states the
destination and freezes it.

## Decision

The ferrodec 2.0 major is exactly the following three breaking changes,
shipped together in one major bump across `ferrodec`,
`ferrodec-decimal64`, and `ferrodec-decimal32`, and nothing else unless
this ADR is amended first.

1. **Retire the ambiguous remainder spellings.** Remove or
   compile-error-reserve bare `rem` and the `core::ops::Rem` (`%`)
   implementation on all three formats. The surface becomes the
   explicit `rem_near` (IEEE 754-2019 §5.3.1 nearest-even quotient) and
   `rem_trunc` (General Decimal Arithmetic truncated quotient) only, so
   no unqualified spelling can silently pick a rule. The siblings'
   current bare `rem`, which is the truncated operation, becomes
   `rem_trunc`. Rationale and the rejected alternatives are in ADR-0027;
   this ADR does not reopen them. The 1.x bridge (sibling `rem_near`,
   parent `rem_trunc`) already shipped, so a caller can migrate to the
   unambiguous names today and cross the 2.0 boundary with no behaviour
   change.

2. **Distinguish parse failures in the type.** Replace the coarse
   `ParseDecimalError` with an enum whose variants separate the
   genuinely different failures a caller acts on differently: empty
   input, an invalid character, a misplaced or doubled sign, a
   malformed exponent, and coefficient or exponent out of range. This
   moves a fact the current API hides at runtime into a value the
   caller can match, the project's standing preference for making
   illegal states distinguishable rather than collapsing them. It is
   breaking because the error type and its variant set are public.

3. **Harmonize `Display` on `toSci`.** Switch `Decimal128`'s default
   `Display` to the GDA `toSci` rule, so all three formats print by the
   one canonical decimal-IO convention that already aligns with
   decTest, Python, and Java. Provide an opt-in `Notation::FixedPreferred`
   (or equivalent) that reproduces the 1.x `Decimal128` rendering for
   callers who depend on it. Per ADR-0014 this changes output for any
   value with a non-negative adjusted exponent and is therefore
   breaking; the escape hatch makes the migration mechanical.

**One major, frozen scope.** All three land in the same 2.0; they are
not spread across 2.0, 3.0, and later. A new breaking idea after this
ADR does not expand 2.0; it waits for a later major and earns its own
ADR. 2.0 is relitigated only against this document.

**Explicitly not in 2.0.** Speculative idiom redesigns are out of
scope unless separately decided and recorded: typestate on the rounding
mode, sealing `Status`, ownership reshaping of the operation surface,
or any "tighten a runtime check into the type" change that is desirable
but not a decided break. The discipline that keeps majors cheap is the
same one that keeps this list short. The `to_f64` signature change is
not a 2.0 item: it already shipped in the 1.4.0 trains (ADR-0018,
ADR-0019) and is history, not destination.

Nothing in this ADR executes in 1.x. It is the recorded target the
remaining 1.x work builds toward, not away from.

## Consequences

- 2.0 is now a single coherent target a contributor can read in one
  place. `fd-9n5` and `fd-7f1` point here and are worked together when
  the major is scheduled; ADR-0027's and ADR-0014's 2.0 sections keep
  their per-change rationale and are subsumed by this ADR only for
  sequencing and scope. ADR-0027's status is updated to executed for
  its 1.x bridge half when 2.0 lands its destination half.
- The dependent ecosystem pays one major-version migration, not three.
  The 2.0 `CHANGELOG` carries a single consolidated `Breaking` section
  with the mechanical migration for each change: rename `rem` / `%`
  call sites to `rem_near` or `rem_trunc`; match the new parse-error
  variants; opt into `Notation::FixedPreferred` if the old `Decimal128`
  `Display` is required. Every break has a documented escape, so no
  caller is stranded.
- No test moves now; this is a planning artifact. When 2.0 executes,
  the conformance per-file expectations are recomputed from the run per
  the ADR-0010 exact-match discipline (the `remaindernear` /
  `remainder` rows already account for the operations; only the
  spellings change), and the parse-error and `Display` suites are
  rewritten against the new surfaces.
- The exact version mechanics (whether all crates jump to 2.0.0 in
  lockstep or keep their independent numbers while each takes a major)
  are a release-engineering decision for the 2.0 pass, not taken here.
  What is fixed here is that the rem/% and `Display` changes are
  cross-format, so the three crates take the major together rather than
  one sibling diverging.
- `fd-61r` is unaffected and is not a 2.0 item: it is an informational
  flag divergence deferred on cost grounds, a different axis from these
  three surface breaks. The `fd-bef` General Decimal Arithmetic
  extension residue (ADR-0028) is additive, not breaking, and is not
  gated to a major.

## Related

- Subsumed for scope and sequencing: ADR-0027 (rem / % asymmetry, the
  decision (b) destination), ADR-0014 (`Display` divergence, the v2.0
  harmonization plan). Restating deferral: ADR-0018 (the richer parse
  error enum deferred as breaking).
- Tracking: `fd-9n5` (rem / % retirement), `fd-7f1` (parse error enum);
  both stay deferred and now reference this ADR as the consolidated
  index. A parse-enum tracking bead is not separately filed; `fd-7f1`
  carries it.
- Discipline: ADR-0010 (conformance exact-match-per-file, the guard the
  2.0 execution recomputes against). Supersedes none; amends none in
  substance, only indexes ADR-0027 and ADR-0014 for the major.
