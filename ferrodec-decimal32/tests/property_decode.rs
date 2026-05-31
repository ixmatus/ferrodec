//! Property tests for `Decimal32::decode` over the full 32-bit input space.
//!
//! `decode` and the `(sign, coefficient, exponent)` constructors are exact
//! inverses on canonical finite values, and `decode` is total over every
//! bit pattern: `Some` exactly for finite values, with bounded components.

use ferrodec_decimal32::{Decimal32, Decimal32Parts};
use proptest::prelude::*;

proptest! {
    /// `decode` then reconstruct is the identity on canonical finite values.
    /// The operand is built canonically from a `(sign, coefficient, exponent)`
    /// triple, so decode and the constructor are exact inverses bit for bit.
    /// Reconstruction goes through `try_new_unsigned` + `neg` (not the signed
    /// `try_new`) so the sign of zero round-trips.
    #[test]
    fn decode_reconstruct_roundtrip_canonical(
        negative in any::<bool>(),
        coefficient in 0u32..10u32.pow(7),
        exponent in -101i32..=90,
    ) {
        let base = Decimal32::try_new_unsigned(coefficient, exponent).unwrap();
        let original = if negative { base.neg() } else { base };

        let p = original.decode().unwrap();
        prop_assert_eq!(
            p,
            Decimal32Parts { negative, coefficient, exponent: exponent as i16 }
        );

        let r = Decimal32::try_new_unsigned(p.coefficient, p.exponent as i32).unwrap();
        let r = if p.negative { r.neg() } else { r };
        prop_assert_eq!(r.to_bits(), original.to_bits());
    }

    /// `decode` is total over the full bit space: `Some` exactly for finite
    /// values, with the coefficient and exponent inside their documented
    /// bounds; `None` otherwise. For canonical finite inputs the reconstruction
    /// is additionally bit-equal (non-canonical inputs decode to zero and are
    /// only numerically equal to the junk bits, so the bit check is gated on
    /// `is_canonical`).
    #[test]
    fn decode_total_over_bits(bits in any::<u32>()) {
        let d = Decimal32::from_bits(bits);
        match d.decode() {
            Some(p) => {
                prop_assert!(d.is_finite());
                prop_assert!(p.coefficient < 10u32.pow(7));
                prop_assert!((-101..=90).contains(&p.exponent));
                if d.is_canonical() {
                    let r = Decimal32::try_new_unsigned(p.coefficient, p.exponent as i32).unwrap();
                    let r = if p.negative { r.neg() } else { r };
                    prop_assert_eq!(r.to_bits(), d.to_bits());
                }
            }
            None => prop_assert!(!d.is_finite()),
        }
    }
}
