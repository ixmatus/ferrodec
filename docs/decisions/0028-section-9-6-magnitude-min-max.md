# ADR-0028: IEEE 754-2019 §9.6 magnitude minimum and maximum

- **Status**: accepted
- **Date**: 2026-05-18

## Context

ferrodec exposes `min` and `max` on all three decimal types and has
always documented them as the IEEE 754-2019 §9.6 `minimumNumber` and
`maximumNumber` operations: a quiet NaN is a missing value and yields
the other operand, a signaling NaN raises `INVALID`, and an
equal-magnitude or cohort tie is resolved by the §5.10 totalOrder
predicate. The §9.6 magnitude variants `minimumMagnitude` and
`maximumMagnitude` were never implemented. The gap was conspicuous:
§9.6 is mandatory in IEEE 754-2019, the rest of the §9.6 surface was
present, and the vendored Mike Cowlishaw decTest suite ships
`ddMaxMag` and `ddMinMag` for Decimal64 that routed to `Skip` for
want of the methods.

A scoping pass over every decTest operation ferrodec did not yet
dispatch separated two populations. One is dispatcher-gap work: the
operation already exists and only the conformance arm was missing
(the copy family, closed under fd-37z). The other is
implementation-gap work: the operation does not exist at all. Most of
the second population (`and`, `or`, `xor`, `invert`, `rotate`,
`shift`, `reduce`, `divideInteger`, `compareSignaling`, `nextToward`,
and a Decimal64 DPD codec) are General Decimal Arithmetic decNumber
extensions, not IEEE 754-2019 mandatory operations, so they are not
load-bearing for ferrodec's central claim of full IEEE 754-2019
conformance. `minimumMagnitude` and `maximumMagnitude` are the
exception: mandatory in §9.6, and the missing half of a family
ferrodec already half-implemented.

## Decision

Implement `min_magnitude` and `max_magnitude` on `Decimal128`,
`Decimal64`, and `Decimal32` as the §9.6 `minimumMagnitudeNumber` and
`maximumMagnitudeNumber` operations, not the NaN-propagating
`minimumMagnitude` and `maximumMagnitude` variants.

The Number variant is chosen for one reason: consistency with the
existing `min` and `max`, which are already the `…Number` variants.
A caller reaching for the magnitude form on ferrodec gets the same
NaN-as-missing-value contract it already relies on for `min` and
`max`; mixing the propagating variant into the same family under
adjacent names would be a quiet trap. The naming follows the
established convention (`min` is `minimumNumber`, so `min_magnitude`
is `minimumMagnitudeNumber`); the longer spelling is not used because
the short one already departs from the literal spec name on the same
principle.

The operation compares `|x|` and `|y|` *numerically*, so cohort does
not participate in the magnitude decision (`minMagnitude(1.0, 1.00)`
is a magnitude tie, not an ordering). On an equal-magnitude tie the
result defers to `min` or `max`. This is exactly the §9.6 definition
("…otherwise minimum(x, y)") and it inherits, rather than restates,
the sign and cohort tie-break that `min` and `max` already carry and
that conformance already validates: `minMagnitude(-2, 2) = -2` and
`maxMagnitude(-2, 2) = 2` fall out for free. NaN handling is
identical to `min` and `max`: a signaling NaN poisons the result with
`INVALID`, first-sNaN wins, and the §6.2.3 payload is preserved; a
quiet NaN is the missing value; two quiet NaNs yield NaN.

The implementation is not shared code across the crates. The parent
uses its private `numeric_cmp` helper; the siblings use the public
`partial_cmp` on the absolute values, because their internal compare
helper is `Class`-typed rather than value-typed. The observable
semantics are byte-identical across the three precisions so values
flow across formats without drift; the internals are idiomatic per
crate, consistent with the family's standing posture that the
siblings are not sed-mirrors of the parent.

## Consequences

A mandatory §9.6 gap is closed on the surface that the conformance
claim rests on. Decimal64 is conformance-validated against the
canonical decTest vectors with zero failures: `ddMaxMag` 241 of 243
and `ddMinMag` 231 of 233 dispatch and pass, the four residual skips
being `#`-hex BID-interchange operands, exact-match-pinned per
ADR-0010. That zero-failure result is also the strongest available
evidence for the parent and Decimal32 algorithm, which is the same
logic at a different width. Decimal32 ships no `dsMaxMag` / `dsMinMag`
vectors (vector-missing, not a dispatcher gap), so its methods rest
on the unit tests plus the shared-algorithm conformance evidence;
Decimal128 likewise has no vendored `dq` magnitude file and is
unit-tested.

The methods are additive and non-breaking, a SemVer-minor surface
addition on every crate; the version bump and CHANGELOG entries are
release-engineering, not part of this slice.

The remaining unimplemented decTest operations stay deliberately out
of scope and are recorded as a stated path rather than dropped:
`and` / `or` / `xor` / `invert`, `rotate`, `shift`, `reduce`,
`divideInteger`, `compareSignaling`, `nextToward`, and a Decimal64
DPD codec. Each is a General Decimal Arithmetic extension outside the
IEEE 754-2019 mandatory set (or, for the DPD codec, a separate
interchange feature), and each would be its own feature slice plus
ADR if pursued. They are tracked so the path to full
decTest-suite coverage is incremental and explicit, not lost. The
copy-family conformance closure under fd-37z and this §9.6 closure
together reduce the dispatcher gap to that named GDA-extension
residue; the by-design conformance-skip taxonomy (the non-IEEE
rounding directives, ADR-0005) is unchanged.

## Related

- IEEE 754-2019 §9.6 (minimum / maximum / magnitude operations) and
  §6.2.3 (signaling-NaN payload propagation).
- ADR-0010: per-file exact-match conformance expectations; every new
  dispatch arm pins its file count from an observed run.
- ADR-0005: the non-IEEE rounding-directive conformance skips, the
  unrelated by-design skip taxonomy.
- decTest: Mike Cowlishaw's General Decimal Arithmetic testcases,
  vendored under each crate's `tests/vectors/`; provenance in the
  per-directory `README.md`.
- Beads: `fd-8aq` (this implementation and the Decimal64 conformance
  wiring), `fd-37z` (the copy-family dispatcher closure that precedes
  it), `fd-bef` (the deferred GDA-extension residue, the stated path).
