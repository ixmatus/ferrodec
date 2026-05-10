//! IEEE 754-2019 Decimal64 in pure Rust. `no_std` capable, targeted at
//! embedded use, with the verification posture established by the
//! `ferrodec` crate (Decimal128).
//!
//! This crate is in early development. The currently exposed surface
//! is only the [`Decimal64`] type wrapper; arithmetic, classification,
//! parse, format, and verification land in subsequent commits per the
//! plan archived at
//! `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`
//! in the workspace root.
//!
//! # IEEE 754-2019 §3.5 Decimal64 parameters
//!
//! - Storage width: 64 bits.
//! - Coefficient precision: 16 decimal digits (≈ 53.15 bits).
//! - Exponent range: -383 to 384 unbiased; 0 to 767 biased; bias 398.
//! - Maximum normal magnitude: 9.999999999999999 × 10³⁸⁴.
//! - Minimum positive normal magnitude: 1 × 10⁻³⁸³.
//! - Encoding: BID (binary integer significand) for arithmetic; DPD
//!   (densely packed decimal) planned for IEEE byte-pattern interchange.
//!
//! # Companion crates
//!
//! - [`ferrodec`](https://crates.io/crates/ferrodec): Decimal128, the
//!   sibling at v1.x.
//! - [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32):
//!   Decimal32, the sibling at v1.x.

#![no_std]

mod classify_types;
mod status;

pub use classify_types::IeeeClass;
pub use status::{RoundingMode, Status};

/// IEEE 754-2019 binary integer-significand Decimal64.
///
/// Wraps a 64-bit BID encoding. Arithmetic, classification, parse,
/// format, and conversions are added in subsequent commits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Decimal64(pub(crate) u64);
