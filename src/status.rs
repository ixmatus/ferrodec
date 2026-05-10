//! IEEE 754 exception flags and rounding modes — re-exports from
//! [`ferrodec-ieee`](https://crates.io/crates/ferrodec-ieee).
//!
//! These types live in the shared `ferrodec-ieee` crate so that
//! `ferrodec`, `ferrodec-decimal32`, and `ferrodec-decimal64` agree on
//! a single concrete `Status` and `RoundingMode` — values flow between
//! the precisions without conversion.

pub use ferrodec_ieee::{RoundingMode, Status};
