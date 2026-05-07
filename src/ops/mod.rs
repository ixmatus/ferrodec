//! IEEE 754 arithmetic operations on [`Decimal128`].
//!
//! Each op is exposed as an inherent method that returns
//! `(Decimal128, Status)` — see the module-level discussion in
//! [`crate::status`] for why we don't carry a global flag word.

mod addsub;
mod div;
mod fma;
mod integral;
mod mul;
pub(crate) mod nan_propagate;
mod quantum;
mod rem;
pub(crate) mod round;
mod sqrt;

pub(crate) use nan_propagate::{nan_from, propagate_nan2, propagate_nan3};
pub(crate) use round::round_and_pack_finite;
