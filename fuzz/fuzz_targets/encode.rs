//! Fuzz target: BID encoding invariants on arbitrary bit patterns.
//!
//! Kani harnesses (`src/verify/canonical.rs`) prove these properties
//! over the full `u128` domain via SMT; this target's value is
//! catching anything the SMT model abstracted away — model-checking
//! sees bounded shifts and pow10 ranges but doesn't actually execute
//! the bit-twiddling on real CPU dispatch.
//!
//! Invariants asserted:
//!
//! 1. **`is_canonical` ↔ `canonicalize` fixed-point**: `is_canonical(d)`
//!    holds iff `canonicalize(d).to_bits() == d.to_bits()`.
//! 2. **Canonicalize idempotence**: `canonicalize(canonicalize(d))`
//!    bit-equals `canonicalize(d)`.
//! 3. **Canonicalize lands canonical**: `is_canonical(canonicalize(d))`
//!    is always `true`.
//! 4. **Classification stable across canonicalize**: `classify(d)`
//!    equals `classify(canonicalize(d))` and the sign predicates agree.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec::Decimal128;

fuzz_target!(|bits: u128| {
    let d = Decimal128::from_bits(bits);
    let c = d.canonicalize();
    let cc = c.canonicalize();

    // 1. is_canonical ↔ canonicalize-fixed-point.
    let canon_eq = c.to_bits() == d.to_bits();
    assert_eq!(
        d.is_canonical(),
        canon_eq,
        "is_canonical disagrees with canonicalize fixed-point: bits {bits:#034x}, \
         canonicalized bits {:#034x}",
        c.to_bits()
    );

    // 2. Idempotence: canonicalize is a projection.
    assert_eq!(
        c.to_bits(),
        cc.to_bits(),
        "canonicalize not idempotent: bits {bits:#034x}, c bits {:#034x}, cc bits {:#034x}",
        c.to_bits(),
        cc.to_bits()
    );

    // 3. Canonicalize lands canonical.
    assert!(
        c.is_canonical(),
        "canonicalize produced a non-canonical value: bits {bits:#034x}, \
         canonicalized bits {:#034x}",
        c.to_bits()
    );

    // 4. Classification agrees across canonicalize.
    assert_eq!(
        d.classify(),
        c.classify(),
        "classify disagrees across canonicalize: bits {bits:#034x}, c bits {:#034x}",
        c.to_bits()
    );
    assert_eq!(
        d.is_sign_negative(),
        c.is_sign_negative(),
        "sign disagrees across canonicalize: bits {bits:#034x}, c bits {:#034x}",
        c.to_bits()
    );
    assert_eq!(
        d.is_nan(),
        c.is_nan(),
        "is_nan disagrees across canonicalize: bits {bits:#034x}, c bits {:#034x}",
        c.to_bits()
    );
    assert_eq!(
        d.is_infinite(),
        c.is_infinite(),
        "is_infinite disagrees across canonicalize: bits {bits:#034x}, c bits {:#034x}",
        c.to_bits()
    );
});
