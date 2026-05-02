//! IEEE 754 arithmetic operations on [`Decimal128`].
//!
//! Each op is exposed as an inherent method that returns
//! `(Decimal128, Status)` — see the module-level discussion in
//! [`crate::status`] for why we don't carry a global flag word.

mod addsub;
mod mul;
mod round;

pub(crate) use round::round_and_pack_finite;
