//! Arithmetic and rounding kernels.
//!
//! The rounding entry point [`round_and_pack_finite`] is the single
//! source of truth for "given a coefficient, exponent, sign, and
//! sticky bit, produce a canonical [`Decimal32`] honouring the active
//! rounding mode and IEEE 754 status flags." Both `parse_str` and the
//! arithmetic ops route through it.
//!
//! [`Decimal32`]: crate::Decimal32

pub(crate) mod addsub;
pub(crate) mod mul;
pub(crate) mod round;

#[allow(unused_imports)] // consumed by convert::parse and arithmetic ops in subsequent commits
pub(crate) use round::round_and_pack_finite;
