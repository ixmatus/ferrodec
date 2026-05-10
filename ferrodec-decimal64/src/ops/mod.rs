//! Arithmetic and rounding kernels.
//!
//! The rounding entry point [`round_and_pack_finite`] is the single
//! source of truth for "given a coefficient, exponent, sign, and
//! sticky bit, produce a canonical [`Decimal64`] honouring the active
//! rounding mode and IEEE 754 status flags." Both `parse_str` and the
//! arithmetic ops route through it.
//!
//! [`Decimal64`]: crate::Decimal64

pub(crate) mod addsub;
pub(crate) mod div;
#[cfg(feature = "exp-log")]
pub(crate) mod exp;
pub(crate) mod fma;
#[cfg(feature = "hyperbolic")]
pub(crate) mod hyper;
pub(crate) mod mul;
#[cfg(feature = "pow")]
pub(crate) mod pow;
pub(crate) mod quantum;
pub(crate) mod rem;
pub(crate) mod round;
pub(crate) mod sqrt;
#[cfg(feature = "trig")]
pub(crate) mod trig;

#[allow(unused_imports)] // consumed by convert::parse and arithmetic ops in subsequent commits
pub(crate) use round::round_and_pack_finite;
