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
use core::num::FpCategory;
use core::str::FromStr;

use ferrodec_ieee::binary_conversion_status;

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal128;
use crate::status::{RoundingMode, Status};

/// The `convertFormat` status for a finite nonzero `d` rendered to a
/// correctly-rounded binary float (fd-aqs.12), extracting `d`'s
/// `(coefficient, exponent)` for the shared exactness rule. `inf` / `zero`
/// / `subnormal` are the converted float's flags.
fn binary_status(
    d: Decimal128,
    inf: bool,
    zero: bool,
    subnormal: bool,
    mantissa_bits: u32,
) -> Status {
    let (coef, exp) = match classify_bits(d.to_bits()) {
        Class::Finite {
            coefficient,
            biased_exp,
            ..
        } => (coefficient, biased_exp as i32 - BIAS as i32),
        _ => (0, 0),
    };
    binary_conversion_status(coef, exp, inf, zero, subnormal, mantissa_bits)
}

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
    /// Convert `self` to `f64` (IEEE 754-2019 §5.4.2 convertFormat),
    /// returning the correctly-rounded result and its status. NaN /
    /// ±∞ / ±0 pass through bit-exactly (an sNaN raises `INVALID`).
    /// Finite values are rendered via `Display`, which is the *exact*
    /// decimal value (GDA toSci, lossless), and parsed by the
    /// correctly-rounded `f64::from_str`, so the composition is
    /// correctly rounded (guarded by `to_f64_is_correctly_rounded`).
    /// `INEXACT` is set exactly when the value is not representable in
    /// f64 (fd-aqs.12, via `ferrodec_ieee::decimal_is_binary_exact`);
    /// a result of ±∞ / ±0 raises `OVERFLOW` / `UNDERFLOW`.
    ///
    /// A 2026-05-10 review's "L5" note had weakened this docstring to
    /// "not correctly rounded, up to 1 ULP" on the mistaken premise
    /// that `Display` produces a shortest-round-trip form; it produces
    /// the exact value, so the conversion is correctly rounded.
    /// fd-aqs.12 restored the accurate contract.
    ///
    /// `rm` is accepted for spec parity; f64's round-to-nearest-even
    /// governs the parse.
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
        // Finite: render to canonical decimal string, parse as f64
        // (correctly rounded). INEXACT is then decided exactly
        // (fd-aqs.12): the earlier code raised it unconditionally, so an
        // exact conversion such as `ONE.to_f64` reported INEXACT.
        let mut buf = StrBuf::new();
        write!(&mut buf, "{self}").expect("Decimal128 display fits 64 bytes");
        match f64::from_str(buf.as_str()) {
            Ok(v) => {
                let status = binary_status(
                    self,
                    v.is_infinite(),
                    v == 0.0,
                    v.classify() == FpCategory::Subnormal,
                    53,
                );
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
            return (if self.is_sign_negative() { -0.0 } else { 0.0 }, Status::OK);
        }
        let mut buf = StrBuf::new();
        write!(&mut buf, "{self}").expect("Decimal128 display fits 64 bytes");
        match f32::from_str(buf.as_str()) {
            Ok(v) => {
                let status = binary_status(
                    self,
                    v.is_infinite(),
                    v == 0.0,
                    v.classify() == FpCategory::Subnormal,
                    24,
                );
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
    pub fn from_f64(value: f64, rm: RoundingMode) -> (Self, Status) {
        if value.is_nan() {
            // Preserve the NaN sign (§6.3) and raise INVALID on a
            // signaling NaN operand (§5.4.2). Rust language NaNs are
            // quiet, but a bit pattern via `f64::from_bits` / FFI can be
            // signaling: among binary64 NaNs, signaling is the quiet bit
            // (mantissa MSB, bit 51) clear. Matches the siblings
            // (fd-aqs.12: the root previously dropped both).
            let nan = if value.is_sign_negative() {
                Decimal128::NAN.neg()
            } else {
                Decimal128::NAN
            };
            let status = if value.to_bits() & 0x0008_0000_0000_0000 == 0 {
                Status::INVALID
            } else {
                Status::OK
            };
            return (nan, status);
        }
        if value.is_infinite() {
            let v = if value.is_sign_negative() {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            };
            return (v, Status::OK);
        }
        if value == 0.0 {
            let v = if value.is_sign_negative() {
                Decimal128::NEG_ZERO
            } else {
                Decimal128::ZERO
            };
            return (v, Status::OK);
        }
        // Use scientific notation explicitly — `{value}` uses fixed
        // notation for moderate magnitudes which can run to 100+
        // characters for very small or very large `f64` (e.g.
        // `1e-100` → "0.0000…0001"). `{value:e}` always fits in a
        // few dozen characters. The f64 shortest-round-trip decimal is
        // ≤ 17 digits, which fits Decimal128's 34-digit coefficient
        // exactly, so `rm` drives the (normally exact) parse and the
        // returned status is the parse's. Signature matches the
        // siblings (ADR-0036).
        let mut buf = StrBuf::new();
        write!(&mut buf, "{value:e}").expect("f64 scientific format fits 64 bytes");
        match Decimal128::parse_str(buf.as_str(), rm) {
            Ok((d, status)) => (d, status),
            Err(_) => (Decimal128::NAN, Status::INVALID),
        }
    }

    /// Convert an `f32` to `Decimal128` using `rm`, returning the
    /// conversion status. Widens to `f64` and reuses
    /// [`Decimal128::from_f64`]; a signaling f32 NaN is detected from the
    /// f32 bits *before* widening (which would quiet it and drop the
    /// signal) and raises `INVALID` per §5.4.2 (fd-aqs.12).
    #[must_use]
    pub fn from_f32(value: f32, rm: RoundingMode) -> (Self, Status) {
        let (d, status) = Self::from_f64(f64::from(value), rm);
        // Restore the sNaN INVALID that `f64::from` quiets away; among
        // binary32 NaNs, signaling is the quiet bit (mantissa MSB, bit
        // 22) clear.
        if value.is_nan() && value.to_bits() & 0x0040_0000 == 0 {
            return (d, status | Status::INVALID);
        }
        (d, status)
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
        // Finite: the conversion is exact (the f64 shortest-decimal fits
        // Decimal128), so the rounding mode is immaterial and the status
        // is discarded; NearestEven matches the sibling `TryFrom`.
        Ok(Self::from_f64(value, RoundingMode::NearestEven).0)
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
        assert!(!Decimal128::from_f64(0.0, RoundingMode::NearestEven)
            .0
            .is_sign_negative());
        assert!(Decimal128::from_f64(-0.0, RoundingMode::NearestEven)
            .0
            .is_sign_negative());
    }

    #[test]
    fn from_f64_one() {
        let d = Decimal128::from_f64(1.0, RoundingMode::NearestEven).0;
        let (cmp, _) = d.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn from_f64_round_trip() {
        let cases = [1.0f64, -1.0, 1.5, -7.654_321, 6.022e23, 1e-100, 9.876e50];
        for &f in &cases {
            let d = Decimal128::from_f64(f, RoundingMode::NearestEven).0;
            let (back, _) = d.to_f64(RoundingMode::NearestEven);
            // f64 display gives shortest round-trip → same f64.
            assert_eq!(back, f, "round-trip failed for {f}");
        }
    }

    #[test]
    fn from_f64_inf_nan() {
        let rm = RoundingMode::NearestEven;
        assert!(Decimal128::from_f64(f64::INFINITY, rm).0.is_infinite());
        assert!(Decimal128::from_f64(f64::NEG_INFINITY, rm).0.is_infinite());
        assert!(Decimal128::from_f64(f64::NEG_INFINITY, rm)
            .0
            .is_sign_negative());
        assert!(Decimal128::from_f64(f64::NAN, rm).0.is_nan());
    }

    #[test]
    fn from_f32_basic() {
        let d = Decimal128::from_f32(2.5_f32, RoundingMode::NearestEven).0;
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

    #[test]
    fn to_binary_inexact_flag_is_exact() {
        // fd-aqs.12: exact conversions raise no INEXACT (the string path
        // used to raise it unconditionally, so ONE.to_f64 was INEXACT),
        // inexact ones do.
        let rm = RoundingMode::NearestEven;
        let p = |s: &str| Decimal128::parse_str(s, rm).unwrap().0;
        for s in ["1", "-1", "0.5", "0.25", "3.5", "100", "8"] {
            assert!(!p(s).to_f64(rm).1.inexact(), "{s} -> f64 exact");
            assert!(!p(s).to_f32(rm).1.inexact(), "{s} -> f32 exact");
        }
        for s in ["0.1", "-0.1", "0.3", "1.2345678901234567", "1E-30"] {
            assert!(p(s).to_f64(rm).1.inexact(), "{s} -> f64 inexact");
            assert!(p(s).to_f32(rm).1.inexact(), "{s} -> f32 inexact");
        }
        // `Decimal128::ONE` directly, the review's named witness.
        assert!(!Decimal128::ONE.to_f64(rm).1.inexact());
    }

    #[test]
    fn to_f64_is_correctly_rounded() {
        // to_f64 renders the *exact* value (Decimal128 Display is GDA
        // toSci, lossless) and parses it with the correctly-rounded
        // `f64::from_str`, so it equals the correctly-rounded conversion
        // of the same value — the precondition the fd-aqs.12 exact
        // INEXACT flag rests on. Each `s` below has ≤34 significant
        // digits, so `parse_str` holds it exactly and `s.parse::<f64>()`
        // is an independent correctly-rounded reference. (This refutes
        // the stale docstring claiming a 1-ULP shortest-decimal
        // divergence for wide values.)
        let rm = RoundingMode::NearestEven;
        for s in [
            "1.234567890123456789012345678901234",
            "9.999999999999999999999999999999999",
            "0.1",
            "0.3",
            "3.141592653589793238462643383279503",
            "2.718281828459045235360287471352662",
            "1.5",
            "1e10",
            "1e-10",
            "1234567890123456789012345678.901234",
        ] {
            let d = Decimal128::parse_str(s, rm).unwrap().0;
            let got = d.to_f64(rm).0;
            let want: f64 = s.parse().unwrap();
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "to_f64({s}) not correctly rounded"
            );
        }
    }

    #[test]
    fn from_f64_nan_sign_and_snan() {
        // fd-aqs.12: §6.3 sign preservation + §5.4.2 sNaN INVALID (the
        // root previously dropped both).
        let rm = RoundingMode::NearestEven;
        let (pos, ps) = Decimal128::from_f64(f64::NAN, rm);
        assert!(pos.is_nan() && !pos.is_sign_negative() && !ps.invalid());
        let (neg, _) = Decimal128::from_f64(-f64::NAN, rm);
        assert!(neg.is_nan() && neg.is_sign_negative());
        // A signaling-NaN bit pattern (exponent all ones, quiet bit
        // clear, payload set) raises INVALID.
        let snan = f64::from_bits(0x7ff0_0000_0000_0001);
        let (d, s) = Decimal128::from_f64(snan, rm);
        assert!(d.is_nan() && s.invalid());
    }

    #[test]
    fn from_f32_signaling_nan_raises_invalid() {
        // fd-aqs.12 follow-up: `f64::from` quiets an f32 sNaN, so
        // from_f32 must inspect the f32 bits (quiet bit 22 clear).
        let rm = RoundingMode::NearestEven;
        let (d, s) = Decimal128::from_f32(f32::from_bits(0x7f80_0001), rm);
        assert!(d.is_nan() && s.invalid(), "f32 sNaN raises INVALID");
        let (dq, sq) = Decimal128::from_f32(f32::from_bits(0x7fc0_0001), rm);
        assert!(dq.is_nan() && !sq.invalid(), "f32 qNaN does not");
    }
}
