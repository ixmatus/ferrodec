//! Arbitrary-precision transcendental functions: `exp`, `ln`, `log10`, and
//! `power`, the four numerical transcendentals the General Decimal Arithmetic
//! specification defines (it has no trigonometric or hyperbolic functions, so
//! there is no pi and no Payne-Hanek argument reduction).
//!
//! Each function reduces its argument, evaluates a series in a private
//! variable-precision float ([`work::Work`]) at a working precision above the
//! context's, and rounds once through [`strategy::finish`], which re-runs the
//! kernel at a growing guard until the rounding is provably correct (bounded
//! Ziv) and falls back to a faithful rounding only at an astronomically
//! unlikely cap. The transcendental constants `ln 2` and `ln 10` are computed
//! on demand by an `atanh` series ([`consts`]); there is no stored table.
//!
//! The kernels are derived fresh from the `atanh`/Taylor identities and the
//! specification's operation definitions; see Muller, *Elementary Functions*,
//! for the range-reduction and error-budget framing. The fixed-width
//! `ferrodec-transcend` kernels are an algorithm-shape reference only: their
//! fixed 50-digit representation, stored constant table, and Payne-Hanek table
//! do not generalize to unbounded precision.

// The kernel surface (work / consts / strategy) is built in this slice and
// consumed by the exp / ln / log10 / power kernels in the following slices;
// this allow is removed once those land and exercise every path.
#![allow(dead_code)]

mod consts;
mod strategy;
mod work;
