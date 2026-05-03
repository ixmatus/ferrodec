#![no_std]
#![doc = include_str!("../README.md")]

mod bid;
mod classify;
mod cmp;
mod convert;
mod decimal;
mod multiword;
mod ops;
mod status;

pub use decimal::Decimal128;
pub use status::{RoundingMode, Status};

#[cfg(feature = "fmt")]
pub use convert::ParseDecimalError;

#[cfg(kani)]
mod verify;
