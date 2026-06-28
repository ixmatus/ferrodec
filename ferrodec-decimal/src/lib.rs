//! `ferrodec-decimal`: arbitrary-precision decimal arithmetic.
//!
//! An implementation of the General Decimal Arithmetic Specification (Mike
//! Cowlishaw's `decNumber` and `decTest` family), the parent specification the
//! fixed-width IEEE 754-2019 formats in the rest of the ferrodec workspace
//! derive from. Unlike those formats, a value here has no fixed width: the
//! coefficient is a growable heap integer ([`ferrodec_multiword::DecBig`]), so
//! the crate is `#![no_std]` but requires `alloc` and a global allocator. It
//! is the workspace's "needs an allocator" tier; the fixed formats keep the
//! no-allocator embedded floor.
//!
//! Each arithmetic operation takes an explicit [`Context`] by reference (the
//! working precision, exponent bounds, rounding mode, and clamp flag) and
//! returns a per-operation status, never mutating global state. See ADR-0038
//! for the design.
//!
//! The implemented surface is the whole General Decimal Arithmetic
//! specification. The numerical operations are the core arithmetic,
//! `squareRoot`, and the four transcendentals `exp`, `ln`, `log10`, and
//! `power`: `exp` / `ln` / `log10` are correctly rounded half-even (like
//! `squareRoot`), and `power` is correctly rounded with the context's rounding
//! mode, stronger than the reference, which is only almost always correctly
//! rounded. The miscellaneous operations are the logical `and` / `or` / `xor` /
//! `invert`, `shift` / `rotate`, `scaleb` / `logb`, the next-value operations,
//! the extended comparisons and magnitude selections, `sameQuantum`, `class`,
//! the copy operations, and the classification predicates. See ADR-0038 for the
//! overall design, ADR-0040 for the transcendental contract, and ADR-0041 for
//! the miscellaneous surface. The operation surface is the complete
//! specification. The public API was settled at 1.0 (ADR-0045) and then
//! reopened at 2.0 (ADR-0055): `Decimal` gained `Ord` / `PartialOrd` (the IEEE
//! `totalOrder`), `FromStr`, and `to_f64`, and because `Ord`'s provided
//! `max` / `min` shadow them, the General Decimal Arithmetic `max` / `min`
//! operations were renamed to [`maxnum`](Decimal::maxnum) /
//! [`minnum`](Decimal::minnum). The performance pass is done (ADR-0043,
//! ADR-0044, with post-1.0 follow-ups in ADR-0046); the README's Performance
//! section has the measured speedups.

#![no_std]

extern crate alloc;

mod arith;
mod classify;
mod compare;
pub mod context;
pub mod decimal;
mod digits;
mod divrem;
mod exponent;
mod logical;
mod next;
mod positioning;
mod quantize;
mod round;
mod sqrt;
mod transc;

#[cfg(feature = "fmt")]
mod convert;
#[cfg(feature = "binary-float")]
mod from_float;
#[cfg(feature = "interop")]
mod interop;
#[cfg(feature = "binary-float")]
mod to_float;

pub use context::{Context, Rounding};
pub use decimal::Decimal;

/// Re-exported for [`Context::new`], whose precision is a `NonZeroU32`
/// (ADR-0054: a zero working precision is unrepresentable, not merely
/// documented).
pub use core::num::NonZeroU32;

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

#[cfg(feature = "binary-float")]
pub use from_float::DecimalFromFloatError;

#[cfg(any(feature = "interop", feature = "binary-float"))]
pub use ferrodec_ieee::RoundingMode;

pub use ferrodec_ieee::Status;
pub use ferrodec_multiword::DecBig;
