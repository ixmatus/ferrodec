//! IEEE 754-2019 Decimal128 (BID encoding) for Rust.
//!
//! ferrodec implements the BID-128 format with 34 decimal digits of
//! precision, exponent range `10⁻⁶¹⁴³` through `10⁺⁶¹⁴⁴`, every IEEE
//! special value, faithfully-rounded transcendentals (≤ 1 ULP), and
//! the full §5 / §5.3 / §5.4.2 / §5.7.2 / §5.10 surface.
//!
//! # Two audiences
//!
//! - **Embedded targets** (the original goal): `no_std`,
//!   `forbid(unsafe_code)`, fixed-size buffers throughout, no
//!   allocator dependency. Cross-compiles cleanly to `thumbv6m-none-eabi`
//!   (Cortex-M0+, no FPU, no hardware divide).
//! - **General decimal arithmetic**: opt-in `serde`, `num-traits`,
//!   `ops` (operator overloads), and idiomatic Rust ergonomics
//!   (`FromStr`, `Sum` / `Product`, `core::error::Error` impls). See
//!   the README's "Choosing between ferrodec and `rust_decimal`"
//!   section for the trade-off summary.
//!
//! # Principled by default
//!
//! Every operation returns `(Decimal128, Status)` and takes an
//! explicit `RoundingMode`; `Eq` / `PartialEq` are bitwise; there are
//! no global flags. The `ops` feature enables conventional `+ - * /`
//! operators for users who accept the trade-off (default
//! `NearestEven`, `Status` discarded).
//!
//! # Cohort stability
//!
//! A finite decimal value has many encodings: `1.5`, `1.50`, and
//! `1.500` are one number at different exponents, its *cohort*.
//! ferrodec preserves the numeric value of every operation exactly
//! as IEEE 754-2019 and the General Decimal Arithmetic specify, and
//! that value is stable across the ferrodec formats and against any
//! conforming GDA implementation. The exponent ferrodec selects
//! within the cohort is not guaranteed to match another
//! implementation, and a future ferrodec version may select a
//! different member. Code that serializes or renders the encoding
//! (quantize-then-serialize, fixed-point money display, golden-file
//! comparison) must pin the exponent with `quantize` rather than
//! rely on the default. The same principle, a stable value behind a
//! divergent surface, drives the `rem` / `%` asymmetry across the
//! family (ADR-0027): ferrodec names its divergences rather than
//! hiding them.
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! ferrodec = "1"
//! ```
//!
//! See the [README](https://github.com/ixmatus/ferrodec) for the full
//! crate-level documentation, code examples, and feature surface.

#![no_std]
#![doc = include_str!("../README.md")]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
#[cfg(feature = "dpd")]
mod dpd;
mod iter;
#[cfg(any(feature = "trig", feature = "exp-log"))]
mod math;
mod multiword;
#[cfg(feature = "num-traits")]
mod num_traits_impls;
mod ops;
#[cfg(feature = "ops")]
mod ops_traits;
#[cfg(feature = "serde")]
mod serde_impls;
mod status;

#[cfg(feature = "num-traits")]
pub use num_traits_impls::FromStrRadixError;

#[cfg(feature = "serde")]
pub use serde_impls::serde_bid;

#[cfg(any(feature = "trig", feature = "exp-log"))]
pub use math::{e, ln10, ln2, pi};

pub use classify::IeeeClass;
pub use decimal::{Decimal128, Decimal128BuildError};
pub use status::{RoundingMode, Status};

#[cfg(feature = "fmt")]
pub use convert::{Engineering, ParseDecimalError};

#[cfg(feature = "binary-float")]
pub use convert::Decimal128FromFloatError;

#[cfg(kani)]
mod verify;
