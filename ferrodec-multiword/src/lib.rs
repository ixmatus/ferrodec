//! Fixed-width unsigned integer primitives for the ferrodec decimal
//! family.
//!
//! A decimal format's binary coefficient fits in a `u128` (Decimal128's
//! is 113-bit), so the public decimal API never exposes multi-word
//! arithmetic. The need arises only in the internal "intermediate" of
//! an op, where two coefficients are aligned (multiplied by `10^k`)
//! before being combined and rounded back to the format's digit width,
//! and in the Payne-Hanek argument reduction for the trigonometric
//! kernels.
//!
//! On 64-bit hosts, `rustc` lowers `u128` operations to native
//! instructions, so the intermediate uses `u128` directly. On 32-bit
//! ARM (Cortex-M0+ floor) the `u128` lowering becomes
//! `__multi3`/`__udivti3` libcalls, which is the reason the plan calls
//! out hand-written `u32`-limb kernels here later. For now the kernels
//! stay readable and correct; the 32-bit fast path is a follow-up
//! profiled against a real M0+ board, not a premature optimisation.
//!
//! `no_std`. The default build is `alloc`-free: the fixed-width
//! `U256`/`U384`/`U512`/`U768`/`U1024` types are pure stack integer
//! math for the Cortex-M0+ floor (`U1024` exists for the ADR-0060
//! exact integer adjudicator's widest comparisons; its module doc
//! carries the width derivation). The optional `alloc` feature additionally compiles
//! in [`DecBig`], a growable base-`10^9` decimal-limb unsigned integer
//! used as the coefficient backend for arbitrary-precision decimal; it
//! is the only part of the crate that touches the heap. `DecBig`
//! multiplies by schoolbook below a limb threshold and Karatsuba above
//! it, and divides by Knuth Algorithm D (TAOCP Vol 2 §4.3.1) at radix
//! `10^9`; see the [`decbig`] module documentation for the
//! representation invariant, the algorithm provenance, and the
//! performance rationale (ADR-0043). The [`bigconst`] module rides on
//! the same feature: it computes π, 2/π, ln 2, ln 10, e, tan(π/8),
//! 1/ln 2, and 1/ln 10 to any requested depth, each with a derived
//! error bound, for the arbitrary precision rung of the transcendental
//! ladder.
//!
//! This is a support crate for the ferrodec decimal family: the surface
//! is shaped for the family's needs rather than as a general-purpose
//! bignum library, though it is published so the public formats can
//! depend on it.

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod u1024;
pub mod u256;
pub mod u384;
pub mod u512;
pub mod u768;

pub use u1024::U1024;
pub use u256::U256;
pub use u384::U384;
pub use u512::U512;
pub use u768::U768;

#[cfg(feature = "alloc")]
pub mod decbig;

#[cfg(feature = "alloc")]
pub use decbig::DecBig;

#[cfg(feature = "alloc")]
pub mod bigconst;
