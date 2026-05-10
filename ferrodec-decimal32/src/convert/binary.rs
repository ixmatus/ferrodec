//! `Decimal32` ↔ `f64` conversion (gated on the `binary-float` feature).
//!
//! The Decimal32-to-f64 direction is exact within f64's ~15.95-digit
//! precision: we extract `(coef, exp)` and compute `coef × 10^exp` using
//! pairwise integer-exponent doubling (so overflow / underflow follow
//! IEEE 754 binary64 conventions). The f64-to-Decimal32 direction
//! formats the f64 to a 17-digit scientific string in a 32-byte stack
//! buffer and parses via [`Decimal32::parse_str`]; rounding is honoured
//! by the parser. Both directions are alloc-free.
//!
//! Used internally by the transcendental kernels (exp, ln, sin, ...);
//! also exposed publicly so callers can interoperate with `f64` math
//! that lives outside the decimal type.

use core::fmt::Write as _;

use crate::bid::{classify_bits, BIAS, Class};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// Lossy convert to `f64`. Specials map straight through: `NaN →
    /// f64::NAN`, `±∞ → f64::±INFINITY`, `±0 → ±0.0`. Finite values
    /// produce the f64 nearest to `coef × 10^exp` (within f64's
    /// ~15.95-digit precision).
    #[must_use]
    pub fn to_f64(self) -> f64 {
        match classify_bits(self.0) {
            Class::QuietNaN { .. } | Class::SignalingNaN { .. } => f64::NAN,
            Class::Infinity { sign: false } => f64::INFINITY,
            Class::Infinity { sign: true } => f64::NEG_INFINITY,
            Class::Zero { sign, .. } => {
                if sign {
                    -0.0
                } else {
                    0.0
                }
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let exp = biased_exp as i32 - BIAS as i32;
                let coef_f = f64::from(coefficient);
                let factor = pow10_f64(exp);
                let magnitude = coef_f * factor;
                if sign {
                    -magnitude
                } else {
                    magnitude
                }
            }
        }
    }

    /// Construct a `Decimal32` from an `f64`, rounding by `rm`.
    ///
    /// Returns `(value, Status)`. `Status::INEXACT` is set when the
    /// rounded 7-digit Decimal32 differs from the f64 input.
    #[must_use]
    pub fn from_f64(x: f64, rm: RoundingMode) -> (Self, Status) {
        if x.is_nan() {
            // f64 only carries quiet NaN at the language level.
            return (Decimal32::NAN, Status::OK);
        }
        if x.is_infinite() {
            return (
                if x > 0.0 {
                    Decimal32::INFINITY
                } else {
                    Decimal32::NEG_INFINITY
                },
                Status::OK,
            );
        }
        if x == 0.0 {
            return (
                if x.is_sign_negative() {
                    Decimal32::NEG_ZERO
                } else {
                    Decimal32::ZERO
                },
                Status::OK,
            );
        }

        let mut buf = [0u8; 32];
        let mut writer = BufWriter {
            buf: &mut buf,
            len: 0,
        };
        // {:.17e} renders 17 significant digits in scientific
        // notation — enough to capture any f64 precisely.
        let _ = write!(writer, "{x:.17e}");
        let len = writer.len;
        let s = match core::str::from_utf8(&buf[..len]) {
            Ok(s) => s,
            // Shouldn't happen: core::fmt for f64 always emits ASCII.
            Err(_) => return (Decimal32::NAN, Status::INVALID),
        };
        match Decimal32::parse_str(s, rm) {
            Ok(out) => out,
            // parse_str on a 17-digit scientific f64 representation
            // should always succeed; treat any error as a defensive
            // NaN + INVALID.
            Err(_) => (Decimal32::NAN, Status::INVALID),
        }
    }
}

/// `10^k` as `f64` for any `i32` `k`. Uses doubling-square exponentiation
/// in f64 so the standard IEEE 754 binary64 overflow / underflow rules
/// apply.
fn pow10_f64(exp: i32) -> f64 {
    if exp == 0 {
        return 1.0;
    }
    let abs = exp.unsigned_abs();
    let mut result: f64 = 1.0;
    let mut base: f64 = 10.0;
    let mut e = abs;
    while e > 0 {
        if e & 1 != 0 {
            result *= base;
        }
        base *= base;
        e >>= 1;
    }
    if exp < 0 {
        1.0 / result
    } else {
        result
    }
}

/// `core::fmt::Write` adapter writing into a fixed-size byte buffer.
struct BufWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl core::fmt::Write for BufWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.len;
        if bytes.len() > remaining {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn to_f64_basic() {
        assert_eq!(Decimal32::ZERO.to_f64(), 0.0);
        assert!(Decimal32::NEG_ZERO.to_f64().is_sign_negative());
        assert_eq!(Decimal32::ONE.to_f64(), 1.0);
        assert_eq!(Decimal32::NEG_ONE.to_f64(), -1.0);
        assert_eq!(from_int(15, -1).to_f64(), 1.5);
        assert_eq!(from_int(-2, 0).to_f64(), -2.0);
    }

    #[test]
    fn to_f64_specials() {
        assert!(Decimal32::NAN.to_f64().is_nan());
        assert_eq!(Decimal32::INFINITY.to_f64(), f64::INFINITY);
        assert_eq!(Decimal32::NEG_INFINITY.to_f64(), f64::NEG_INFINITY);
    }

    #[test]
    fn from_f64_basic() {
        // from_f64 produces values numerically equal to the input;
        // the cohort depends on the f64 rendering's fractional width.
        // Compare numerically via partial_cmp.
        let (d, _) = Decimal32::from_f64(2.5, RoundingMode::NearestEven);
        let two_five = from_int(25, -1);
        assert_eq!(
            d.partial_cmp(two_five).0,
            Some(core::cmp::Ordering::Equal),
            "from_f64(2.5) numerically equals 2.5"
        );

        let (d, _) = Decimal32::from_f64(-1.0, RoundingMode::NearestEven);
        assert_eq!(
            d.partial_cmp(Decimal32::NEG_ONE).0,
            Some(core::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn from_f64_specials() {
        let (d, _) = Decimal32::from_f64(f64::NAN, RoundingMode::NearestEven);
        assert!(d.is_quiet_nan());

        let (d, _) = Decimal32::from_f64(f64::INFINITY, RoundingMode::NearestEven);
        assert!(d.is_infinite() && !d.is_sign_negative());

        let (d, _) = Decimal32::from_f64(-0.0, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), Decimal32::NEG_ZERO.to_bits());

        let (d, _) = Decimal32::from_f64(0.0, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), Decimal32::ZERO.to_bits());
    }

    #[test]
    fn from_f64_overflow_rounds_to_infinity() {
        let (d, s) = Decimal32::from_f64(1e200, RoundingMode::NearestEven);
        assert!(d.is_infinite());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn round_trip_simple_values() {
        for s in &["1", "1.5", "-2.5", "0.0001", "12345.67"] {
            let parsed = Decimal32::parse_str(s, RoundingMode::NearestEven).unwrap().0;
            let as_f64 = parsed.to_f64();
            let (back, _) = Decimal32::from_f64(as_f64, RoundingMode::NearestEven);
            // Numerically equal (cohort may differ).
            assert_eq!(
                parsed.partial_cmp(back).0,
                Some(core::cmp::Ordering::Equal),
                "round-trip failed for {s}"
            );
        }
    }
}
