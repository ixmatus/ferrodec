# ADR-0017: decimal64 conformance coverage gap surfaced during Slice D

- **Status**: superseded by ADR-0018
- **Date**: 2026-05-11

## Context

The 1.15 cycle plan envisioned Slice D as "wire up decimal64's conformance
dispatcher to the same op surface decimal128 has, bump
`expected_per_file` with the resulting pass counts, ship." The plan's
implicit assumption: decimal64 was conformance-clean enough that the
dispatcher arms would mostly pass on first attempt, and any failures
would be a small handful of edge cases that could be triaged
in-slice.

Slice D's first hour disproved the assumption. Adding dispatch arms
for `add`, `subtract`, `multiply`, `divide`, `fma` against the
vendored Cowlishaw decTest suite produced:

* **58 failures in `ddAdd.decTest`** out of ~1091 cases (5.3% failure
  rate).
* **2 failures in `ddDivide.decTest`** out of ~702 cases.
* **1 failure in `ddMultiply.decTest`** out of ~446 cases.
* **A `debug_assert` panic in `ferrodec-decimal64/src/bid.rs:216`**
  (`biased_exp <= BIASED_EXP_MAX`) on some `ddFMA.decTest` case,
  before the dispatcher could even reach its compare step.

The failures cluster into three bug classes:

1. **Magnitude loss in finite-finite addition.** Case `ddadd360`
   produces `0E+50` where the spec expects `1.0000E+5` (100,000).
   The value is dropped entirely, not just rounded incorrectly.

2. **Wrong rounding direction at the 16-digit precision boundary.**
   The `ddadd71100..ddadd71119` sequence (and the negated
   `71200..71219` sequence) exercises additions whose exact result
   rounds to `99.999…9` (16 nines) under half-even, but
   `Decimal64::add` returns `100.0…0` instead. The direction is
   "round-up-to-clean-number" rather than the spec's
   round-half-to-even.

3. **Internal saturation violates `pack_finite`'s precondition.**
   Some `ddFMA` case (not yet narrowed) hits a code path where the
   biased exponent overshoots `BIASED_EXP_MAX`. The
   `pack_finite` `debug_assert!(biased_exp <= BIASED_EXP_MAX)`
   then panics in debug builds.

These shapes match exactly the H-tier bugs the 2026-05-09 six-agent
correctness review found in `Decimal128` pre-1.13 (parse magnitude
loss, FMA sub-ULP directional rounding, pack overflow). The fixes
landed in `Decimal128` in 1.13.x but were never propagated to the
decimal64 port — the conformance dispatcher gap masked the absence.

## Decision

1. **Slice D is rebranded as a discovery and documentation slice.**
   Ship this ADR, the `ferrodec-decimal64/KNOWN_ISSUES.md` entries
   that name each bug class with a specific decTest reproducer, and
   no dispatcher expansion. The Apply-only dispatcher decimal64 has
   today stays.

2. **A dedicated "decimal64 correctness" slice spins out, separate
   from the 1.15 cycle.** That slice runs a six-agent review of
   decimal64 mirroring the 2026-05-09 decimal128 review, identifies
   every bug class, fixes them in per-cadence commits, and only then
   wires up the conformance dispatch arms (which become the
   regression guard for the fixes).

3. **The rest of the 1.15 plan continues as scheduled.** Slices E
   (decimal128 transcendental accuracy), F (test apparatus
   hardening), and G (drift cleanup) do not depend on decimal64
   quality and proceed in order. The sibling transcendentals piece
   originally bundled into Slice D moves into a follow-up after the
   correctness slice closes.

4. **decimal32 also gets a `KNOWN_ISSUES.md`** but its content is
   thinner: the dispatcher gap is the main item, since dsEncode's
   `#hex` decoder isn't wired up yet. decimal32's vector coverage is
   intentionally narrow (only `dsBase` and `dsEncode` ship in the
   workspace) so the conformance signal gap is less severe than
   decimal64's.

## Consequences

**Wins.**

- The gap between "decimal64 1.2.0 is published" and "decimal64
  conforms to the dec spec for the ops users actually call" is now
  written down. Future readers don't have to rediscover it by
  wiring up a dispatcher and watching it fail.
- The KNOWN_ISSUES entries name specific case IDs, so each bug class
  has a one-line reproducer for the eventual fix slice.
- The 1.15 cycle stays on schedule for the work that isn't blocked
  by decimal64 quality (E, F, G).

**Costs.**

- decimal64 ships 1.15 with the same correctness gap it shipped 1.2.0
  with. No regression, but no progress either. The honest accounting
  matters more than aspirational coverage.
- The conformance-dispatch buildout is deferred indefinitely
  (specifically, until the decimal64 correctness slice closes).
  Users who care about decimal64 conformance signal continue to see
  only the `tosci` / `apply` coverage in `expected_per_file`.

**Drift.**

- The `ferrodec-decimal64/KNOWN_ISSUES.md` table should be the
  source of truth for the gap. If the dedicated correctness slice
  closes any of the three bug classes, that file's table is the
  first edit; this ADR's status moves to `superseded by the
  correctness slice's ADR` at the same time.

## Related

- ADR-0010: Testing strategy after the six-agent correctness review
  (the methodology that should be applied to decimal64 in the
  follow-up slice).
- ADR-0015 / ADR-0016: Kani policy ADRs (analogous "what we cover vs
  what we defer" structure).
- `ferrodec-decimal64/KNOWN_ISSUES.md`: per-bug-class table with
  decTest reproducer IDs.
- Plan: 1.15 cycle plan at
  `~/.claude/plans/spawn-6-agents-explore-wondrous-hamster.md`
  (Slice D, revised scope; Slices E, F, G continue as scheduled).
