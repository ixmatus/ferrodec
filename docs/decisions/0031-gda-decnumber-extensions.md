# ADR-0031: GDA decNumber extension operations

- **Status**: accepted
- **Date**: 2026-05-20

## Context

After fd-37z closed the copy-family conformance dispatcher gap and
fd-8aq closed §9.6 magnitude (ADR-0028), the decTest residue across the
ferrodec family is the General Decimal Arithmetic decNumber extensions
ADR-0028 lines 92 to 103 enumerated and deferred: `and`, `or`, `xor`,
`invert`, `rotate`, `shift`, `reduce`, `divideInteger`, plus
`compareSignaling`, `nextToward`, and a Decimal64 DPD codec. Each was
recorded as "its own feature slice plus ADR if pursued."

ADR-0029 line 8 then set the 1.x lens: the line is API stable and, after
the §9.6 work, spec complete for the IEEE 754-2019 mandatory surface
across all three formats. The lens is deliberate. It says what the
release-engineering claim rests on and what it does not.

The eight ops `reduce`, `divide_integer`, `logical_invert`,
`logical_and`, `logical_or`, `logical_xor`, `shift`, `rotate` share a
posture distinct from the rest of the GDA-extension residue. They have
complete vendored conformance vectors for Decimal64
(`ferrodec-decimal64/tests/vectors/dd{Reduce,DivideInt,Invert,And,Or,Xor,Shift,Rotate}.decTest`,
1,884 cases that currently route to `Outcome::Skip` for want of a
dispatch arm). Speleotrove ships dq variants for the parent under the
same BSD license. The kernels are exact correctly rounded by spec, so
ADR-0021's exact-oracle posture is the entire correctness story; no
faithful ULP envelope (ADR-0024), no oracle-independence criterion
(ADR-0026), no metamorphic identities apply. The only new infrastructure
is a per-crate `coefficient_to_digits` / `digits_to_coefficient` round
trip; everything else clones the single-operand pattern in
`src/ops/quantum.rs` or the two-operand pattern in `src/ops/rem.rs`.

The frugality argument for landing them now is the canonical-name
argument from CLAUDE.md. A user migrating from `decNumber` or
`java.math.BigDecimal` finds `reduce`, `rotate`, `shift`, the four
logical ops, and `divide_integer` absent and is forced into a workaround
or another crate. The half-implementation that consumes the canonical
name and forces every future user to route around it is the failure
mode the value statement names explicitly. The decTest-coverage
argument is the same gap from the conformance side: 1,884 currently
dispatched-as-Skip cases on Decimal64 alone, with the dq counterparts
ready upstream.

This ADR is a deliberate relitigation of ADR-0029's IEEE-mandatory lens
to admit the eight additive, non-breaking GDA extensions in 1.x. The
lens itself is not amended. These ops are not IEEE 754-2019 mandatory;
the release-engineering claim still rests on the §9.6-closed mandatory
surface, not on GDA-extension completeness. What changes is the
relationship between the lens and the residue: ADR-0028 named the
residue a stated path, and this ADR walks part of it.

The remaining three items (`compareSignaling`, `nextToward`, Decimal64
DPD codec) stay deferred under `fd-ci0`. Each is a separate feature
decision with its own threat model. `compareSignaling` interacts with
the sNaN propagation surface and warrants a careful look at the §6.2.3
payload story before a slice; `nextToward` adds a directed-rounding
neighbor and crosses the rounding-mode surface; the DPD codec for
Decimal64 is interchange infrastructure (`ferrodec-decimal128` already
has DPD behind the `dpd` feature, ADR-0009) and is closer to fd-bef
than to a GDA-arithmetic op. They are not deferred from this slice for
reasons of cost; they are deferred because they are different
decisions.

## Decision

Implement the eight GDA decNumber extension operations on all three
decimal types as additive 1.x surface, behind their canonical names and
in their canonical semantics:

1. `Decimal128::reduce`, `Decimal64::reduce`, `Decimal32::reduce`:
   strip non-significant trailing zeros from a finite operand;
   the zero coefficient normalizes to exponent zero with sign preserved.
2. `Decimal{128,64,32}::divide_integer`: truncated-toward-zero integer
   quotient with exponent zero. `Division_impossible` (INVALID) when
   the integer quotient would exceed format precision in digit count;
   `Division_by_zero` for `finite / 0`; `Invalid_operation` for `0 / 0`.
3. `Decimal{128,64,32}::logical_invert`: digit-wise complement of a
   single non-negative-integer operand whose digits lie in {0, 1} and
   whose exponent is zero; output is precision-wide.
4. `Decimal{128,64,32}::logical_and`, `logical_or`, `logical_xor`:
   digit-wise truth-table ops over two operands under the same
   logical-operand precondition; result exponent is zero, sign positive,
   shorter operand zero-extended on the left.
5. `Decimal{128,64,32}::shift`: shift the coefficient by an integer
   `rhs` in `[-precision, +precision]`. Positive rhs is left shift with
   zero fill on the right; negative rhs is right shift with the low
   digits dropped. rhs must be a true integer (exponent zero or positive
   AND coefficient quantized to that integer); the literal `1.0` is
   rejected as INVALID even though numerically 1.
6. `Decimal{128,64,32}::rotate`: rotate the coefficient by an integer
   rhs in the same domain as `shift`, with shifted-out digits wrapping
   to the other end of the precision-wide digit window.

All eight are correctly rounded by spec: the result of each is a single
exact bit pattern with no rounding tie and no rounding mode parameter.
None of them ever raises `INEXACT`. NaN propagation follows the
family-wide convention: a signaling NaN raises INVALID and yields a
quiet NaN; a quiet NaN passes through.

The verification posture per format is:

- **Decimal64**: per-bucket exact-match pin against the vendored
  `dd*.decTest` files, eight new rows in `expected_per_file()` per
  ADR-0010, the binding "runner compares each file's totals to its row
  and panics on any divergence" guard. ~1,884 cases move from Skip to
  Pass.
- **Decimal128**: vendor the eight `dq*.decTest` counterparts from
  speleotrove verbatim under `tests/vectors/`, dispatch and pin the
  same way. License header (Cowlishaw / IBM BSD) intact.
- **Decimal32**: no `ds*` vectors exist upstream (decSingle was
  historically storage only). Verification rests on two artifacts:
  hand-derived unit tests under
  `ferrodec-decimal32/tests/property_gda_ext.rs` covering NaN
  propagation, sNaN INVALID raise, sign rules, the precision-7 boundary
  cases for shift and rotate, and the zero-coefficient edges; and
  cross-format property tests under
  `ferrodec-decimal32/tests/d64_crosscheck.rs` lifting Decimal32 inputs
  to Decimal64, applying the conformance-proven Decimal64 op, narrowing,
  and comparing. The same lens ADR-0028 lines 84 to 87 used for
  `min_magnitude` and `max_magnitude` Decimal128 / Decimal32 coverage.

The shared infrastructure is the digit-extraction helper
`coefficient_to_digits` / `digits_to_coefficient`. It lives per crate,
not in `ferrodec-multiword` or `ferrodec-ieee`, because the carve-out
in `ferrodec-ieee/src/digits.rs` lines 9 to 13 ("the u32 and u64
variants stay precision-local because their natural callers never need
to call across precisions") generalizes from digit count to digit
extraction: each format's op only ever extracts its own coefficient.
The helper is ~30 lines per crate including its inverse and its
round-trip property test. `ferrodec-multiword`, `ferrodec-ieee`, and
`ferrodec-transcend` stay unchanged.

The surface is additive and non-breaking, a SemVer-minor bump on every
crate that gains methods: `ferrodec` 1.17.0 to 1.18.0,
`ferrodec-decimal64` 1.7.0 to 1.8.0, `ferrodec-decimal32` 1.7.0 to
1.8.0. The version bump and CHANGELOG entries follow the
release-engineering convention ADR-0028 set ("not part of this slice"
in the per-op sense; this ADR groups them as the slice-final commits).

## Consequences

The decTest dispatcher gap collapses to the three deferred residual
items: `compareSignaling`, `nextToward`, Decimal64 DPD codec. The
copy-family closure (fd-37z), the §9.6 closure (ADR-0028), and this
GDA-extension closure together leave the by-design conformance-skip
taxonomy of ADR-0005 (the non-IEEE rounding directives) as the only
remaining `Skip` source on the dispatcher side.

Twenty-four new method signatures land across the three formats (eight
ops on each of `impl Decimal128`, `impl Decimal64`, `impl Decimal32`),
all `pub fn`, all non-breaking. The two-operand ops carry the standard
ferrodec `(Self, Status)` return; the single-operand ops the same.

The conformance-pass accounting moves measurably. Decimal64 gains
~1,884 dispatchable cases at the exact pass count of its first green
local run, pinned per ADR-0010. Decimal128 gains the dq counterpart
counts at the same exact-pin discipline. Decimal32 stays at its
existing dispatched count (no new vector files), but acquires hand
plus cross-format coverage equivalent in shape to the §9.6 magnitude
treatment.

The maintenance cost is one new small module per crate
(`coefficient_to_digits` / `digits_to_coefficient` with a round-trip
property test) and one new logical-ops kernel
parameterized on a 2-bit truth table that serves
`logical_and / or / xor`. No new Kani harnesses: the ops are exact and
dispatch-only, with no rounding-mode interaction or special-case shim
shape that bounded-symbolic execution would meaningfully add to over
the decTest corpus.

The lens of ADR-0029 is preserved verbatim. The release-engineering
claim still rests on the §9.6-closed IEEE 754-2019 mandatory surface.
The 2.0 plan is untouched: this ADR is additive only and does not
amend the breaking-change set in ADR-0029.

The deferred residue stays deferred. `fd-ci0` carries the umbrella
forward; `compareSignaling`, `nextToward`, and the Decimal64 DPD codec
each await their own ADR if pursued.

## Related

- General Decimal Arithmetic specification, Mike Cowlishaw, at
  speleotrove.com/decimal/decarith.html, the source of the eight ops'
  semantics and the conformance vectors.
- ADR-0010: per-file exact-match conformance expectations; every new
  dispatch arm in this slice pins its file count from an observed run.
- ADR-0021: the exact correctly-rounded oracle that governs the
  verification posture; these ops live entirely in its domain.
- ADR-0028: the slice that closed §9.6 and recorded the residue this
  ADR walks; lines 92 to 103 list the eleven items, lines 84 to 87 set
  the Decimal32 plus Decimal128 vector-missing precedent.
- ADR-0029: the 1.x lens this ADR deliberately relitigates without
  amending; the 2.0 plan is unaffected.
- Beads: `fd-ci0` (the umbrella for GDA-extension and adjacent
  residue; carries the three deferred items after this slice closes),
  the per-op children file inside it.
- `feedback_gpg_agent_ssh_silent_merge_abort.md`: the YubiKey-boundary
  discipline that governs the slice-final signed merge.
