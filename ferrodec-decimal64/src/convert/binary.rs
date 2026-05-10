//! `Decimal64` ↔ `f64` conversion (gated on the `binary-float` feature).
//!
//! The Decimal64-to-f64 direction is rounded to f64's ~15.95-digit
//! precision: we extract `(coef, exp)` and compute `coef × 10^exp` using
//! pairwise integer-exponent doubling (so overflow / underflow follow
//! IEEE 754 binary64 conventions). Because Decimal64 carries 16 digits
//! and f64 only ~15.95, the bottom digit of the Decimal64 input may be
//! lost in the round trip; callers needing full Decimal64 precision in
//! their f64 interop should use [`Decimal64::parse_str`] /
//! [`core::fmt::Display`] instead. The f64-to-Decimal64 direction
//! formats the f64 to a 17-digit scientific string in a 32-byte stack
//! buffer and parses via [`Decimal64::parse_str`]; rounding is honoured
//! by the parser. Both directions are alloc-free.
//!
//! Used internally by the transcendental kernels (exp, ln, sin, ...);
//! also exposed publicly so callers can interoperate with `f64` math
//! that lives outside the decimal type.

use core::fmt::Write as _;

use crate::bid::{classify_bits, BIAS, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
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
                // `coefficient` is a u64 up to 10¹⁶ − 1 ≈ 2⁵³·15;
                // f64 carries 53 bits of mantissa, so the cast is
                // exact for values up to 2⁵³ and round-to-nearest
                // for larger values — which matches f64's own
                // round-to-nearest behaviour for the subsequent
                // `coef × 10^exp` multiply.
                #[allow(clippy::cast_precision_loss)]
                let coef_f = coefficient as f64;
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

    /// Construct a `Decimal64` from an `f64`, rounding by `rm`.
    ///
    /// Returns `(value, Status)`. `Status::INEXACT` is set when the
    /// rounded 16-digit Decimal64 differs from the f64 input.
    #[must_use]
    pub fn from_f64(x: f64, rm: RoundingMode) -> (Self, Status) {
        if x.is_nan() {
            // f64 only carries quiet NaN at the language level.
            return (Decimal64::NAN, Status::OK);
        }
        if x.is_infinite() {
            return (
                if x > 0.0 {
                    Decimal64::INFINITY
                } else {
                    Decimal64::NEG_INFINITY
                },
                Status::OK,
            );
        }
        if x == 0.0 {
            return (
                if x.is_sign_negative() {
                    Decimal64::NEG_ZERO
                } else {
                    Decimal64::ZERO
                },
                Status::OK,
            );
        }

        // 48-byte buffer for `{:.17e}` worst-case rendering of any
        // finite f64. The longest output `-1.<17 digits>e-308` is
        // ~26 chars; subnormal `5e-324`-shaped values render to ~24.
        // We allocate 48 (~2× headroom) so the future stdlib float
        // formatter can grow without silently overflowing.
        let mut buf = [0u8; 48];
        let mut writer = BufWriter {
            buf: &mut buf,
            len: 0,
        };
        let write_result = write!(writer, "{x:.17e}");
        if write_result.is_err() {
            // Buffer overflow — defensive fallback. With 48 bytes
            // this is unreachable on every libcore version we know
            // of, but we don't return a wrong value if a future
            // libcore extends the format.
            return (Decimal64::NAN, Status::INVALID);
        }
        let len = writer.len;
        let s = match core::str::from_utf8(&buf[..len]) {
            Ok(s) => s,
            // Shouldn't happen: core::fmt for f64 always emits ASCII.
            Err(_) => return (Decimal64::NAN, Status::INVALID),
        };
        match Decimal64::parse_str(s, rm) {
            Ok(out) => out,
            // parse_str on a 17-digit scientific f64 representation
            // should always succeed; treat any error as a defensive
            // NaN + INVALID.
            Err(_) => (Decimal64::NAN, Status::INVALID),
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

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn to_f64_basic() {
        assert_eq!(Decimal64::ZERO.to_f64(), 0.0);
        assert!(Decimal64::NEG_ZERO.to_f64().is_sign_negative());
        assert_eq!(Decimal64::ONE.to_f64(), 1.0);
        assert_eq!(Decimal64::NEG_ONE.to_f64(), -1.0);
        assert_eq!(from_int(15, -1).to_f64(), 1.5);
        assert_eq!(from_int(-2, 0).to_f64(), -2.0);
    }

    #[test]
    fn to_f64_specials() {
        assert!(Decimal64::NAN.to_f64().is_nan());
        assert_eq!(Decimal64::INFINITY.to_f64(), f64::INFINITY);
        assert_eq!(Decimal64::NEG_INFINITY.to_f64(), f64::NEG_INFINITY);
    }

    #[test]
    fn from_f64_basic() {
        // from_f64 produces values numerically equal to the input;
        // the cohort depends on the f64 rendering's fractional width.
        // Compare numerically via partial_cmp.
        let (d, _) = Decimal64::from_f64(2.5, RoundingMode::NearestEven);
        let two_five = from_int(25, -1);
        assert_eq!(
            d.partial_cmp(two_five).0,
            Some(core::cmp::Ordering::Equal),
            "from_f64(2.5) numerically equals 2.5"
        );

        let (d, _) = Decimal64::from_f64(-1.0, RoundingMode::NearestEven);
        assert_eq!(
            d.partial_cmp(Decimal64::NEG_ONE).0,
            Some(core::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn from_f64_specials() {
        let (d, _) = Decimal64::from_f64(f64::NAN, RoundingMode::NearestEven);
        assert!(d.is_quiet_nan());

        let (d, _) = Decimal64::from_f64(f64::INFINITY, RoundingMode::NearestEven);
        assert!(d.is_infinite() && !d.is_sign_negative());

        let (d, _) = Decimal64::from_f64(-0.0, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), Decimal64::NEG_ZERO.to_bits());

        let (d, _) = Decimal64::from_f64(0.0, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), Decimal64::ZERO.to_bits());
    }

    #[test]
    fn from_f64_max_in_range() {
        // f64::MAX ≈ 1.7977e308, well within Decimal64's E_MAX of 384,
        // so the converted value is finite (no Decimal64 input from f64
        // can overflow Decimal64's range; Decimal32 has this test, but
        // Decimal64's range exceeds f64's).
        let (d, _) = Decimal64::from_f64(f64::MAX, RoundingMode::NearestEven);
        assert!(d.is_finite());
        assert!(!d.is_zero());
    }

    #[test]
    fn round_trip_simple_values() {
        for s in &["1", "1.5", "-2.5", "0.0001", "12345.67"] {
            let parsed = Decimal64::parse_str(s, RoundingMode::NearestEven).unwrap().0;
            let as_f64 = parsed.to_f64();
            let (back, _) = Decimal64::from_f64(as_f64, RoundingMode::NearestEven);
            // Numerically equal (cohort may differ).
            assert_eq!(
                parsed.partial_cmp(back).0,
                Some(core::cmp::Ordering::Equal),
                "round-trip failed for {s}"
            );
        }
    }
}
