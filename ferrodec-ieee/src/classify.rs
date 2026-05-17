//! IEEE 754-2019 classification enum, shared across decimal precisions.

/// IEEE 754-2019 §5.7.2 `class(x)` enum, exposing all ten standard
/// classes a decimal floating-point datum can occupy.
///
/// Every `Decimal32` / `Decimal64` / `Decimal128` value belongs to
/// exactly one variant. Each sibling crate exposes an `ieee_class`
/// method on its precision-specific type that returns this enum.
/// The standard's class operation is required to be quiet — calling
/// it on a signaling NaN does *not* raise `Status::INVALID`.
///
/// NaN classes do not carry sign by IEEE convention: a sign bit set
/// on a NaN is observable through the precision-specific
/// `is_sign_negative` predicate but does not split [`IeeeClass::QuietNaN`]
/// or [`IeeeClass::SignalingNaN`] into signed variants.
///
/// For a coarser classification matching `f32` / `f64`, each sibling
/// also exposes a `classify` method returning [`core::num::FpCategory`]
/// (five variants).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IeeeClass {
    /// Signaling NaN. Most operations consume this and raise
    /// `Status::INVALID`; the class operation itself is quiet (per
    /// IEEE 754-2019 §5.7.2).
    SignalingNaN,
    /// Quiet NaN. Propagates through arithmetic without raising
    /// `Status::INVALID` (a property called *quiet* propagation).
    QuietNaN,
    /// `−∞`.
    NegativeInfinity,
    /// Negative finite value with magnitude at or above the format's
    /// minimum positive normal magnitude.
    NegativeNormal,
    /// Negative finite value with magnitude strictly below the
    /// format's minimum positive normal magnitude but strictly above
    /// zero.
    NegativeSubnormal,
    /// `−0`. Distinct from [`IeeeClass::PositiveZero`] under the
    /// total-order comparison but equal under partial comparison.
    NegativeZero,
    /// `+0`. See [`IeeeClass::NegativeZero`] for the comparison
    /// semantics.
    PositiveZero,
    /// Positive finite value strictly below the format's minimum
    /// positive normal magnitude and strictly above zero.
    PositiveSubnormal,
    /// Positive finite value at or above the format's minimum
    /// positive normal magnitude.
    PositiveNormal,
    /// `+∞`.
    PositiveInfinity,
}

/// Decoded form of a BID-encoded decimal datum's bit pattern.
///
/// This is the *decode* result a sibling's `classify_bits` produces
/// from a raw interchange pattern: the canonical sign / biased
/// exponent / coefficient (or NaN payload) recovered from the bits,
/// after the IEEE 754-2019 §3.5.2 non-canonical-coefficient
/// canonicalisation. It is distinct from [`IeeeClass`], which is the
/// §5.7.2 ten-class *observation* of a value; this enum carries the
/// reconstructed numeric components the arithmetic and transcendental
/// kernels actually consume.
///
/// The shape is precision-agnostic: a `u128` coefficient / payload
/// covers Decimal32, Decimal64 and Decimal128, and `biased_exp: u32`
/// covers every sibling's exponent envelope. Only the bit-layout
/// `classify_bits` that produces this enum is precision-specific.
///
/// The decode never fails: every input bit pattern maps to exactly
/// one variant.
///
/// Provenance: lifted verbatim (variant shape and field semantics)
/// from `ferrodec`'s in-crate `bid::Class`, which now aliases this
/// type so the shared kernel and all three siblings see one
/// definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IeeeDecodedClass {
    /// Finite, non-zero value with `coefficient ∈ [1, 10^p − 1]`.
    Finite {
        sign: bool,
        biased_exp: u32,
        coefficient: u128,
    },
    /// Numerical zero. `biased_exp` is preserved so `total_cmp` can
    /// distinguish cohorts (`+0E+0`, `+0E+1`, …).
    Zero { sign: bool, biased_exp: u32 },
    /// ±Infinity.
    Infinity { sign: bool },
    /// Quiet NaN with the given trailing-significand payload.
    QuietNaN { sign: bool, payload: u128 },
    /// Signaling NaN with the given trailing-significand payload.
    SignalingNaN { sign: bool, payload: u128 },
}
