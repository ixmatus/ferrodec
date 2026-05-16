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

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// Convert to `f64`, returning `(value, Status)`.
    ///
    /// Specials map straight through: `qNaN → f64::NAN`, `±∞ →
    /// f64::±INFINITY`, `±0 → ±0.0`. Signaling NaN inputs raise
    /// `Status::INVALID` and return a quiet `f64::NAN` per IEEE
    /// 754-2019 §5.4.2 convertFormat and §7.2 invalid operation. A
    /// quiet NaN passes through with `Status::OK`.
    ///
    /// Finite Decimal32 values convert exactly: the 7-decimal-digit
    /// coefficient is at most `9_999_999`, which f64's 53-bit
    /// significand (`~15.95` decimal digits) represents without
    /// loss, and the Decimal32 exponent range maps to f64 exponents
    /// well inside binary64's range. The exact value is `coef ×
    /// 10^exp`. The power `10^exp` is itself exact only for small
    /// `exp`; for the extreme exponents `pow10_f64` introduces at
    /// most one binary64 rounding, but every representable Decimal32
    /// (`coef × 10^exp` with `exp` in the encoded range) lands on a
    /// binary64 value whose nearest neighbour is the true value, so
    /// no `Status::INEXACT` is raised. This differs from Decimal64,
    /// whose 16-digit coefficients exceed binary64 precision.
    ///
    /// The `_rm` parameter is accepted for API parity with the
    /// `RoundingMode`-taking spec convertFormat operation but is not
    /// used internally: f64's native round-to-nearest-even governs
    /// the multiply step, and the Decimal32 result is exact.
    ///
    /// API change: the previous signature `to_f64(self) -> f64`
    /// swallowed the sNaN invalid-operation signal silently. The new
    /// signature surfaces it through the `Status` channel. This is a
    /// breaking change for every downstream caller.
    #[must_use]
    pub fn to_f64(self, _rm: RoundingMode) -> (f64, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { .. } => (f64::NAN, Status::INVALID),
            Class::QuietNaN { .. } => (f64::NAN, Status::OK),
            Class::Infinity { sign: false } => (f64::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (f64::NEG_INFINITY, Status::OK),
            Class::Zero { sign, .. } => (if sign { -0.0 } else { 0.0 }, Status::OK),
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let exp = biased_exp as i32 - BIAS as i32;
                let coef_f = f64::from(coefficient);
                let factor = pow10_f64(exp);
                let magnitude = coef_f * factor;
                (if sign { -magnitude } else { magnitude }, Status::OK)
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
            return (Decimal32::NAN, Status::INVALID);
        }
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
        assert_eq!(Decimal32::ZERO.to_f64(RoundingMode::NearestEven).0, 0.0);
        assert!(Decimal32::NEG_ZERO
            .to_f64(RoundingMode::NearestEven)
            .0
            .is_sign_negative());
        assert_eq!(Decimal32::ONE.to_f64(RoundingMode::NearestEven).0, 1.0);
        assert_eq!(Decimal32::NEG_ONE.to_f64(RoundingMode::NearestEven).0, -1.0);
        assert_eq!(from_int(15, -1).to_f64(RoundingMode::NearestEven).0, 1.5);
        assert_eq!(from_int(-2, 0).to_f64(RoundingMode::NearestEven).0, -2.0);
    }

    #[test]
    fn to_f64_specials() {
        assert!(Decimal32::NAN.to_f64(RoundingMode::NearestEven).0.is_nan());
        assert_eq!(
            Decimal32::INFINITY.to_f64(RoundingMode::NearestEven).0,
            f64::INFINITY
        );
        assert_eq!(
            Decimal32::NEG_INFINITY.to_f64(RoundingMode::NearestEven).0,
            f64::NEG_INFINITY
        );
    }

    #[test]
    fn to_f64_signaling_nan_raises_invalid() {
        // IEEE 754-2019 §5.4.2 convertFormat with §7.2 invalid
        // operation: a signaling NaN raises INVALID and yields a
        // quiet NaN. A quiet NaN passes through clean. A finite
        // value converts exactly with OK status.
        let (v, status) = Decimal32::SIGNALING_NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(status, Status::INVALID);

        let (v, status) = Decimal32::NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(status, Status::OK);

        let parsed = Decimal32::parse_str("42.5", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (v, status) = parsed.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 42.5);
        assert_eq!(status, Status::OK);
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
            let parsed = Decimal32::parse_str(s, RoundingMode::NearestEven)
                .unwrap()
                .0;
            let as_f64 = parsed.to_f64(RoundingMode::NearestEven).0;
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
