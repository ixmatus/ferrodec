//! Calculator transcendentals.
//!
//! Each sub-feature gates one cluster of the surface:
//!
//! * `trig` — `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`.
//!   Pulls the Payne-Hanek 6 300-digit `2/π` table in [`argred`].
//! * `exp-log` — `exp`, `exp2`, `ln`, `log2`, `log10`, `cbrt`.
//! * `hyperbolic` — `sinh`, `cosh`, `tanh`, `asinh`, `acosh`,
//!   `atanh`. Implies `exp-log` (kernels delegate to `exp` / `ln`).
//! * `pow` — `pow`. Implies `exp-log` (`pow(x, y) = exp(y · ln x)`).
//! * `transcendentals` — meta-feature for "all of the above"; what
//!   pre-1.2 dependents asked for.
//!
//! ## Accuracy
//!
//! Faithfully rounded (≤ 1 ULP at 34 digits) against `astro-float`
//! across the supported domain. Every kernel runs its inner Taylor
//! / Newton / argument-reduction loop at [`Extended`] (50-digit U256-
//! coefficient) precision and rounds once at the [`Decimal128`]
//! boundary. `sin` / `cos` accuracy is uniform across the full
//! Decimal128 magnitude range (Payne-Hanek with the wider U512
//! window picks up boundary cases within ~33 digits of a multiple of
//! π/2).
//!
//! [`Decimal128`]: crate::Decimal128
//! [`Extended`]: extended::Extended

mod consts;
pub(crate) mod extended;

#[cfg(feature = "exp-log")]
mod cbrt;
#[cfg(feature = "exp-log")]
pub(crate) mod exp;
#[cfg(feature = "exp-log")]
pub(crate) mod ln;

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

pub use consts::{e, ln10, ln2, pi};
