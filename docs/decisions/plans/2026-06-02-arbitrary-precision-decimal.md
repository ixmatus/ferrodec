# Plan: `ferrodec-decimal` arbitrary precision decimal

Engagement plan for ADR-0038. Archived in tree so the intent is
reconstructable from the repository alone.

## Context

ferrodec ships three fixed width IEEE 754-2019 formats (Decimal128 / 64 / 32).
IEEE 754 stops there. The General Decimal Arithmetic Specification
(`decNumber` / `decTest`, speleotrove.com) is the authority for arbitrary
precision decimal, and no pure Rust `no_std` formally checked implementation
fills the slot. This engagement plants that flag as a new workspace member,
`ferrodec-decimal`, reusing the existing rounding logic, status types,
conformance harness, and differential oracle rather than reinventing them.

Settled forks: storage is `no_std` + `alloc` (heap bignum coefficient, truly
unbounded); v1.0 surface is the General Decimal Arithmetic core arithmetic plus
`squareRoot` (transcendentals deferred); the coefficient backend is a base
`10^9` decimal limb integer (`DecBig`) built inside `ferrodec-multiword`; the
crate name is `ferrodec-decimal`. Spec authority is the General Decimal
Arithmetic Specification, with `decNumber` as a behaviour oracle only (code
derived fresh per the provenance rules) and Knuth TAOCP Vol 2 §4.3.1 Algorithm
D for division.

## Architecture

1. **`DecBig`** in `ferrodec-multiword` behind a new `alloc` feature: little
   endian `Vec<u32>` limbs in radix `10^9`, normalized, zero is the empty
   vector. Operations: `add`, `sub`, `cmp`, `mul` (schoolbook), `div_rem`
   (Algorithm D), `div_rem_small`, `div_rem10`, `mul_pow10`, `div_rem_pow10`,
   `pow10`, `decimal_digit_count`, `isqrt` (with exact remainder).
2. **`Decimal`** value type: a tagged enum over `Finite { sign, coeff, exp }`,
   `Infinity { sign }`, and `Nan { sign, signaling, payload }`.
3. **`Context`**: working precision, exponent bounds, rounding mode, clamp;
   passed by reference per operation with a per operation `Status` return. The
   eight General Decimal Arithmetic rounding modes live in a crate local enum
   that reuses `ferrodec_ieee::should_round_up` for the five shared modes and
   hand writes `half_down` / `05up` / `up`.
4. **Rounding core**: a fresh mirror of the parent's `round_and_pack_finite`
   five step algorithm over `DecBig` and a context supplied precision.
5. **Operations**: the v1.0 surface, each mapped to its general `*.decTest`
   file; correctly rounded `squareRoot` via the exact `DecBig::isqrt` residue.
6. **Conversion and interop**: unbounded `parse_str`, `Display` in General
   Decimal Arithmetic `toSci` / `toEng` notation, `from_parts` / `decode`, and
   feature gated lossless conversions to and from the three fixed formats.

## Reuse

`ferrodec_ieee::should_round_up` (Kani proved rounding direction),
`ferrodec_ieee::Status` and `RoundingMode`, the parent's
`round_and_pack_finite` as the algorithm to mirror, the precision aware
conformance `Context` and `run_suite` in `ferrodec-test-support`, the existing
`differential::Request` against Python `decimal` (libmpdec, the spec reference
implementation), and astro-float as a second value oracle.

## Slicing

The whole arc runs on one feature branch (`fd-decimal`) with unsigned commits;
a single signed merge to `main` happens at the end, not per slice. The slices
are commit groupings, one concern per commit:

1. `DecBig` bignum in `ferrodec-multiword` (behind `alloc`) plus ADR-0038
   proposed. Highest risk is division correctness; isolate it first.
2. The `Decimal` value type, `Context`, the rounding mode enum, `parse_str`,
   `Display`, and the parts decode round trip.
3. The rounding core plus `add` / `subtract` / `multiply`, with the arbitrary
   type wired into the conformance harness and the first general `decTest`
   vectors vendored and pinned (ADR-0010 per file table). Differential against
   Python `decimal`.
4. `divide` / `divideInteger` / the remainder family / `fma`.
5. `squareRoot` plus the quantum, compare, and select operations.
6. Feature gated interop conversions, the full conformance lock, astro-float
   cross check, the README disclosure entry, ADR-0038 accepted, version
   `0.1.0`, and examples.

## Verification

Bounded property tests against a ground truth `u128` oracle and reconstruction
identities for `DecBig` (Kani is intractable over the heap coefficient; see
ADR-0038), the libmpdec differential at several precisions, astro-float value
cross check, and the general `*.decTest` conformance suite with the ADR-0010
per file expectation guard.

## Versioning

Brand new crate at `0.1.0`. `1.0` waits on the deferred arbitrary precision
transcendentals, full conformance, a settled API, and a performance pass.
Publishing stays the owner's hand.
