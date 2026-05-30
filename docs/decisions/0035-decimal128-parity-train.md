# ADR-0035: Decimal128 parity train and conformance oracle hardening

- **Status**: proposed
- **Date**: 2026-05-29

## Context

A 22-cell adversarially-verified review of the workspace on 2026-05-29
(main `0dd0ad0`, full `--all-features` suite green at 0 fail) surfaced a
single dominant theme: the parent `Decimal128` has drifted *behind* its
siblings. This inverts the usual direction. ADR-0018 (decimal64),
ADR-0019 (decimal32), ADR-0022, and ADR-0030 are a family of sibling
trains that ported parent fixes down to the siblings. This review found
the dual: a family of fixes that landed on the siblings and were never
ported back up to the parent.

The drift went unnoticed because the conformance harness cannot see it.
The parent comparator (`tests/conformance.rs:1110-1117`) accepts any NaN
for any NaN-expected vector: it tests `actual.is_nan()` and nothing
more. A comment there claims a pinned payload is checked, but no payload
or sign comparison exists anywhere in `compare()`. The conformance
status mask (`tests/conformance.rs:1129-1133`) is
`INVALID | DIV_BY_ZERO | OVERFLOW | UNDERFLOW | INEXACT` and omits
`CLAMPED`. The sibling runners (`check()`) share both blind spots. So
the project's stated rule, recorded in `KNOWN_ISSUES.md` ("every
NaN-producing arithmetic op preserves the operand's payload per IEEE
754-2019 §6.2.3 first-NaN-wins rule"), is asserted but not verified, and
the 0-fail gate is vacuous for NaN sign, NaN payload, and `CLAMPED`.
The 1.9.0 cycle added parsing and `Display` for NaN-with-payload
literals precisely because decTest pins them; the comparator just never
looks.

The confirmed parent-versus-sibling divergences, each read at source:

- **`sub` flips a NaN's sign.** The parent negates the second operand
  unconditionally (`src/ops/addsub.rs:85-87`), so `neg()` toggles the
  sign bit of a NaN and the propagated result carries the flipped sign:
  `x - NaN` yields `-NaN`. `decimal64` fixed exactly this
  (`ferrodec-decimal64/src/ops/addsub.rs:78-81`, guard plus a dedicated
  `sub_special_only_for_kani` anti-drift harness, with a doc note citing
  the `ddSubtract.decTest` `ddsub830` cases). `decimal32` carries the
  same unconditional negation as the parent. The parent's own doc
  comment falsely claims the negation "preserves NaN payload."
- **NaN payload is position-first, not sNaN-priority.** The parent
  returns `propagate_nan2(a, b)` for `mul` (`src/ops/mul.rs:58-61`),
  `quantize` (`src/ops/quantum.rs:52-66`), and `divide_integer`
  (`src/ops/divide_integer.rs:89-97`), so a qNaN-times-sNaN mix carries
  the wrong operand's payload. The siblings give the signaling operand
  priority, matching the decNumber dq-vectors.
- **`CLAMPED` is dropped where the siblings raise it.** The parent omits
  `CLAMPED` on the FMA zero-product-with-zero-addend branch
  (`src/ops/fma.rs:251-272`), the `mul` quantum-clamp path
  (`src/ops/mul.rs:83-95`), `finite / Inf` (`src/ops/div.rs:100-109`),
  and the round-and-pack preferred-quantum clamp
  (`src/ops/round.rs:50-61`). `decimal32`'s `mul` clamp path likewise
  omits it where `decimal64` emits it.
- **The parser lacks the sibling overflow caps.** The parent uses a bare
  `digits_after_point += 1` (`src/convert/parse.rs:186-197`) and an
  uncapped `extra_int_digits.saturating_add(1)` whose `as i32` cast
  (`src/convert/parse.rs:296`) turns `u32::MAX` into `-1`, flipping the
  exponent sign. The in-code comment claims a `2^31 - 1` bound the code
  never enforces. The sibling parser caps both counters at
  `MAX_EXPONENT_MAGNITUDE` before the cast, with the rationale recorded
  in `ferrodec-decimal64/src/convert/parse.rs:101-104` (the `L12 / B7`
  hardening). Triggering it needs a multi-gigabyte input, so it is
  unreachable on the STM32U-class target and theoretical on a host, but
  it is a real latent defect with a comment that overstates its
  guarantee, and it is the same drift in a safety-relevant path.

Two further parent correctness defects sit adjacent to the train and are
cheapest to fix in the same engagement:

- **`engineering()` is off by a power of ten.** When a single-digit
  coefficient's scientific exponent is not a multiple of three,
  `format_engineering_into` (`src/convert/format.rs:571-587`) caps
  `int_digits` at the coefficient length and never zero-pads the integer
  part. `Decimal128::try_new(5, 1)` (the value 50) renders `"5E+0"`
  through the public `engineering()` Display wrapper. The three existing
  unit tests (12345, 1234e-7, 1e3) all happen to dodge the shape.
- **`FromPrimitive::from_i64` / `from_u64` double-round through f64.**
  The siblings route integers at or above `10^16` through `n as f64`
  (`ferrodec-decimal64/src/num_traits_impls.rs:124-146`, decimal32
  analogous), but an f64 holds exact integers only to `2^53 ≈ 9·10^15`,
  coarser than the 16-digit target, so the result is rounded in a
  binary intermediate before the decimal rounding. The `Decimal128`
  path is exact. The doc comment frames the f64 route as "appropriate
  rounding," which understates that it is double rounding.

The shared `ferrodec-ieee` primitives, the multiword arithmetic, the DPD
codec, and the sqrt rounding kernels reviewed clean.

## Decision

Run a `Decimal128` parity train, the dual of ADR-0018/0019, in a fixed
order whose first step makes the rest verifiable.

1. **Harden the conformance comparator first.** When the expected token
   pins a NaN sign or payload, compare it; add `CLAMPED` to the
   conformance status mask. Apply to all three runners. This step is the
   reproducer: under Parnell's debugging discipline the failing check
   precedes the fix, and the hardened comparator is expected to turn red
   on the parent for the divergences listed above. The number of newly
   failing vectors is determined by running the hardened suite, not
   asserted here. The leniency was originally justified on the grounds
   that the dec spec does not pin payloads on every NaN result; the
   corrected rule is narrower and correct: check the sign and payload
   the vector actually pins, skip only where the vector leaves them
   free.

2. **Port the sibling fixes up to the parent**, each derived from the
   corrected sibling, not from recall, the reverse of ADR-0030's
   direction. One concern per commit:
   - the `sub` NaN-sign guard, on the parent and on `decimal32`, with
     the parent doc comment corrected;
   - sNaN-priority NaN-payload propagation in the parent `mul`,
     `quantize`, and `divide_integer`;
   - `CLAMPED` emission parity in the parent `mul`, `div`, and
     round-and-pack, and in `decimal32` `mul`;
   - the parser overflow caps on the parent, porting the `L12 / B7`
     hardening and correcting the overstated comment.

3. **Fix the two adjacent parent correctness defects** as standalone
   commits independent of the comparator gate: the `engineering()`
   zero-pad, and the exact-integer conversion for `from_i64` / `from_u64`
   on both siblings (round the integer directly to the format precision
   rather than through f64).

Coherence and coverage follow-ups surfaced by the same review are in
scope to file but out of scope for this ADR's decision, because they are
additive API or test work rather than parent-sibling behavior parity:
IEEE 754-2019 §5.7.2 `same_quantum` is missing entirely on `Decimal32`;
a set of public items exist only on the parent (`is_integer`, `signum`
inherent, `ulp`, `radix`, the `Sum` / `Product` iterators, the
`pi` / `e` / `ln2` / `ln10` constants, `from_f32`) or diverge in
signature (`from_f64`); `FromStrRadixError` is not re-exported from the
siblings; format precision is honored on the parent but ignored on the
siblings; the ADR-0031 GDA cross-check test files promised for
`decimal32` do not exist; and the Kani harness surface is asymmetric in
both directions. Each gets its own issue and, where it changes public
surface, its own ADR.

## Consequences

The 0-fail conformance gate becomes meaningful for NaN sign, NaN
payload, and `CLAMPED`, which is the load-bearing win: the verification
that the README and `KNOWN_ISSUES.md` already claim starts actually
holding. The three formats reach behavioral parity modulo their format
parameters across the arithmetic, quantum, and parse surfaces, so a
caller porting between them stops hitting silent divergences.

The cost is touching shipped 2.x parent code. The NaN-sign and
NaN-payload changes are observable through `to_bits()` on all three
formats and through the parent's payload-rendering `Display`, so they are
behavior changes, not pure internal cleanups, even though every one
moves the result toward the spec and toward the siblings. They are bug
fixes and ship as such; the release engineering decides patch versus
minor against the SemVer signal, the same call ADR-0030 made for the
sibling FMA remediation. The `CLAMPED` and parser changes are not
reachable by any conformance vector today (the comparator could not see
them), so step 1 is what converts them from invisible to guarded.

Provenance is clean: every parent fix is derived from the corresponding
corrected sibling, the exact inverse of the ADR-0030 sibling port, and
the comparator hardening is derived from the decTest vector format, not
from a reference implementation's internals. The durable artifacts are
the hardened comparator (a standing regression guard for the whole NaN
and `CLAMPED` surface) and per-defect regression pins for the shapes
this review found by reading source.

This ADR does not supersede the sibling-train ADRs. It completes the
round trip: ADR-0018/0019/0022/0030 carried parent fixes down; this
carries the sibling fixes back up, and hardens the oracle that should
have caught the gap.

## Related

- Review: the 2026-05-29 multi-agent review (correctness, safety,
  coherence), recorded in active project memory.
- Other ADRs: sibling trains ADR-0018 / ADR-0019 / ADR-0022 / ADR-0030;
  exact-oracle contract ADR-0021; per-op status ADR-0002; testing
  strategy ADR-0010; GDA extensions ADR-0031; the 2.0 break plan
  ADR-0029; `rem` semantics ADR-0027.
- Issues: the `fd-*` parity-train epic and its children filed alongside
  this ADR.
