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
//! The implemented surface is the whole General Decimal Arithmetic numerical
//! specification: the core arithmetic, `squareRoot`, and the four
//! transcendentals `exp`, `ln`, `log10`, and `power`. `exp` / `ln` / `log10`
//! are correctly rounded half-even (like `squareRoot`); `power` is correctly
//! rounded with the context's rounding mode, stronger than the reference, which
//! is only almost always correctly rounded. See ADR-0040 for the transcendental
//! contract and ADR-0038 for the overall design. The crate stays on the `0.x`
//! line pending the final API settle and the deferred performance pass.

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
mod positioning;
mod quantize;
mod round;
mod sqrt;
mod transc;

#[cfg(feature = "fmt")]
mod convert;
#[cfg(feature = "interop")]
mod interop;

pub use context::{Context, Rounding};
pub use decimal::Decimal;

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

#[cfg(feature = "interop")]
pub use ferrodec_ieee::RoundingMode;

pub use ferrodec_ieee::Status;
pub use ferrodec_multiword::DecBig;
