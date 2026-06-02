# ADR-0038: Arbitrary precision decimal (`ferrodec-decimal`)

- **Status**: proposed
- **Date**: 2026-06-02

## Context

ferrodec ships three fixed width decimal formats: the parent `ferrodec`
(Decimal128), `ferrodec-decimal64`, and `ferrodec-decimal32`. IEEE 754-2019
specifies exactly those widths and nothing wider. The canonical specification
for arbitrary precision decimal is Mike Cowlishaw's General Decimal Arithmetic
Specification (the `decNumber` and `decTest` family), which is also the parent
specification the three IEEE formats derive from. A user arriving from IBM
`decNumber`, Python `decimal` (libmpdec), or `java.math.BigDecimal` finds no
pure Rust, `no_std`, formally checked arbitrary precision decimal that follows
the specification. The canonical slot sits empty.

The constraint that shapes the design is the project's embedded floor. The
fixed formats run on the Cortex-M0+ floor with no allocator. Arbitrary
precision needs a coefficient that grows without a fixed bound, which the
domain cannot satisfy without allocation. So the new crate occupies a
different tier from its siblings: it needs a heap.

A second constraint is correctness tooling. The fixed formats are verified by a
layered stack: Kani proofs on the bit level predicates, property tests against
astro-float, and the format specific `dq` / `dd` / `ds` conformance vectors. An
arbitrary precision crate wants the same rigor, but two pieces do not transfer.
Kani does not reason tractably about an unbounded heap coefficient, and the
`ferrodec-transcend` kernel, pinned to 50 working digits with a fixed
6300 digit Payne-Hanek table, does not generalize to a working precision chosen
at runtime.

## Decision

Add a new workspace member, `ferrodec-decimal`, providing arbitrary precision
decimal arithmetic to the General Decimal Arithmetic Specification.

**Storage.** `#![no_std]` with `alloc`, and `#![forbid(unsafe_code)]` like the
rest of the workspace. The coefficient is a heap bignum, so the crate requires
a global allocator. This is the deliberate "allocate where the domain needs it"
tier; the fixed format no allocator path is untouched.

**Coefficient backend.** A new growable base `10^9` decimal limb unsigned
integer, `DecBig`, built inside `ferrodec-multiword` behind a new `alloc`
feature. Decimal radix storage makes the operations a decimal kernel leans on
cheap: scaling by a power of ten is a limb shift, the digit count is a length
computation, and trailing digit extraction needs no binary to decimal step.
Radix `10^9` in a `u32` limb keeps each partial product inside a `u64`
accumulator, which lowers well on the 32 bit floor; a `10^19` radix would force
`u128` libcalls. The long division is Knuth Algorithm D (TAOCP Vol 2 §4.3.1)
specialized to radix `10^9`, derived from the algorithm description rather than
transcribed.

**Spec authority and oracle.** The General Decimal Arithmetic Specification is
authoritative. The `decNumber` C library is a behaviour oracle only, never a
code template. Python `decimal` is libmpdec, which is the specification's
reference implementation, so it serves as the differential oracle at a matched
context. The general (precision driven) `*.decTest` suite is the conformance
validator; those vectors are not yet vendored and will be added per operation
group.

**Context and rounding.** A `Context` carries the working precision, exponent
bounds, rounding mode, and clamp flag, passed by reference per operation with a
per operation `Status` return, consistent with ADR-0002 and ADR-0003. The
General Decimal Arithmetic eight rounding modes live in a crate local enum that
reuses `ferrodec_ieee::should_round_up` for the five it shares with the IEEE
formats and hand writes the three the fixed formats deliberately decline
(ADR-0005). The crate reuses `ferrodec_ieee::Status` unchanged; the
specification's extra conditions fold onto the existing flags exactly as the
conformance runner already maps them.

**v1.0 surface.** The General Decimal Arithmetic core arithmetic plus
`squareRoot`: `add`, `subtract`, `multiply`, `divide`, `divideInteger`, the
remainder family, `fma`, `compare` and `compareTotal`, `quantize`, `rescale`,
round to integral, `reduce`, the sign and select operations, and the logical,
shift, and rotate extensions already specified for the fixed formats
(ADR-0031). Correctly rounded `squareRoot` uses the exact `DecBig` integer
square root residue to decide the final digit. Transcendentals (`exp`, `ln`,
`log10`, `power`) are out of v1.0 scope and deferred to a later phase that must
derive a fresh arbitrary precision argument reduction and error budget.

**Verification.** `DecBig` follows the precedent the fixed width
`ferrodec-multiword` primitives already set: property tests against a ground
truth oracle, not Kani. The oracle is `u128` arithmetic over the full `u128`
range (which exercises up to five limbs) plus reconstruction identities
(`q*v + r == u` with `r < v`, scale round trips, square root floor) for
operands wider than `u128`. Kani was measured against the heap type and found
intractable: a single one limb `(a + b) - b == a` harness expands to roughly
66 million SAT variables (the `Vec` equality lowers to a `memcmp` loop that
does not unwind) and does not discharge in minutes, so no Kani harnesses ship
on `DecBig`. The decimal layer's rounding direction continues to rest on the
existing Kani proved, precision independent `should_round_up`, which is a pure
digit level predicate Kani discharges exhaustively.

**Versioning.** `ferrodec-decimal` starts at `0.1.0`, not `1.0`. Under the
project's spec completeness rule, `1.0` waits on the deferred arbitrary
precision transcendentals, full general suite conformance, a settled public
API, and a performance pass. The `0.x` line states plainly that the API may
break.

## Consequences

A global allocator becomes a requirement for this one crate, which is a real
departure from the no allocator floor and is why the decision is recorded
rather than folded into a release. The fixed formats and their embedded path
are unaffected: `DecBig` sits behind the `ferrodec-multiword` `alloc` feature,
off by default.

The conformance harness gains the general `*.decTest` suite, broadening what
the test apparatus understands beyond the fixed format dispatch. The rounding
superset living local to the crate keeps the IEEE crate at five modes and
honors ADR-0005, at the cost of a small amount of duplicated mode dispatch.

The verification posture is honest about the heap: a heap coefficient does not
get a whole input Kani proof, and the property test oracle plus the reuse of
proved digit level predicates carry the weight instead. The specific failure
mode this leaves exposed, and which the disclosure must name, is a rounding or
boundary error on an input wider than the `u128` oracle reaches that no
property test draw happened to generate.

The deferred transcendentals mean v1.0 is a deliberately narrow but coherent
cut, with the path to completeness stated. The crate plants the canonical name
without over promising the full surface.

## Related

- Plan: `plans/2026-06-02-arbitrary-precision-decimal.md`
- Other ADRs: ADR-0002 (per op Status), ADR-0003 (method only API), ADR-0005
  (`half_down` / `05up` will not fix for the fixed formats), ADR-0010
  (conformance per file expectation table), ADR-0031 (GDA `decNumber` extension
  operations), ADR-0037 (compile time decimal literals).
- References: General Decimal Arithmetic Specification and the `decNumber` /
  `decTest` suite (speleotrove.com/decimal); Knuth, *The Art of Computer
  Programming*, Volume 2, §4.3.1, Algorithm D; IEEE 754-2019.
