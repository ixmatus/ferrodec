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
pub(crate) mod div;
pub(crate) mod divide_integer;
#[cfg(feature = "exp-log")]
pub(crate) mod exp;
pub(crate) mod fma;
#[cfg(feature = "hyperbolic")]
pub(crate) mod hyper;
pub(crate) mod integral;
pub(crate) mod logical;
pub(crate) mod mul;
#[cfg(feature = "pow")]
pub(crate) mod pow;
pub(crate) mod quantum;
pub(crate) mod reduce;
pub(crate) mod rem;
#[cfg(feature = "exp-log")]
pub(crate) mod rootn;
pub(crate) mod rotate;
pub(crate) mod round;
#[cfg(feature = "exp-log")]
pub(crate) mod rsqrt;
pub(crate) mod shift;
pub(crate) mod sqrt;
#[cfg(feature = "trig")]
pub(crate) mod trig;

#[allow(unused_imports)] // consumed by convert::parse and arithmetic ops in subsequent commits
pub(crate) use round::round_and_pack_finite;
