//! Conversions between [`Decimal128`](crate::Decimal128) and other types.
//!
//! Submodules:
//!
//! * [`int`] — round-trip with `i32` / `i64` / `i128` / `u32` / `u64` / `u128`.
//! * [`parse`] — `&str` parser (feature-gated by `fmt`).
//! * [`binary`] — `f32` / `f64` conversions (feature-gated by `binary-float`).

mod int;

#[cfg(feature = "fmt")]
mod format;
#[cfg(feature = "fmt")]
mod parse;

#[cfg(feature = "binary-float")]
mod binary;

#[cfg(feature = "fmt")]
pub use parse::ParseDecimalError;
