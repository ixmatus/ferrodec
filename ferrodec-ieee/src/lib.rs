//! Shared IEEE 754-2019 metadata types for the `ferrodec` family.
//!
//! This crate factors out the precision-agnostic types that all three
//! ferrodec siblings ([`ferrodec`](https://crates.io/crates/ferrodec)
//! at Decimal128, [`ferrodec-decimal32`](https://crates.io/crates/ferrodec-decimal32)
//! at Decimal32, [`ferrodec-decimal64`](https://crates.io/crates/ferrodec-decimal64)
//! at Decimal64) need to share for cross-precision interop:
//!
//! * [`Status`] — the IEEE 754-2019 §7 exception flags (`INVALID`,
//!   `DIV_BY_ZERO`, `OVERFLOW`, `UNDERFLOW`, `INEXACT`) packed in a
//!   single byte. Returned by every operation that can lose precision.
//! * [`RoundingMode`] — the five IEEE 754-2019 §4.3.3 rounding-direction
//!   attributes.
//! * [`IeeeClass`] — the IEEE 754-2019 §5.7.2 `class(x)` enum, with
//!   the ten standard classes a decimal floating-point datum can
//!   occupy.
//! * [`should_round_up`] — the rounding-decision predicate every
//!   sibling's `round_and_pack_finite` consumes. Pure function of
//!   `(RoundingMode, sign, last_kept_lsb, round_digit, sticky)`.
//! * [`decimal_digit_count_u128`] — the count of decimal digits in
//!   a `u128`, used by every sibling's alignment / scaling bounds.
//!
//! The siblings re-export these types verbatim, so callers writing
//! `ferrodec::Status` and `ferrodec_decimal32::Status` see the same
//! concrete type and can pass values between the two crates without
//! conversion.
//!
//! `no_std`, alloc-free, MSRV 1.84.

#![no_std]

mod classify;
mod digits;
mod round;
mod status;

pub use classify::IeeeClass;
pub use digits::decimal_digit_count_u128;
pub use round::should_round_up;
pub use status::{RoundingMode, Status};
