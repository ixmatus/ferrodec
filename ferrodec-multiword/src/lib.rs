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
//! `U256`/`U384`/`U512` types are pure stack integer math for the
//! Cortex-M0+ floor. The optional `alloc` feature additionally compiles
//! in [`DecBig`], a growable base-`10^9` decimal-limb unsigned integer
//! used as the coefficient backend for arbitrary-precision decimal; it
//! is the only part of the crate that touches the heap.

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod u256;
pub mod u384;
pub mod u512;

pub use u256::U256;
pub use u384::U384;
pub use u512::U512;

#[cfg(feature = "alloc")]
pub mod decbig;

#[cfg(feature = "alloc")]
pub use decbig::DecBig;
