//! Shared correctly rounded Extended precision transcendental kernel for
//! the ferrodec decimal family.
//!
//! The exponential / logarithmic / trigonometric / hyperbolic / power
//! kernels are evaluated at 50 digit `Extended` working precision and
//! rounded once at the format boundary, giving correctly rounded results
//! across the IEEE 754-2019 §9.2 surface on all three formats (ADR-0032;
//! supersedes ADR-0024's faithful contract). The 50 digit working
//! precision clears the smallest empirical Arb worst case half ULP margin
//! (`4.167e-8` for `cosh` at `Decimal32` precision) by more than thirty
//! orders of magnitude on every format; per function margins and the
//! shared error model live in ADR-0032 §Decision. The corpus test
//! (`tests/transcend_vectors.rs` and the sibling mirrors) is the
//! standing empirical witness, with MPFR cross confirmation behind the
//! optional `mpfr-gate` feature (ADR-0026).
//!
//! The kernel is generic over a [`DecimalFormat`] seam so all three
//! siblings ([`ferrodec`] at Decimal128, `ferrodec-decimal64`,
//! `ferrodec-decimal32`) share one verified implementation rather than
//! a per precision copy. The seam has exactly three generic boundaries:
//! decoding a datum into the kernel ([`DecimalFormat::to_extended_parts`]),
//! rounding the kernel result back out
//! ([`DecimalFormat::round_and_pack_finite`]), and the Newton seed for
//! reciprocal / square root ([`DecimalFormat::recip_seed`] /
//! [`DecimalFormat::sqrt_seed`]). Everything between those boundaries is
//! precision agnostic fixed width integer math.
//!
//! `no_std`, alloc-free (the only `alloc` use is in test modules).
//!
//! [`ferrodec`]: https://crates.io/crates/ferrodec
//! [`tests/transcend_vectors.rs`]: https://github.com/ixmatus/ferrodec/tree/main/tests/transcend_vectors.rs

#![no_std]

// Tests read committed corpora (the S1 witness bands for the ladder
// budget audit) from the workspace tree; production stays no_std.
#[cfg(test)]
extern crate std;

// The unbounded rung allocates (its coefficients are heap-backed);
// nothing else in the crate does, so the dependency is feature-gated
// rather than crate-wide.
#[cfg(feature = "unbounded-ladder")]
extern crate alloc;

pub mod consts;
pub mod extended;
// Rung 2 of the ADR-0059 escalation ladder. Crate-internal until M8
// wires the ladder; the kernels reach it only through the `ExtNum`
// seam.
mod extended2;
// The unbounded rung of the same ladder (M8b): the working type whose
// precision is chosen at run time, behind the `unbounded-ladder`
// feature because it is the crate's only allocating code. The Ziv
// driver that walks it lands in M8b step 5.
#[cfg(feature = "unbounded-ladder")]
mod extended_dyn;
mod format;
mod ladder;
#[cfg(test)]
mod mock_format;

// Exact-result detection for cbrt / pow (§7.5 INEXACT contract). Gated on
// `exp-log` (cbrt); `pow` implies `exp-log`, so both consumers see it.
#[cfg(feature = "exp-log")]
mod exact;

#[cfg(feature = "trig")]
pub mod argred;
#[cfg(feature = "exp-log")]
pub mod cbrt;
#[cfg(feature = "exp-log")]
pub mod exp;
#[cfg(feature = "hyperbolic")]
pub mod hyperbolic;
#[cfg(feature = "trig")]
pub mod inverse_trig;
#[cfg(feature = "exp-log")]
pub mod ln;
#[cfg(feature = "pow")]
pub mod pow;
#[cfg(feature = "trig")]
pub mod sincos;

pub use format::DecimalFormat;
