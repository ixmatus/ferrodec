//! Calculator transcendentals.
//!
//! Each sub-feature gates one cluster of the surface:
//!
//! * `trig` — `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`.
//!   Pulls the Payne-Hanek 6 300-digit `2/π` table in [`argred`].
//! * `exp-log` — `exp`, `exp2`, `ln`, `log2`, `log10`, `cbrt`,
//!   `rootn`, `rsqrt`, `compound`, `hypot`.
//! * `hyperbolic` — `sinh`, `cosh`, `tanh`, `asinh`, `acosh`,
//!   `atanh`. Implies `exp-log` (kernels delegate to `exp` / `ln`).
//! * `pow` — `pow`, `powi`, `powr`. Implies `exp-log`
//!   (`pow(x, y) = exp(y · ln x)`).
//! * `transcendentals` — meta-feature for "all of the above"; what
//!   pre-1.2 dependents asked for.
//!
//! ## Accuracy
//!
//! Correctly rounded across the supported domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). Every kernel runs its
//! inner Taylor / Newton / argument-reduction loop at [`Extended`]
//! (50-digit U256-coefficient) precision and rounds once at the
//! [`Decimal128`] boundary; the 50-digit working precision exceeds
//! the smallest empirical Arb worst-case half-ULP margin (`4.167e-8`
//! for `cosh` at Decimal32, the binding case across the family) by
//! more than thirty orders of magnitude on every format. The
//! astro-float oracle continues to gate the strictly weaker faithful
//! (≤ 1 ULP at 34 digits) invariant as a hard-defect catcher (see
//! `docs/testing.md`). `sin` / `cos` accuracy is uniform across the
//! full Decimal128 magnitude range (Payne-Hanek with the wider U512
//! window picks up boundary cases within ~33 digits of a multiple of
//! π/2).
//!
//! ## Implementation
//!
//! The kernel bodies live in the shared [`ferrodec-transcend`] crate,
//! generic over a `DecimalFormat` seam, so all three decimal siblings
//! reuse one verified implementation rather than a per-precision copy
//! (extracted in P0a.2). The modules here are thin: `format_impl`
//! supplies `impl DecimalFormat for Decimal128`, and each kernel
//! module is a delegating shim that re-exposes the public
//! `Decimal128` method at `F = Decimal128` and retains that method's
//! behaviour tests as the byte-identical regression gate.
//!
//! [`Decimal128`]: crate::Decimal128
//! [`Extended`]: ferrodec_transcend::extended::Extended
//! [`ferrodec-transcend`]: ferrodec_transcend

mod consts;
mod extended;
mod format_impl;

#[cfg(feature = "exp-log")]
mod cbrt;
#[cfg(feature = "exp-log")]
mod compound;
#[cfg(feature = "exp-log")]
mod exp;
#[cfg(feature = "exp-log")]
mod hypot;
#[cfg(feature = "exp-log")]
mod ln;
#[cfg(feature = "exp-log")]
mod rootn;
#[cfg(feature = "exp-log")]
mod rsqrt;

#[cfg(feature = "trig")]
mod argred;
#[cfg(feature = "trig")]
mod inverse_trig;
#[cfg(feature = "trig")]
mod sincos;

#[cfg(feature = "hyperbolic")]
mod hyperbolic;

#[cfg(feature = "pow")]
mod pow;
// IEEE 754-2019 §9.2 `powr` (ADR-0059 Track D D3): the same kernel as
// `pow` under the §9.2.1 `powr` special-value table.
#[cfg(feature = "pow")]
mod powr;

pub use consts::{e, ln10, ln2, pi};
