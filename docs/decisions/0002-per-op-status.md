# ADR-0002: Per-op `(value, Status)` over global flag word

- **Status**: accepted
- **Date**: 2025-09-01

## Context

IEEE 754:2019 §7 specifies five exception flags (`INVALID`, `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`) raised by floating-point operations. The traditional C / IEEE 754 interface exposes these as a *global* word that operations OR into. A long-running computation accumulates flags; the user reads the global word periodically.

That model assumes a single-threaded process, mutable global state, and a discipline of clearing flags at known points. None of those map cleanly to Rust: `static mut` is `unsafe`, thread-local state has cost and surprise factor, and "flags persist across calls" runs counter to Rust's pure-function default.

## Decision

Every fallible arithmetic operation returns `(Decimal128, Status)`. The status is for *that single call only*. Callers compose flags however they want — manually OR'ing, or via `Status::merge` / `BitOr` / `BitOrAssign` impls. ferrodec never reads or writes a global / thread-local flag word.

## Consequences

**Wins:**

- Pure functions everywhere. `add`, `sub`, `mul`, etc. take values and return values. No hidden state, trivially `Send` / `Sync`.
- `Status` is a 1-byte struct (`pub struct Status(u8)`), passed by value. Cost is essentially free — same as returning a single u8 alongside the result.
- Tests and conformance harnesses can compare exact flag bits per call, which decTest expects anyway.
- Composition is explicit: `let mut total = Status::OK; let (r1, s) = a.add(b, rm); total |= s; ...`. Callers decide where flag-accumulation boundaries are.

**Costs:**

- Code is wordier than the global-flag model: every arithmetic call destructures a tuple. The `ops` feature flag (see ADR-0003) accepts the trade for callers who want operator syntax.
- Users coming from C `<fenv.h>` must learn a different idiom. The crate's README's "Quick start" leads with `(value, status) = a.add(b, rm)` for exactly this reason.

**Why this isn't reconsidered:**

Global mutable state for arithmetic flags is a 1980s artifact preserved for C ABI compatibility. Modern Rust libraries that use it (rare) typically ship escape hatches or wrappers anyway. The pure-function shape is the right default for a 2025-vintage Rust crate.

## Related

- `src/status.rs` — `Status` type and IEEE 754 §7 flag definitions.
- ADR-0003 — relaxes the explicit-status requirement when `core::ops` operators are used (with documented `Status` discard).
