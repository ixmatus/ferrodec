//! `&str` ↔ [`Decimal64`] conversions.
//!
//! Both directions are gated on the `fmt` feature: parse uses
//! `core::str` only (no allocation, no `std`), and format writes via
//! `core::fmt::Write` to the caller's `Formatter`.
//!
//! [`Decimal64`]: crate::Decimal64

#[cfg(feature = "binary-float")]
mod binary;

#[cfg(feature = "binary-float")]
pub use binary::Decimal64FromFloatError;

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
