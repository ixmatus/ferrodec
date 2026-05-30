//! NaN payload propagation helpers for the arithmetic kernels.
//!
//! IEEE 754-2019 §6.2.3 specifies that a NaN result produced by an
//! operation should carry the payload of one of the input NaN
//! operands. The standard allows implementations to choose which one;
//! ferrodec uses **signaling-NaN priority, then operand order**: a
//! signaling operand's payload wins over a quiet one's, and ties within
//! a class break by position (a, then b, then c for FMA). This matches
//! the `ferrodec-decimal64` / `ferrodec-decimal32` siblings and the
//! decNumber `dq*` / `dd*` / `ds*` corpus, so the three formats agree.
//!
//! Signaling NaN inputs raise `INVALID` (handled at the call sites)
//! and convert to a quiet NaN result with the same payload.

use crate::bid::{pack_quiet_nan, T_MASK};
use crate::decimal::Decimal128;

/// Build a quiet NaN result from a single NaN operand, preserving its
/// sign and 110-bit payload.
#[inline]
pub(crate) fn nan_from(src: Decimal128) -> Decimal128 {
    debug_assert!(src.is_nan());
    let bits = src.to_bits();
    let sign = (bits >> 127) & 1 == 1;
    let payload = bits & T_MASK;
    Decimal128::from_bits(pack_quiet_nan(sign, payload))
}

/// Build a quiet NaN result for a binary op given that at least one
/// of the two operands is NaN. A signaling operand's payload takes
/// precedence over a quiet one's; ties break by operand order.
#[inline]
pub(crate) fn propagate_nan2(a: Decimal128, b: Decimal128) -> Decimal128 {
    if a.is_signaling_nan() {
        nan_from(a)
    } else if b.is_signaling_nan() {
        nan_from(b)
    } else if a.is_nan() {
        nan_from(a)
    } else {
        debug_assert!(b.is_nan());
        nan_from(b)
    }
}

/// Build a quiet NaN result for a ternary op (FMA) given at least one
/// NaN operand. Signaling-NaN priority, then operand order (a, b, c).
#[inline]
pub(crate) fn propagate_nan3(a: Decimal128, b: Decimal128, c: Decimal128) -> Decimal128 {
    if a.is_signaling_nan() {
        nan_from(a)
    } else if b.is_signaling_nan() {
        nan_from(b)
    } else if c.is_signaling_nan() {
        nan_from(c)
    } else if a.is_nan() {
        nan_from(a)
    } else if b.is_nan() {
        nan_from(b)
    } else {
        debug_assert!(c.is_nan());
        nan_from(c)
    }
}
