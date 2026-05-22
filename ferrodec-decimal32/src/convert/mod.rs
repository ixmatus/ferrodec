//! `&str` ↔ [`Decimal32`] conversions.
//!
//! Both directions are gated on the `fmt` feature: parse uses
//! `core::str` only (no allocation, no `std`), and format writes via
//! `core::fmt::Write` to the caller's `Formatter`.
//!
//! [`Decimal32`]: crate::Decimal32

#[cfg(feature = "binary-float")]
mod binary;

#[cfg(feature = "binary-float")]
pub use binary::Decimal32FromFloatError;

// Integer conversions need neither `fmt` nor `binary-float`: they
// scale the decimal coefficient directly. Always available.
mod int;

#[cfg(feature = "fmt")]
mod format;

#[cfg(feature = "fmt")]
mod parse;

#[cfg(feature = "fmt")]
pub use format::{Engineering, FixedPreferred};

#[cfg(feature = "fmt")]
pub use parse::ParseDecimalError;
