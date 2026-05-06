#![no_std]
#![doc = include_str!("../README.md")]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
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

pub use decimal::{Decimal128, Decimal128BuildError};
pub use status::{RoundingMode, Status};

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

#[cfg(feature = "binary-float")]
pub use convert::Decimal128FromFloatError;

#[cfg(kani)]
mod verify;
