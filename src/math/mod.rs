//! Calculator transcendentals — `exp`, `ln`, `log10`, `sin`, `cos`,
//! `pow`.
//!
//! Gated behind the `transcendentals` feature.
//!
//! ## v1 accuracy goal
//!
//! The plan calls for **faithfully rounded** (≤ 1 ULP) results
//! against `astro-float` as the oracle. v1 ships a more pragmatic
//! envelope:
//!
//! * Native `Decimal128` arithmetic throughout for exp / ln / log10 /
//!   pow — no extended-precision inner type.
//! * `sin` / `cos` use Payne-Hanek argument reduction
//!   ([`argred`]) with a 6 300-digit `2/π` table, so the trig
//!   accuracy is uniform across the full Decimal128 magnitude range
//!   (no `|x| ≤ 10^9` cap).
//! * Targeted accuracy: `≤ 5 ULP` across the supported domain.
//!   In practice it's typically `≤ 2 ULP`, but we don't promise
//!   correctly-rounded results.
//!
//! Closing the gap to faithful rounding is a follow-up that needs an
//! extended-precision intermediate (Decimal256-equivalent) or a
//! precomputed minimax polynomial table — both out of scope for this
//! commit.

mod consts;
pub(crate) mod extended;

#[cfg(feature = "exp-log")]
mod cbrt;
#[cfg(feature = "exp-log")]
pub(crate) mod exp;
#[cfg(feature = "exp-log")]
pub(crate) mod ln;

#[cfg(feature = "trig")]
mod argred;
#[cfg(feature = "trig")]
mod inverse_trig;
#[cfg(feature = "trig")]
mod sincos;

#[cfg(feature = "hyperbolic")]
mod hyperbolic;

#[cfg(feature = "pow")]
mod pow;

pub use consts::{e, ln10, ln2, pi};
