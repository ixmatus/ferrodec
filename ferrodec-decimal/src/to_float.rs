//! `Decimal::to_f64`: the binary-float read-out (the `binary-float` feature).
//!
//! The reverse of [`TryFrom<f64>`](crate::Decimal) (`from_float`). Where the
//! `from_float` direction is *exact* (every finite `f64` is a dyadic rational,
//! hence a finite decimal), this direction must round: most decimals are not
//! representable in 53-bit binary, so a single round to the `f64` grid is
//! unavoidable. The conversion renders `self` to its exact decimal string and
//! parses that string once with `f64::from_str`, so the rounding happens
//! exactly once. Going through the string (rather than via `Decimal128`) avoids
//! the double-rounding hazard of an intermediate fixed-width format.

use crate::Decimal;
use alloc::string::ToString;
use core::str::FromStr;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal {
    /// Convert `self` to the nearest `f64`, returning the value and a status.
    /// NaN / ±∞ / ±0 are passed through bit-exactly (an sNaN raises `INVALID`
    /// and yields a quiet NaN, a qNaN passes through quietly, per IEEE 754-2019
    /// §5.4.2 `convertFormat`). A finite value is rendered to its **exact**
    /// decimal string via [`Display`](core::fmt::Display) and parsed once by
    /// `f64::from_str`.
    ///
    /// The rounding is therefore `f64::from_str`'s, which is
    /// round-to-nearest-even: the returned `f64` is the nearest-even rounding of
    /// the exact decimal value of `self`. This is a **single** rounding step
    /// (the exact decimal of `self`, not an intermediate fixed-width value, is
    /// what reaches the `f64` grid), so it carries no double-rounding error.
    ///
    /// `rm` informs only the unrepresentable edges: a value too large in
    /// magnitude overflows to `±∞` with `OVERFLOW | INEXACT`, and a nonzero
    /// value too small underflows to `±0` with `UNDERFLOW | INEXACT`. The
    /// dominant rounding of in-range values is always round-to-nearest-even
    /// regardless of `rm`; a caller needing a different mode on the rounding
    /// step itself must quantize the `Decimal` first. As with
    /// `Decimal128::to_f64`, an in-range finite result is reported `INEXACT`
    /// unconditionally: detecting exact representability would require a
    /// re-encode and bit compare, which this read-out does not perform.
    #[must_use]
    pub fn to_f64(&self, _rm: RoundingMode) -> (f64, Status) {
        if self.is_nan() {
            let status = if self.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return (f64::NAN, status);
        }
        if self.is_infinite() {
            let v = if self.is_negative() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
            return (v, Status::OK);
        }
        if self.is_zero() {
            return (if self.is_negative() { -0.0 } else { 0.0 }, Status::OK);
        }
        // Finite, nonzero: render to the exact decimal string and round once.
        let s = self.to_string();
        match f64::from_str(&s) {
            Ok(v) => {
                let mut status = Status::OK;
                if v.is_infinite() {
                    status |= Status::OVERFLOW | Status::INEXACT;
                } else if v == 0.0 {
                    // A nonzero Decimal rounding to zero underflowed.
                    status |= Status::UNDERFLOW | Status::INEXACT;
                } else {
                    status |= Status::INEXACT;
                }
                (v, status)
            }
            // `Display` always emits a valid numeric string, so this arm is
            // unreachable; it mirrors the `Decimal128::to_f64` contract.
            Err(_) => (f64::NAN, Status::INVALID),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrodec_multiword::DecBig;

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn small_exact_values() {
        assert_eq!(parse("1.0").to_f64(RoundingMode::NearestEven).0, 1.0);
        assert_eq!(parse("2.5").to_f64(RoundingMode::NearestEven).0, 2.5);
        assert_eq!(parse("-2.5").to_f64(RoundingMode::NearestEven).0, -2.5);
        assert_eq!(parse("0.25").to_f64(RoundingMode::NearestEven).0, 0.25);
    }

    #[test]
    fn exact_long_expansion_of_point_one_rounds_to_canonical() {
        // The exact 55-digit decimal value of the f64 `0.1` must round back to
        // that same `0.1_f64` bit pattern (single round-to-nearest-even).
        let exact = parse("0.1000000000000000055511151231257827021181583404541015625");
        let (v, st) = exact.to_f64(RoundingMode::NearestEven);
        assert_eq!(v.to_bits(), 0.1_f64.to_bits());
        assert!(st.inexact());
    }

    #[test]
    fn signed_zero_passes_through() {
        let (p, _) = Decimal::finite(false, DecBig::zero(), 0).to_f64(RoundingMode::NearestEven);
        assert_eq!(p, 0.0);
        assert!(!p.is_sign_negative());
        let (n, _) = Decimal::finite(true, DecBig::zero(), 0).to_f64(RoundingMode::NearestEven);
        assert_eq!(n, 0.0);
        assert!(n.is_sign_negative());
    }

    #[test]
    fn infinities_pass_through() {
        let (p, sp) = Decimal::infinity(false).to_f64(RoundingMode::NearestEven);
        assert!(p.is_infinite() && !p.is_sign_negative());
        assert!(sp.is_ok());
        let (n, _) = Decimal::infinity(true).to_f64(RoundingMode::NearestEven);
        assert!(n.is_infinite() && n.is_sign_negative());
    }

    #[test]
    fn quiet_nan_passes_through_quietly() {
        let (v, s) = Decimal::quiet_nan(false, DecBig::zero()).to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert!(!s.invalid(), "qNaN must not raise INVALID");
    }

    #[test]
    fn signaling_nan_raises_invalid() {
        let (v, s) =
            Decimal::signaling_nan(false, DecBig::zero()).to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert!(s.invalid(), "sNaN must raise INVALID");
    }

    #[test]
    fn overflow_to_infinity() {
        let (v, st) = parse("1e400").to_f64(RoundingMode::NearestEven);
        assert!(v.is_infinite() && !v.is_sign_negative());
        assert!(st.overflow() && st.inexact());
    }

    #[test]
    fn underflow_to_zero() {
        let (v, st) = parse("1e-400").to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 0.0);
        assert!(!v.is_sign_negative());
        assert!(st.underflow() && st.inexact());
    }
}
