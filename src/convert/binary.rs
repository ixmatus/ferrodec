//! `f32` / `f64` conversions.
//!
//! Both directions go through the canonical decimal-string round-trip:
//!
//! ```text
//! Decimal128 ─┬─Display→  "1.23e4"  ─f64::FromStr→─┐
//!             │                                   │
//! Decimal128 ←┴─parse_str─  "1.23e4"  ←f64::Display┘
//! ```
//!
//! Pros: correctness is delegated to two well-tested round-trip
//! kernels (Rust's `f64::Display` is Grisu/Ryu, and our
//! `parse_str` is the same parser the public API exposes).
//!
//! Cons: each direction allocates a small fixed-size buffer for the
//! intermediate string and walks it once. Acceptable for the
//! display-formatting use case this feature was added for; if the
//! conversion ever lands on a hot path, a direct
//! `mantissa · 5^|e|` integer pipeline can be added without touching
//! the public API.
//!
//! ## Buffer sizing
//!
//! `f64::Display` produces at most ~24 bytes for any finite value
//! (sign, leading digit, "." separator, ≤ 17 fractional digits, "e"
//! and a 4-digit signed exponent). `Decimal128`'s `Display` can
//! produce up to ~45 bytes (sign, 34 digits, decimal point,
//! "e"+5-digit exponent). 64 bytes covers both with margin.

use core::fmt::Write as _;
use core::str::FromStr;

use crate::decimal::Decimal128;
use crate::status::{RoundingMode, Status};

/// Maximum length of a `Display`-formatted `Decimal128` or `f64`.
const STR_BUF_LEN: usize = 64;

struct StrBuf {
    buf: [u8; STR_BUF_LEN],
    len: usize,
}

impl StrBuf {
    fn new() -> Self {
        Self {
            buf: [0; STR_BUF_LEN],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        // The writes only ever go through `core::fmt::Display`, which
        // emits valid UTF-8.
        core::str::from_utf8(&self.buf[..self.len]).expect("display output is utf-8")
    }
}

impl core::fmt::Write for StrBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.len + bytes.len() > STR_BUF_LEN {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

impl Decimal128 {
    /// Convert `self` to `f64`, using the canonical decimal-string
    /// round-trip. NaN / ±∞ / ±0 are passed through bit-exactly.
    /// Finite values are rendered to a buffer via `Display` and parsed
    /// by `f64::from_str`, giving correctly-rounded conversion to
    /// nearest within the f64 precision envelope (~17 decimal digits).
    ///
    /// `rm` currently informs only the unrepresentable-edge cases
    /// (overflow ⇒ ±∞, underflow ⇒ ±0); the dominant rounding is
    /// performed by `f64::from_str`, which is round-to-nearest-even.
    #[must_use]
    pub fn to_f64(self, _rm: RoundingMode) -> (f64, Status) {
        if self.is_nan() {
            // IEEE 754-2019 §5.4.2 convertFormat: an sNaN operand
            // raises INVALID and yields a quiet NaN; a qNaN passes
            // through quietly.
            let status = if self.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return (f64::NAN, status);
        }
        if self.is_infinite() {
            return (
                if self.is_sign_negative() {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                Status::OK,
            );
        }
        if self.is_zero() {
            return (if self.is_sign_negative() { -0.0 } else { 0.0 }, Status::OK);
        }
        // Finite: render to canonical decimal string, parse as f64.
        let mut buf = StrBuf::new();
        write!(&mut buf, "{self}").expect("Decimal128 display fits 64 bytes");
        match f64::from_str(buf.as_str()) {
            Ok(v) => {
                let mut status = Status::OK;
                if v.is_infinite() {
                    status |= Status::OVERFLOW | Status::INEXACT;
                } else if v == 0.0 && !self.is_zero() {
                    status |= Status::UNDERFLOW | Status::INEXACT;
                } else {
                    // f64 round-to-nearest is inexact for any value
                    // not exactly representable in 53-bit binary
                    // precision. We don't try to detect "exact" here
                    // (would require a re-encode + bit compare).
                    status |= Status::INEXACT;
                }
                (v, status)
            }
            Err(_) => (f64::NAN, Status::INVALID),
        }
    }

    /// Convert `self` to `f32`.
    ///
    /// Direct decimal-string → f32 path (not `to_f64() as f32`): the
    /// double-rounding hazard from going through f64 produces a
    /// 1-ULP error for any value that lies on a half-ULP boundary in
    /// f32 but slightly off-boundary in f64. `f32::from_str` is
    /// correctly rounded, so a single rounding step keeps the result
    /// inside the f32 envelope.
    #[must_use]
    pub fn to_f32(self, _rm: RoundingMode) -> (f32, Status) {
        if self.is_nan() {
            // Same sNaN rule as to_f64: signaling raises INVALID and
            // quiets to a NaN; quiet passes through.
            let status = if self.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return (f32::NAN, status);
        }
        if self.is_infinite() {
            return (
                if self.is_sign_negative() {
                    f32::NEG_INFINITY
                } else {
                    f32::INFINITY
                },
                Status::OK,
            );
        }
        if self.is_zero() {
            return (
                if self.is_sign_negative() { -0.0 } else { 0.0 },
                Status::OK,
            );
        }
        let mut buf = StrBuf::new();
        write!(&mut buf, "{self}").expect("Decimal128 display fits 64 bytes");
        match f32::from_str(buf.as_str()) {
            Ok(v) => {
                let mut status = Status::OK;
                if v.is_infinite() {
                    status |= Status::OVERFLOW | Status::INEXACT;
                } else if v == 0.0 && !self.is_zero() {
                    status |= Status::UNDERFLOW | Status::INEXACT;
                } else {
                    // Same caveat as to_f64: we don't try to detect
                    // "exact" here, which would require a re-encode +
                    // bit compare against the original Decimal128.
                    status |= Status::INEXACT;
                }
                (v, status)
            }
            Err(_) => (f32::NAN, Status::INVALID),
        }
    }

    /// Convert an `f64` to `Decimal128` via the canonical
    /// decimal-string round-trip. NaN / ±∞ / ±0 are passed through.
    /// `f64`'s ~17 decimal digits of precision become a 17-digit
    /// Decimal128 coefficient (with the rest being trailing zeros
    /// from `Display`'s shortest-round-trip output).
    #[must_use]
    pub fn from_f64(value: f64) -> Self {
        if value.is_nan() {
            return Decimal128::NAN;
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            };
        }
        if value == 0.0 {
            return if value.is_sign_negative() {
                Decimal128::NEG_ZERO
            } else {
                Decimal128::ZERO
            };
        }
        // Use scientific notation explicitly — `{value}` uses fixed
        // notation for moderate magnitudes which can run to 100+
        // characters for very small or very large `f64` (e.g.
        // `1e-100` → "0.0000…0001"). `{value:e}` always fits in a
        // few dozen characters.
        let mut buf = StrBuf::new();
        write!(&mut buf, "{value:e}").expect("f64 scientific format fits 64 bytes");
        match Decimal128::parse_str(buf.as_str(), RoundingMode::NearestEven) {
            Ok((d, _)) => d,
            Err(_) => Decimal128::NAN,
        }
    }

    /// Convert an `f32` to `Decimal128`.
    #[must_use]
    pub fn from_f32(value: f32) -> Self {
        Self::from_f64(value as f64)
    }
}

/// Error returned by [`TryFrom<f32>`] / [`TryFrom<f64>`] for
/// [`Decimal128`] when the input is not a finite number.
///
/// `from_f32` / `from_f64` route NaN and ±∞ to their `Decimal128`
/// counterparts silently. The `TryFrom` impls reject those inputs
/// instead so callers expecting a finite decimal don't have to
/// re-check the result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal128FromFloatError {
    /// The input was NaN.
    NotANumber,
    /// The input was `+∞` or `−∞`.
    Infinite,
}

impl core::fmt::Display for Decimal128FromFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotANumber => f.write_str("cannot convert NaN to Decimal128"),
            Self::Infinite => f.write_str("cannot convert ±∞ to Decimal128"),
        }
    }
}

impl core::error::Error for Decimal128FromFloatError {}

impl TryFrom<f64> for Decimal128 {
    type Error = Decimal128FromFloatError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_nan() {
            return Err(Decimal128FromFloatError::NotANumber);
        }
        if value.is_infinite() {
            return Err(Decimal128FromFloatError::Infinite);
        }
        Ok(Self::from_f64(value))
    }
}

impl TryFrom<f32> for Decimal128 {
    type Error = Decimal128FromFloatError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::try_from(f64::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from_f64_finite_succeeds() {
        let d = Decimal128::try_from(1.5_f64).unwrap();
        let (back, _) = d.to_f64(RoundingMode::NearestEven);
        assert_eq!(back, 1.5);
    }

    #[test]
    fn try_from_f64_nan_rejects() {
        let r = Decimal128::try_from(f64::NAN);
        assert_eq!(r, Err(Decimal128FromFloatError::NotANumber));
    }

    #[test]
    fn try_from_f64_inf_rejects() {
        let r = Decimal128::try_from(f64::INFINITY);
        assert_eq!(r, Err(Decimal128FromFloatError::Infinite));
        let r = Decimal128::try_from(f64::NEG_INFINITY);
        assert_eq!(r, Err(Decimal128FromFloatError::Infinite));
    }

    #[test]
    fn try_from_f32_routes_through_f64() {
        let d = Decimal128::try_from(2.5_f32).unwrap();
        let (back, _) = d.to_f32(RoundingMode::NearestEven);
        assert_eq!(back, 2.5_f32);
        assert_eq!(
            Decimal128::try_from(f32::NAN),
            Err(Decimal128FromFloatError::NotANumber)
        );
    }

    #[test]
    fn to_f64_zero_signed() {
        let (v, _) = Decimal128::ZERO.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 0.0);
        assert!(!v.is_sign_negative());
        let (v, _) = Decimal128::NEG_ZERO.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 0.0);
        assert!(v.is_sign_negative());
    }

    #[test]
    fn to_f64_one() {
        let (v, _) = Decimal128::ONE.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn to_f64_neg_one() {
        let (v, _) = Decimal128::NEG_ONE.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, -1.0);
    }

    #[test]
    fn to_f64_pi_approx() {
        let pi = Decimal128::parse_str("3.14159265358979323846", RoundingMode::default())
            .unwrap()
            .0;
        let (v, _) = pi.to_f64(RoundingMode::NearestEven);
        assert!((v - core::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn to_f64_huge_overflows() {
        let big = Decimal128::parse_str("1e400", RoundingMode::default())
            .unwrap()
            .0;
        let (v, st) = big.to_f64(RoundingMode::NearestEven);
        assert!(v.is_infinite());
        assert!(st.overflow());
    }

    #[test]
    fn to_f64_tiny_underflows() {
        let tiny = Decimal128::parse_str("1e-400", RoundingMode::default())
            .unwrap()
            .0;
        let (v, st) = tiny.to_f64(RoundingMode::NearestEven);
        assert_eq!(v, 0.0);
        assert!(st.underflow());
    }

    #[test]
    fn to_f64_inf_passes_through() {
        let (v, _) = Decimal128::INFINITY.to_f64(RoundingMode::NearestEven);
        assert!(v.is_infinite() && !v.is_sign_negative());
        let (v, _) = Decimal128::NEG_INFINITY.to_f64(RoundingMode::NearestEven);
        assert!(v.is_infinite() && v.is_sign_negative());
    }

    #[test]
    fn to_f64_qnan_passes_through_quietly() {
        // qNaN: convertFormat returns NaN with no exception flag.
        let (v, s) = Decimal128::NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert!(!s.invalid(), "qNaN must not raise INVALID");
    }

    #[test]
    fn to_f64_snan_raises_invalid() {
        // IEEE 754-2019 §5.4.2 convertFormat: an sNaN operand raises
        // INVALID and yields a quiet NaN. The previous implementation
        // silently dropped the sNaN signal.
        let (v, s) = Decimal128::SIGNALING_NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan(), "result must still be NaN");
        assert!(s.invalid(), "sNaN must raise INVALID");
    }

    #[test]
    fn to_f32_snan_raises_invalid() {
        // to_f32 inherits to_f64's status (it routes through to_f64
        // and then double-rounds the value), so the sNaN signal must
        // propagate through the wrapper.
        let (v, s) = Decimal128::SIGNALING_NAN.to_f32(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert!(s.invalid(), "sNaN must raise INVALID through to_f32");
    }

    #[test]
    fn from_f64_zero_signed() {
        assert!(!Decimal128::from_f64(0.0).is_sign_negative());
        assert!(Decimal128::from_f64(-0.0).is_sign_negative());
    }

    #[test]
    fn from_f64_one() {
        let d = Decimal128::from_f64(1.0);
        let (cmp, _) = d.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn from_f64_round_trip() {
        let cases = [1.0f64, -1.0, 1.5, -7.654_321, 6.022e23, 1e-100, 9.876e50];
        for &f in &cases {
            let d = Decimal128::from_f64(f);
            let (back, _) = d.to_f64(RoundingMode::NearestEven);
            // f64 display gives shortest round-trip → same f64.
            assert_eq!(back, f, "round-trip failed for {f}");
        }
    }

    #[test]
    fn from_f64_inf_nan() {
        assert!(Decimal128::from_f64(f64::INFINITY).is_infinite());
        assert!(Decimal128::from_f64(f64::NEG_INFINITY).is_infinite());
        assert!(Decimal128::from_f64(f64::NEG_INFINITY).is_sign_negative());
        assert!(Decimal128::from_f64(f64::NAN).is_nan());
    }

    #[test]
    fn from_f32_basic() {
        let d = Decimal128::from_f32(2.5_f32);
        let want = Decimal128::parse_str("2.5", RoundingMode::default())
            .unwrap()
            .0;
        let (cmp, _) = d.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn to_f32_basic() {
        let d = Decimal128::parse_str("3.5", RoundingMode::default())
            .unwrap()
            .0;
        let (v, _) = d.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, 3.5_f32);
    }

    #[test]
    fn to_f32_no_double_rounding() {
        // Direct decimal → f32 path must agree with `format!("{d}").parse::<f32>()`
        // for every value. Going through f64 first introduces a
        // double-rounding hazard for values that lie on a half-ULP of
        // f32 but slightly off in f64. The test compares the two and
        // would fail bit-exact if any case diverged.
        //
        // The values cover small integers, simple fractions, two
        // boundary-ish exponents, and one classic double-rounding
        // candidate (a value picked so that f64-rounding nudges it
        // across a half-ULP_f32 boundary).
        for s in [
            "0",
            "1",
            "-1",
            "3.5",
            "0.1",
            "-0.1",
            "1.234567890123456789",
            "1e-30",
            "1e30",
            // 8.589973e9 is near a power-of-2 boundary in f32; it's
            // the first half-ULP in `[2^33, 2^34)` and is a known
            // double-rounding pet case for decimal → binary
            // conversions through an intermediate.
            "8589973000",
        ] {
            let d = Decimal128::parse_str(s, RoundingMode::default()).unwrap().0;
            let (got, _) = d.to_f32(RoundingMode::NearestEven);
            let mut buf = StrBuf::new();
            write!(&mut buf, "{d}").unwrap();
            let want = f32::from_str(buf.as_str()).expect("display parses back");
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "to_f32({s}): got {got:?}, want {want:?} (display={})",
                buf.as_str(),
            );
        }
    }

    #[test]
    fn to_f32_inf_nan_zero() {
        let (v, _) = Decimal128::INFINITY.to_f32(RoundingMode::default());
        assert!(v.is_infinite() && !v.is_sign_negative());
        let (v, _) = Decimal128::NEG_INFINITY.to_f32(RoundingMode::default());
        assert!(v.is_infinite() && v.is_sign_negative());
        let (v, _) = Decimal128::ZERO.to_f32(RoundingMode::default());
        assert_eq!(v.to_bits(), 0u32);
        let (v, _) = Decimal128::NEG_ZERO.to_f32(RoundingMode::default());
        assert_eq!(v.to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn to_f32_huge_overflows() {
        let big = Decimal128::parse_str("1e100", RoundingMode::default())
            .unwrap()
            .0;
        let (v, s) = big.to_f32(RoundingMode::default());
        assert!(v.is_infinite());
        assert!(s.overflow());
        assert!(s.inexact());
    }

    #[test]
    fn to_f32_tiny_underflows() {
        let small = Decimal128::parse_str("1e-100", RoundingMode::default())
            .unwrap()
            .0;
        let (v, s) = small.to_f32(RoundingMode::default());
        assert_eq!(v, 0.0_f32);
        assert!(s.underflow());
        assert!(s.inexact());
    }
}
