//! Shared faithful Extended-precision transcendental kernel for the
//! ferrodec decimal family.
//!
//! The exponential / logarithmic / trigonometric / hyperbolic / power
//! kernels are evaluated at 50-digit `Extended` working precision and
//! rounded once at the format boundary, giving faithfully-rounded
//! (≤ 1 ULP) results without a lossy `f64` detour. The kernel is
//! generic over a [`DecimalFormat`] seam so all three siblings
//! ([`ferrodec`] at Decimal128, `ferrodec-decimal64`,
//! `ferrodec-decimal32`) share one verified implementation rather
//! than a per-precision copy.
//!
//! The seam has exactly three generic boundaries: decoding a datum
//! into the kernel ([`DecimalFormat::to_extended_parts`]), rounding
//! the kernel result back out ([`DecimalFormat::round_and_pack_finite`]),
//! and the Newton seed for reciprocal / square-root
//! ([`DecimalFormat::recip_seed`] / [`DecimalFormat::sqrt_seed`]).
//! Everything between those boundaries is precision-agnostic
//! fixed-width integer math.
//!
//! The [`Extended`](extended::Extended) intermediate is now present
//! (genericized over [`DecimalFormat`]); the kernel bodies land in
//! subsequent commits of the extraction.
//!
//! `no_std`, alloc-free (the only `alloc` use is in test modules).
//!
//! [`ferrodec`]: https://crates.io/crates/ferrodec

#![no_std]

pub mod consts;
pub mod extended;
mod format;

#[cfg(feature = "trig")]
pub mod argred;
#[cfg(feature = "exp-log")]
pub mod cbrt;
#[cfg(feature = "exp-log")]
pub mod exp;
#[cfg(feature = "trig")]
pub mod inverse_trig;
#[cfg(feature = "exp-log")]
pub mod ln;
#[cfg(feature = "trig")]
pub mod sincos;

pub use format::DecimalFormat;
