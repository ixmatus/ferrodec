//! Multi-word integer primitives used inside arithmetic operations.
//!
//! `Decimal128`'s 113-bit binary coefficient already fits in a `u128`, so the
//! public API never exposes multi-word arithmetic. The need arises only in
//! the internal "intermediate" of an op, where two coefficients are aligned
//! (multiplied by `10^k`) before being combined and rounded back to 34
//! decimal digits.
//!
//! On 64-bit hosts, `rustc` lowers `u128` operations to native instructions,
//! so the intermediate uses `u128` directly. On 32-bit ARM (Cortex-M0+ floor)
//! the `u128` lowering becomes `__multi3`/`__udivti3` libcalls, which is the
//! reason the plan calls out hand-written `u32`-limb kernels in this module
//! later. For now we keep things readable and correct; the 32-bit fast path
//! is a follow-up profiled against a real M0+ board, not a premature
//! optimisation.

pub(crate) mod u256;
pub(crate) mod u384;
pub(crate) mod u512;

pub(crate) use u256::U256;
pub(crate) use u384::U384;
pub(crate) use u512::U512;
