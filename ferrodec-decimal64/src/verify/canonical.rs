//! Kani harnesses for `is_canonical` / `canonicalize` (IEEE 754-2019
//! §5.7.2 / §5.4.2). Port of `ferrodec/src/verify/canonical.rs` to the
//! 64-bit format.
//!
//! Both routines are pure bit-manipulation — they decode the BID type
//! field, mask out unused bits, and re-pack. There are no symbolic
//! multiplications or rounding loops, so SMT can dispatch the entire
//! `u64` domain quickly.

use crate::Decimal64;

/// Calling `canonicalize` twice yields the same bits as calling it
/// once: it is a projection onto the canonical-encoding subset.
#[kani::proof]
fn canonicalize_is_idempotent() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    let once = d.canonicalize();
    let twice = once.canonicalize();
    assert!(once.to_bits() == twice.to_bits());
}

/// `canonicalize(x)` is always canonical.
#[kani::proof]
fn canonicalize_yields_canonical() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(d.canonicalize().is_canonical());
}

/// `is_canonical(x)` ⇔ `canonicalize(x)` is a fixed point: a bit
/// pattern is canonical exactly when `canonicalize` leaves it alone.
#[kani::proof]
fn is_canonical_iff_canonicalize_fixpoint() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    let same = d.canonicalize().to_bits() == d.to_bits();
    assert!(d.is_canonical() == same);
}
