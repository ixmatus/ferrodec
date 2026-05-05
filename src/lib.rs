#![no_std]
#![doc = include_str!("../README.md")]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
#[cfg(feature = "transcendentals")]
mod math;
mod multiword;
mod ops;
mod status;

#[cfg(feature = "transcendentals")]
pub use math::{e, ln10, ln2, pi};

pub use decimal::{Decimal128, Decimal128BuildError};
pub use status::{RoundingMode, Status};

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

#[cfg(kani)]
mod verify;
