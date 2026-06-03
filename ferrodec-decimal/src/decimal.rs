//! The arbitrary-precision decimal value type.

use ferrodec_multiword::DecBig;

/// Internal representation of a decimal value.
///
/// A finite value is `(-1)^sign * coefficient * 10^exponent`. A zero is finite
/// with a zero coefficient; it still carries an exponent (its quantum) and a
/// sign, so negative zero and the full cohort of zeros are representable. The
/// coefficient is held as an integer, so its trailing decimal zeros are
/// significant: `1.230` is coefficient `1230` with exponent `-3` and four
/// digits, a distinct cohort member from `1.23`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Repr {
    Finite {
        sign: bool,
        coeff: DecBig,
        exp: i32,
    },
    Infinity {
        sign: bool,
    },
    Nan {
        sign: bool,
        signaling: bool,
        payload: DecBig,
    },
}

/// An arbitrary-precision decimal number to the General Decimal Arithmetic
/// specification: a finite value (sign, integer coefficient, exponent), a
/// signed infinity, or a quiet or signaling NaN with an optional diagnostic
/// payload.
///
/// `PartialEq` is *representation* equality: it is exact and cohort sensitive,
/// so `1.0` and `1.00` are unequal, and a NaN equals an identically shaped
/// NaN. This is deliberately distinct from the General Decimal Arithmetic
/// numeric `compare`, which is a separate operation (added in a later slice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    repr: Repr,
}

impl Decimal {
    // -- constructors --

    /// Positive zero with exponent zero.
    #[must_use]
    pub fn zero() -> Self {
        Self::finite(false, DecBig::zero(), 0)
    }

    /// A finite value `(-1)^sign * coefficient * 10^exponent`.
    #[must_use]
    pub fn finite(sign: bool, coefficient: DecBig, exponent: i32) -> Self {
        Self {
            repr: Repr::Finite {
                sign,
                coeff: coefficient,
                exp: exponent,
            },
        }
    }

    /// Signed infinity.
    #[must_use]
    pub fn infinity(sign: bool) -> Self {
        Self {
            repr: Repr::Infinity { sign },
        }
    }

    /// A quiet NaN with a diagnostic payload (`DecBig::zero()` for none).
    #[must_use]
    pub fn quiet_nan(sign: bool, payload: DecBig) -> Self {
        Self {
            repr: Repr::Nan {
                sign,
                signaling: false,
                payload,
            },
        }
    }

    /// A signaling NaN with a diagnostic payload (`DecBig::zero()` for none).
    #[must_use]
    pub fn signaling_nan(sign: bool, payload: DecBig) -> Self {
        Self {
            repr: Repr::Nan {
                sign,
                signaling: true,
                payload,
            },
        }
    }

    // -- integer constructors --

    /// The exact value of an unsigned 64-bit integer, at exponent zero.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self::finite(false, DecBig::from_u64(value), 0)
    }

    /// The exact value of an unsigned 128-bit integer, at exponent zero.
    #[must_use]
    pub fn from_u128(value: u128) -> Self {
        Self::finite(false, DecBig::from_u128(value), 0)
    }

    /// The exact value of a signed 64-bit integer, at exponent zero.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self::finite(value < 0, DecBig::from_u64(value.unsigned_abs()), 0)
    }

    /// The exact value of a signed 128-bit integer, at exponent zero.
    #[must_use]
    pub fn from_i128(value: i128) -> Self {
        Self::finite(value < 0, DecBig::from_u128(value.unsigned_abs()), 0)
    }

    // -- classification --

    /// True for a finite value (including any zero).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        matches!(self.repr, Repr::Finite { .. })
    }

    /// True for either signed infinity.
    #[must_use]
    pub fn is_infinite(&self) -> bool {
        matches!(self.repr, Repr::Infinity { .. })
    }

    /// True for a quiet or signaling NaN.
    #[must_use]
    pub fn is_nan(&self) -> bool {
        matches!(self.repr, Repr::Nan { .. })
    }

    /// True for a signaling NaN only.
    #[must_use]
    pub fn is_signaling_nan(&self) -> bool {
        matches!(
            self.repr,
            Repr::Nan {
                signaling: true,
                ..
            }
        )
    }

    /// True for a finite value whose coefficient is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        matches!(&self.repr, Repr::Finite { coeff, .. } if coeff.is_zero())
    }

    /// The sign bit, which every value carries (including negative zero,
    /// negative infinity, and a signed NaN).
    #[must_use]
    pub fn is_negative(&self) -> bool {
        match &self.repr {
            Repr::Finite { sign, .. } | Repr::Infinity { sign } | Repr::Nan { sign, .. } => *sign,
        }
    }

    /// Number of significant digits in the coefficient of a finite value
    /// (`1` for a zero, by the specification's convention); `None` for a
    /// special value.
    #[must_use]
    pub fn digits(&self) -> Option<u64> {
        match &self.repr {
            Repr::Finite { coeff, .. } => Some(coeff.decimal_digit_count()),
            _ => None,
        }
    }

    // -- decode accessors --

    /// The `(sign, coefficient, exponent)` of a finite value, else `None`.
    /// The decode counterpart to [`Decimal::finite`].
    #[must_use]
    pub fn finite_parts(&self) -> Option<(bool, &DecBig, i32)> {
        match &self.repr {
            Repr::Finite { sign, coeff, exp } => Some((*sign, coeff, *exp)),
            _ => None,
        }
    }

    /// A copy of this value with its sign replaced, preserving everything else
    /// (coefficient and exponent, or NaN payload and signaling flag). Used by
    /// the copy operations, which manipulate only the sign bit.
    #[must_use]
    pub(crate) fn with_sign(&self, sign: bool) -> Decimal {
        match &self.repr {
            Repr::Finite { coeff, exp, .. } => Decimal::finite(sign, coeff.clone(), *exp),
            Repr::Infinity { .. } => Decimal::infinity(sign),
            Repr::Nan {
                signaling, payload, ..
            } => {
                if *signaling {
                    Decimal::signaling_nan(sign, payload.clone())
                } else {
                    Decimal::quiet_nan(sign, payload.clone())
                }
            }
        }
    }

    /// The `(sign, signaling, payload)` of a NaN, else `None`.
    #[must_use]
    pub fn nan_parts(&self) -> Option<(bool, bool, &DecBig)> {
        match &self.repr {
            Repr::Nan {
                sign,
                signaling,
                payload,
            } => Some((*sign, *signaling, payload)),
            _ => None,
        }
    }
}

impl Default for Decimal {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_finite_zero_positive() {
        let z = Decimal::zero();
        assert!(z.is_finite());
        assert!(z.is_zero());
        assert!(!z.is_negative());
        assert!(!z.is_nan());
        assert!(!z.is_infinite());
        assert_eq!(z.digits(), Some(1));
    }

    #[test]
    fn negative_zero_distinct_from_positive() {
        let neg_zero = Decimal::finite(true, DecBig::zero(), 0);
        let pos_zero = Decimal::zero();
        assert!(neg_zero.is_zero());
        assert!(neg_zero.is_negative());
        // Representation equality separates the two signed zeros.
        assert_ne!(neg_zero, pos_zero);
    }

    #[test]
    fn cohort_members_are_distinct() {
        // 1.0 (coeff 10, exp -1) versus 1.00 (coeff 100, exp -2).
        let one_0 = Decimal::finite(false, DecBig::from_u32(10), -1);
        let one_00 = Decimal::finite(false, DecBig::from_u32(100), -2);
        assert_ne!(one_0, one_00);
        assert_eq!(one_0.digits(), Some(2));
        assert_eq!(one_00.digits(), Some(3));
    }

    #[test]
    fn infinity_classification() {
        let pos_inf = Decimal::infinity(false);
        let neg_inf = Decimal::infinity(true);
        assert!(pos_inf.is_infinite());
        assert!(!pos_inf.is_finite());
        assert!(neg_inf.is_negative());
        assert!(!pos_inf.is_negative());
        assert_eq!(pos_inf.digits(), None);
    }

    #[test]
    fn nan_classification() {
        let qnan = Decimal::quiet_nan(false, DecBig::zero());
        let snan = Decimal::signaling_nan(true, DecBig::from_u32(123));
        assert!(qnan.is_nan());
        assert!(!qnan.is_signaling_nan());
        assert!(snan.is_nan());
        assert!(snan.is_signaling_nan());
        assert!(snan.is_negative());
        let (sign, signaling, payload) = snan.nan_parts().unwrap();
        assert!(sign && signaling);
        assert_eq!(payload.to_u128(), Some(123));
    }

    #[test]
    fn integer_constructors_are_exact() {
        assert_eq!(Decimal::from_u64(0), Decimal::zero());
        assert_eq!(
            Decimal::from_u64(12345),
            Decimal::finite(false, DecBig::from_u32(12345), 0)
        );
        assert_eq!(
            Decimal::from_u128(u128::MAX),
            Decimal::finite(false, DecBig::from_u128(u128::MAX), 0)
        );
        assert_eq!(
            Decimal::from_i64(-42),
            Decimal::finite(true, DecBig::from_u32(42), 0)
        );
        assert!(!Decimal::from_i64(7).is_negative());
        // i64::MIN does not overflow: unsigned_abs gives 2^63.
        let min64 = Decimal::from_i64(i64::MIN);
        let (sign, coeff, exp) = min64.finite_parts().unwrap();
        assert!(sign && exp == 0 && coeff.to_u128() == Some(1u128 << 63));
        let min128 = Decimal::from_i128(i128::MIN);
        assert_eq!(
            min128.finite_parts().unwrap().1.to_u128(),
            Some(1u128 << 127)
        );
    }

    #[test]
    fn finite_parts_roundtrip() {
        let d = Decimal::finite(true, DecBig::from_u128(123_456_789_000), -4);
        let (sign, coeff, exp) = d.finite_parts().unwrap();
        assert!(sign);
        assert_eq!(coeff.to_u128(), Some(123_456_789_000));
        assert_eq!(exp, -4);
        assert!(Decimal::infinity(false).finite_parts().is_none());
    }
}
