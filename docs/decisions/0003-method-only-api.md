# ADR-0003: Method-only API; `core::ops` opt-in via feature flag

- **Status**: accepted
- **Date**: 2025-09-01

## Context

`core::ops::Add` etc. take two operands and return one value — no room for a `RoundingMode` argument or `Status` return. Adopting the trait means picking a default rounding mode and dropping the status flags.

ferrodec's design (ADR-0002) makes both choices visible at every call site. The default arithmetic API is methods (`a.add(b, rm) → (Decimal128, Status)`). Whether to *also* implement `core::ops::Add` etc., and what defaults they should commit to, was an open question.

## Decision

Two-tier API:

- **Default (always available)**: methods only. `a.add(b, rm)` returns `(Decimal128, Status)`. Every call carries an explicit `RoundingMode` and a per-call `Status`.
- **Opt-in `ops` feature flag**: implements `core::ops::{Add, Sub, Mul, Div, Rem, Neg}` and the `*Assign` variants. The trait impls always use `RoundingMode::NearestEven` and discard the `Status`. Users who want `+` `-` `*` `/` `%` syntax accept the trade-off explicitly via `Cargo.toml`.

The README's "Why no `core::ops` (and how to opt in)" section frames this for new users.

## Consequences

**Wins:**

- New users see the explicit, principled API first. `RoundingMode` is impossible to forget.
- Existing-codebase users porting from `f64` or `rust_decimal` get familiar operator syntax with one feature-flag addition.
- The traits-via-feature-flag pattern composes cleanly with `num-traits` (which requires `Add` / `Sub` / etc. to be in the public type — see ADR-0002 follow-ups in the 1.4 / 1.5 release window).
- Embedded callers paying for the kernel size only get the trait surface they ask for. No paid-for-but-unused vtable.

**Costs:**

- Two ways to do the same operation, each with different defaults and different semantics around status. Documentation has to explain when to reach for which.
- The `ops` feature flag is a soft public-API surface — users depending on it transitively (e.g. via `num-traits`) need to track the dependency.
- "Opt-in operators" is unusual for Rust crates of similar size; reviewers occasionally flag it as an anti-pattern. The README's design-rationale section preempts that.

**Why this isn't reconsidered:**

The alternative — operators by default, with a hidden `RoundingMode::NearestEven` and silent `Status` drop — looks more idiomatic but hides the two most important characteristics of decimal arithmetic. Embedded calculator users (the original target audience) need explicit rounding control by default; non-embedded users coming from `f64` accept the opt-in cost in exchange for the explicitness elsewhere.

## Related

- `src/ops_traits.rs` — operator implementations gated on `feature = "ops"`.
- `src/num_traits_impls.rs` — `num-traits` adapter, transitively requires `ops`.
- README "Why no `core::ops` (and how to opt in)" section.
- ADR-0002 — the per-op `(value, Status)` shape that motivates the method-first default.
