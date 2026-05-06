//! Kani harnesses for `is_canonical` / `canonicalize` (IEEE 754-2019
//! §5.7.2 / §5.4.2).
//!
//! Both routines are pure bit-manipulation — they decode the BID type
//! field, mask out unused bits, and re-pack. There are no symbolic
//! multiplications or rounding loops, so SMT can dispatch the entire
//! `u128` domain quickly.

use crate::Decimal128;

/// Calling `canonicalize` twice yields the same bits as calling it
/// once. This is a projection property: `canonicalize` maps the full
/// `u128` space onto the canonical-encoding subset, and applying the
/// projection a second time is a no-op.
#[kani::proof]
fn canonicalize_is_idempotent() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    let once = d.canonicalize();
    let twice = once.canonicalize();
    assert!(once.to_bits() == twice.to_bits());
}

/// `canonicalize(x)` is always canonical. Combined with idempotence,
/// this pins the function as a projection onto the canonical-encoding
/// subset of `u128`.
#[kani::proof]
fn canonicalize_yields_canonical() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    assert!(d.canonicalize().is_canonical());
}

/// `is_canonical(x)` ⇔ `canonicalize(x).to_bits() == x.to_bits()`.
/// The defining contract of the predicate: a bit pattern is canonical
/// exactly when it's a fixed point of `canonicalize`.
#[kani::proof]
fn is_canonical_iff_canonicalize_fixpoint() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    let same = d.canonicalize().to_bits() == d.to_bits();
    assert!(d.is_canonical() == same);
}
