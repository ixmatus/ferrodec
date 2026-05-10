//! `&str` ↔ [`Decimal32`] conversions.
//!
//! Both directions are gated on the `fmt` feature: parse uses
//! `core::str` only (no allocation, no `std`), and format writes via
//! `core::fmt::Write` to the caller's `Formatter`.
//!
//! [`Decimal32`]: crate::Decimal32

#[cfg(feature = "binary-float")]
mod binary;

#[cfg(feature = "fmt")]
mod format;

#[cfg(feature = "fmt")]
mod parse;

#[cfg(feature = "fmt")]
pub use format::Engineering;

#[cfg(feature = "fmt")]
pub use parse::ParseDecimalError;
