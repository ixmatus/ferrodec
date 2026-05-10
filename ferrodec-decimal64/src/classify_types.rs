//! IEEE 754-2019 classification enum.
//!
//! Adapted from `ferrodec/src/classify.rs`. The enum definition is the
//! same shape as ferrodec's; only the doc text is retargeted from
//! Decimal128 to Decimal64. Extraction to a shared `ferrodec-ieee`
//! crate is deferred until three concrete consumers exist (ferrodec,
//! ferrodec-decimal32, ferrodec-decimal64), per the plan archived at
//! `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`
//! in the workspace root.

/// IEEE 754-2019 §5.7.2 `class(x)` enum, exposing all ten standard
/// classes a decimal floating-point datum can occupy.
///
/// Each value of [`Decimal64`](crate::Decimal64) belongs to exactly one
/// variant. Use `Decimal64::ieee_class` (added in a subsequent commit)
/// to obtain it. The standard's class operation is required to be
/// quiet — calling it on a signaling NaN does *not* raise
/// `Status::INVALID`.
///
/// NaN classes do not carry sign by IEEE convention: a sign bit set on
/// a NaN is observable through `Decimal64::is_sign_negative` but does
/// not split [`IeeeClass::QuietNaN`] or [`IeeeClass::SignalingNaN`]
/// into signed variants.
///
/// For a coarser classification matching `f32` / `f64`, use
/// `Decimal64::classify`, which returns [`core::num::FpCategory`] (five
/// variants).
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
    /// Negative finite value with magnitude at or above
    /// `Decimal64::MIN_POSITIVE_NORMAL` (`10⁻³⁸³`).
    NegativeNormal,
    /// Negative finite value with magnitude strictly below
    /// `Decimal64::MIN_POSITIVE_NORMAL` but strictly above zero.
    NegativeSubnormal,
    /// `−0`. Distinct from [`IeeeClass::PositiveZero`] under
    /// `Decimal64::total_cmp` but equal under
    /// `Decimal64::partial_cmp`.
    NegativeZero,
    /// `+0`. See [`IeeeClass::NegativeZero`] for the comparison
    /// semantics.
    PositiveZero,
    /// Positive finite value strictly below
    /// `Decimal64::MIN_POSITIVE_NORMAL` and strictly above zero.
    PositiveSubnormal,
    /// Positive finite value at or above
    /// `Decimal64::MIN_POSITIVE_NORMAL`.
    PositiveNormal,
    /// `+∞`.
    PositiveInfinity,
}
