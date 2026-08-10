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
#[cfg(feature = "exp-log")]
pub(crate) mod compound;
pub(crate) mod div;
pub(crate) mod divide_integer;
#[cfg(feature = "exp-log")]
pub(crate) mod exp;
pub(crate) mod fma;
#[cfg(feature = "hyperbolic")]
pub(crate) mod hyper;
#[cfg(feature = "exp-log")]
pub(crate) mod hypot;
pub(crate) mod integral;
pub(crate) mod logical;
pub(crate) mod mul;
#[cfg(feature = "pow")]
pub(crate) mod pow;
#[cfg(feature = "pow")]
pub(crate) mod powr;
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
// The IEEE 754-2019 §9.2 pi-scaled family (ADR-0061 Track D D4),
// under its own standalone feature: the exact decimal reduction needs
// none of `trig`'s Payne-Hanek machinery.
#[cfg(feature = "trig-pi")]
pub(crate) mod trig_pi;

#[allow(unused_imports)] // consumed by convert::parse and arithmetic ops in subsequent commits
pub(crate) use round::round_and_pack_finite;
