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
//! The v1.0 surface is the General Decimal Arithmetic core arithmetic plus
//! `squareRoot`; the transcendental functions are a stated later phase, and
//! the crate stays on the `0.x` line until the specification surface is
//! complete.

#![no_std]

extern crate alloc;

mod arith;
mod compare;
pub mod context;
pub mod decimal;
mod divrem;
mod quantize;
mod round;
mod sqrt;

#[cfg(feature = "fmt")]
mod convert;

pub use context::{Context, Rounding};
pub use decimal::Decimal;

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

pub use ferrodec_ieee::Status;
pub use ferrodec_multiword::DecBig;
