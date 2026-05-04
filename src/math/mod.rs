//! Calculator transcendentals — `exp`, `ln`, `log10` (and, in
//! follow-ups, `sin`/`cos`/`pow`).
//!
//! Gated behind the `transcendentals` feature.
//!
//! ## v1 accuracy goal
//!
//! The plan calls for **faithfully rounded** (≤ 1 ULP) results
//! against `astro-float` as the oracle. v1 ships a more pragmatic
//! envelope:
//!
//! * Native `Decimal128` arithmetic throughout — no extended-precision
//!   inner type.
//! * Targeted accuracy: `≤ 5 ULP` on the documented input domain.
//!   In practice it's typically `≤ 2 ULP`, but we don't promise
//!   correctly-rounded results.
//! * Domain limits documented per-function. Inputs outside those
//!   limits are handled (special cases / overflow / underflow) but
//!   the residual error may exceed the 5-ULP envelope.
//!
//! Closing the gap to faithful rounding is a follow-up that needs an
//! extended-precision intermediate (Decimal256-equivalent) or a
//! precomputed minimax polynomial table — both out of scope for this
//! commit.

mod consts;
mod exp;
mod ln;
mod pow;
mod sincos;

pub use consts::{e, ln10, ln2, pi};
