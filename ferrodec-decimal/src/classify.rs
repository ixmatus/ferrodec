//! General Decimal Arithmetic classification: the `class` operation, the
//! context-dependent `isNormal` / `isSubnormal` predicates, and the format
//! constants `isCanonical`, `isSigned`, and `radix`.
//!
//! A finite nonzero value is subnormal when its adjusted exponent (the power of
//! ten of its most significant digit) falls below the context minimum `emin`,
//! and normal otherwise; a zero is neither. Classification looks at the value as
//! given, with no rounding. See the General Decimal Arithmetic specification
//! ("class", "is-normal", "is-subnormal", "is-canonical", "radix") and
//! ADR-0041.

use crate::{Context, Decimal};
use ferrodec_multiword::DecBig;

impl Decimal {
    /// General Decimal Arithmetic `class`: the classification of `self` as one
    /// of `sNaN`, `NaN`, `-Infinity`, `-Normal`, `-Subnormal`, `-Zero`,
    /// `+Zero`, `+Subnormal`, `+Normal`, or `+Infinity`. The NaN classes carry
    /// no sign or payload. Normal versus subnormal depends on the context.
    #[must_use]
    pub fn class(&self, ctx: &Context) -> &'static str {
        if self.is_signaling_nan() {
            return "sNaN";
        }
        if self.is_nan() {
            return "NaN";
        }
        let neg = self.is_negative();
        if self.is_infinite() {
            return if neg { "-Infinity" } else { "+Infinity" };
        }
        let (_, coeff, exp) = self.finite_parts().expect("finite after special cases");
        if coeff.is_zero() {
            return if neg { "-Zero" } else { "+Zero" };
        }
        match (neg, is_subnormal_finite(coeff, exp, ctx.emin)) {
            (false, false) => "+Normal",
            (false, true) => "+Subnormal",
            (true, false) => "-Normal",
            (true, true) => "-Subnormal",
        }
    }

    /// Whether `self` is a normal number in this context: finite, non-zero, and
    /// with an adjusted exponent at or above `emin`.
    #[must_use]
    pub fn is_normal(&self, ctx: &Context) -> bool {
        self.finite_parts().is_some_and(|(_, coeff, exp)| {
            !coeff.is_zero() && !is_subnormal_finite(coeff, exp, ctx.emin)
        })
    }

    /// Whether `self` is subnormal in this context: finite, non-zero, and with
    /// an adjusted exponent below `emin`.
    #[must_use]
    pub fn is_subnormal(&self, ctx: &Context) -> bool {
        self.finite_parts()
            .is_some_and(|(_, coeff, exp)| is_subnormal_finite(coeff, exp, ctx.emin))
    }

    /// Whether `self` is canonical. Every value of this arbitrary-precision type
    /// is canonical: the representation has no redundant encodings (unlike the
    /// fixed-width BID and DPD formats), so this is always `true`.
    #[must_use]
    pub fn is_canonical(&self) -> bool {
        true
    }

    /// Whether `self` carries a negative sign (the General Decimal Arithmetic
    /// `isSigned`), including negative zero, negative infinity, and a signed
    /// NaN. An alias of [`is_negative`](Self::is_negative).
    #[must_use]
    pub fn is_signed(&self) -> bool {
        self.is_negative()
    }

    /// The radix of the arithmetic, always the decimal `10`.
    #[must_use]
    pub fn radix() -> Decimal {
        Decimal::finite(false, DecBig::from_u32(10), 0)
    }
}

/// Whether a finite value with coefficient `coeff` and exponent `exp` is
/// subnormal in a context with minimum adjusted exponent `emin`: it must be
/// non-zero and have an adjusted exponent below `emin`.
fn is_subnormal_finite(coeff: &DecBig, exp: i32, emin: i32) -> bool {
    !coeff.is_zero() && i64::from(exp) + coeff.decimal_digit_count() as i64 - 1 < i64::from(emin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;

    fn ctx() -> Context {
        Context::new(9, 999, -999, Rounding::HalfEven)
    }

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn class_strings() {
        let c = ctx();
        assert_eq!(parse("0").class(&c), "+Zero");
        assert_eq!(parse("-0.00").class(&c), "-Zero");
        // Adjusted exponent below emin (-999) is subnormal; at emin is normal.
        assert_eq!(parse("1E-1007").class(&c), "+Subnormal");
        assert_eq!(parse("0.99999999E-999").class(&c), "+Subnormal");
        assert_eq!(parse("1.00000000E-999").class(&c), "+Normal");
        assert_eq!(parse("1E-999").class(&c), "+Normal");
        assert_eq!(parse("-2.50").class(&c), "-Normal");
        assert_eq!(parse("Inf").class(&c), "+Infinity");
        assert_eq!(parse("-Inf").class(&c), "-Infinity");
        // NaN classes carry no sign or payload.
        assert_eq!(parse("-NaN12345").class(&c), "NaN");
        assert_eq!(parse("-sNaN999").class(&c), "sNaN");
    }

    #[test]
    fn predicates() {
        let c = ctx();
        assert!(parse("1").is_normal(&c));
        assert!(!parse("1").is_subnormal(&c));
        assert!(parse("1E-1007").is_subnormal(&c));
        assert!(!parse("1E-1007").is_normal(&c));
        // A zero is neither normal nor subnormal.
        assert!(!parse("0").is_normal(&c));
        assert!(!parse("0").is_subnormal(&c));
        // Specials are neither.
        assert!(!Decimal::infinity(false).is_normal(&c));
        assert!(!Decimal::quiet_nan(false, DecBig::zero()).is_subnormal(&c));
        // Every value is canonical; isSigned tracks the sign bit.
        assert!(parse("1").is_canonical());
        assert!(Decimal::quiet_nan(true, DecBig::zero()).is_canonical());
        assert!(parse("-0").is_signed());
        assert!(!parse("0").is_signed());
        // radix is always ten.
        assert_eq!(Decimal::radix(), parse("10"));
    }
}
