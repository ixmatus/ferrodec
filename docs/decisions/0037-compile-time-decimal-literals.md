# ADR-0037: Compile time decimal literal constructors

- **Status**: accepted
- **Date**: 2026-06-01

## Context

ferrodec targets an STM32U class embedded calculator. Firmware needs to
embed exact published constants (Planck's constant `6.62607015e-34`, the
speed of light `2.99792458e8`, standard gravity `9.80665`) as `const`
values, with no runtime parser, no allocation, and `no_std`. Today the
options are all unsatisfying: `parse_str` is not `const` and rounds;
`try_new(coefficient, exponent)` is not `const`, cannot express the sign
of zero, and turns a published decimal into a coefficient and exponent
the reader has to decode; or the author precomputes a `from_bits(0x...)`
pattern by hand, which is opaque and error prone.

ferrodec 3.1.0 shipped `decode`, a quantum preserving
`const fn decode(self) -> Option<DecimalNParts>` that decomposes a finite
value into `{ negative, coefficient, exponent }`. The natural complement
is its inverse: a `const` constructor that reconstructs the value, plus
an ergonomic layer that lets a source literal read as its published
decimal form.

The owner's posture frames the design: types as the primary
specification, total functions, illegal states unrepresentable, compile
time checks over runtime ones, frugality (every dependency earns its
keep), `no_std` first, and formal verification as a first class activity.
The three sibling formats are authored per format, not generated from one
another.

## Decision

Add the inverse of `decode` in three layers, on all three formats
(Decimal32, Decimal64, Decimal128), as an additive 3.2.0 minor bump.

**Layer 1, the verified primitive (always available, no feature).**

```rust
pub const fn from_parts(parts: DecimalNParts) -> Option<Self>
```

Placed next to `decode` in `classify.rs`. Returns `Some` exactly when the
coefficient is below the format limit and the unbiased exponent is in
range, `None` otherwise. It forms a bijection with `decode` on canonical
finite values: `from_parts(d.decode()?)` reproduces `d` bit for bit, and
`from_parts(p)?.decode()` reproduces `p`. Unlike `try_new` it carries an
explicit sign, so it can build negative zero, and being `const` it works
in `const` initializers even with `default-features = false`. On
Decimal128 the body is two inline bounds checks (computed in `i32`, never
`i16`) plus `pack_finite`; on the siblings it composes the existing
`const` `Coefficient` and `BiasedExp` newtype constructors.

**Layer 2, the legible literal parser (`fmt` gated).**

```rust
pub const fn from_str_const(s: &str) -> Self
```

A `const` byte scanner produces the parts, then delegates to
`from_parts`. On the exactly representable, finite subset it mirrors the
runtime `parse_str` (the same sign handling, leading zero rules, and
quantum derivation) with the rounding machinery removed. Because an
accepted literal carries at most format precision significant figures,
the coefficient fits the native integer type directly, so the scanner
needs neither `U256` nor rounding. A malformed, oversized, or out of
range literal panics, which is a compile error in `const` context: exact
or nothing, never silent rounding.

**Layer 3, the terse spelling (`fmt` gated).**

```rust
macro_rules! dec { ($s:literal) => { $crate::DecimalN::from_str_const($s) }; }
```

A one line `#[macro_export] macro_rules!` per crate, so `dec!("6.626e-34")`
costs no proc macro crate and no `syn` or `quote` dependency. Const
evaluation bakes the value into the binary, so the macro keeps the "no
runtime parser, just a baked constant" property a proc macro would have
offered.

**Grammar: finite only.** The scanner accepts `sign? digits ("." digits?)?
(("e"|"E") sign? digits)?` and rejects Infinity and NaN. Named constants
(`INFINITY`, `NAN`) already cover the specials, and restricting to finite
values keeps the scanner's whole job "produce exact parts or panic", a
clean total specification.

**Threat model.** A `from_str_const` literal is author controlled source,
not attacker input. The defended failure is a programmer typo, and the
right response is a compile time rejection. This is the opposite posture
from `parse_str`, whose threat model is untrusted input and whose answer
is a recoverable error with rounding. The two parsers stay separate.

**Verification.** Layer 1 is proved by three Kani harnesses per format
(`verify/from_parts.rs`): the bijection with `decode` in both directions
over the full symbolic domain, plus rejection totality. Layer 2 reduces
to Layer 1 plus a small scanner, and the scanner's correctness reduces in
turn to "agrees with `parse_str`": a property test checks
`from_str_const == parse_str` bit for bit over thousands of exactly
representable literals per format. Surface forms, the zero cohort, the
`const` and `dec!` paths, and the rejection messages are pinned by
example and `#[should_panic]` tests. The runtime parser is already
conformance tested against the speleotrove suite, so the const parser
inherits that coverage by equivalence rather than duplicating it.

## Consequences

Source embeds published constants as themselves
(`Decimal128::from_str_const("6.62607015e-34")` or `dec!("...")`), with
no runtime cost, no allocation, and no new dependency. `from_parts` works
in a `no_std`, `fmt` off firmware build for callers that already have
integer parts, and completes the `decode` round trip with a Kani proved
bijection. The exact or compile error contract means a mistyped or
inexact constant fails the build rather than silently rounding into a
durable artifact.

The costs are honest. A value with more trailing zeros than the format
has significant figures (`"1"` followed by 34 zeros) is rejected as
written; the author writes it in scientific notation (`"1e34"`), which is
both shorter and clearer. Digit separators and surrounding whitespace are
rejected, because accepting them would diverge the const parser from
`parse_str` and break the equivalence reduction; this is a deliberate non
goal. Each format's scanner is authored separately rather than generated,
consistent with the per format discipline, at the cost of three similar
bodies.

## Rejected alternatives

- **Positional `from_digits(negative, coefficient, exponent)`.** Dominated
  by `from_parts(DecimalNParts { .. })`: same capability, but the
  `(bool, uN, i16)` shape invites transposing the coefficient and the
  exponent, and the struct form mirrors `decode` and is self documenting
  through its named fields.

- **A `dec!` proc macro.** The original framing assumed a proc macro
  because, without a `const` string parser, a proc macro is the only way
  to parse a literal at compile time. Once `from_str_const` exists, `dec!`
  collapses to a `macro_rules!` wrapper with the same baked constant
  result, so a host only proc macro crate and a `syn` or `quote`
  dependency surface buys only marginally terser syntax. Not worth the
  dependency under the frugality posture.

- **A separate panicking `from_parts` variant.** Exposing both
  `from_parts -> Option` and a panicking projection doubles the surface.
  The `Option` form is the honest checked primitive for hand written
  parts (with `.unwrap()` at the `const` site), and `from_str_const` is
  the ergonomic panicking path for literals, so two public entry points
  cover both needs without a third.

- **trybuild compile fail tests for the rejection paths.** `rust-toolchain.toml`
  pins the floating `stable` channel, so committed `.stderr` snapshots of
  const evaluation errors would break CI whenever a stable rustc reformats
  its diagnostics, a maintenance tax misaligned with the permacomputing
  horizon. The property the tests would add is already covered: the
  `const` context test fails to compile if `from_str_const` ever stops
  being `const`, the `#[should_panic]` tests pin the rejection logic and
  every message, and "a panic during const evaluation is a compile error"
  is a Rust language guarantee, not project specific behavior in need of a
  snapshot. Skipped deliberately.

## Related

- Plan: `plans/that-points-at-a-linked-moth.md`.
- Builds on the 3.1.0 `decode` accessor and the `DecimalNParts` structs.
- Other ADRs: ADR-0018 / ADR-0019 (the `BiasedExp` / `Coefficient`
  typed precondition newtypes the sibling `from_parts` composes).
