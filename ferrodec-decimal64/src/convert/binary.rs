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

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// Lossy convert to `f64`, returning `(value, Status)`.
    ///
    /// Specials map straight through: `qNaN → f64::NAN`, `±∞ →
    /// f64::±INFINITY`, `±0 → ±0.0`. Finite values produce the f64
    /// nearest to `coef × 10^exp` (within f64's ~15.95-digit
    /// precision). Signaling NaN inputs raise `Status::INVALID` and
    /// return a quiet `f64::NAN` per IEEE 754-2019 §5.4.2.
    ///
    /// The `_rm` parameter is accepted for API parity with the
    /// `RoundingMode`-taking spec convertFormat operation but is
    /// not used internally: f64's native rounding (the FPU's
    /// round-to-nearest-even) governs the multiply step. Callers
    /// can request a different direction by formatting the
    /// Decimal64 to a string and parsing with `f64::from_str`.
    ///
    /// **API change (1.4.0)**: the previous signature
    /// `to_f64(self) -> f64` swallowed the sNaN INVALID signal
    /// silently. The new signature mirrors `ferrodec`'s Decimal128
    /// `to_f64` (commit `67bd45c`).
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
                (if sign { -magnitude } else { magnitude }, Status::OK)
            }
        }
    }

    /// Lossy convert to `f32`, returning `(value, Status)`.
    ///
    /// This takes the direct decimal-string path: format `self` with
    /// [`core::fmt::Display`] into a stack buffer, then parse that
    /// with `str::parse::<f32>` (correctly rounded). Routing through
    /// `to_f64(...).0 as f32` instead rounds twice; the f64 step can
    /// nudge a value across an f32 half-ULP boundary, the `as f32`
    /// step then rounds the wrong way, and the result misses the
    /// correctly rounded f32 by one ULP. One rounding step keeps the
    /// result inside the f32 envelope. This is the M4 fix; the
    /// previous `ToPrimitive::to_f32` delegated through f64.
    ///
    /// Specials map straight through: `qNaN → f32::NAN`, `±∞ →
    /// f32::±INFINITY`, `±0 → ±0.0`. Signaling NaN inputs raise
    /// `Status::INVALID` and return a quiet `f32::NAN` per IEEE
    /// 754-2019 §5.4.2. A finite value that overflows f32 returns
    /// `±∞` with `OVERFLOW | INEXACT`; one that underflows to zero
    /// returns `±0.0` with `UNDERFLOW | INEXACT`; otherwise the
    /// result carries `INEXACT` (exactness is not separately
    /// detected, matching [`Decimal64::to_f64`]).
    ///
    /// The `_rm` parameter is accepted for API parity with the spec
    /// convertFormat operation; f32's native round-to-nearest-even
    /// governs the parse step.
    #[must_use]
    pub fn to_f32(self, _rm: RoundingMode) -> (f32, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { .. } => (f32::NAN, Status::INVALID),
            Class::QuietNaN { .. } => (f32::NAN, Status::OK),
            Class::Infinity { sign: false } => (f32::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (f32::NEG_INFINITY, Status::OK),
            Class::Zero { sign, .. } => (if sign { -0.0 } else { 0.0 }, Status::OK),
            Class::Finite { .. } => {
                // A finite Decimal64 in Display (to-scientific-string)
                // notation is at most ~25 chars (sign, `0.`, up to
                // six leading zeros, 16 significant digits). 48 bytes
                // matches the `from_f64` path's buffer with ~2×
                // headroom.
                let mut buf = [0u8; 48];
                let mut writer = BufWriter {
                    buf: &mut buf,
                    len: 0,
                };
                if write!(writer, "{self}").is_err() {
                    // Unreachable for any finite Decimal64 at 48
                    // bytes; defensive rather than return a wrong
                    // value if a future Display widens.
                    return (f32::NAN, Status::INVALID);
                }
                let len = writer.len;
                let s = match core::str::from_utf8(&buf[..len]) {
                    Ok(s) => s,
                    // Decimal64 Display always emits ASCII.
                    Err(_) => return (f32::NAN, Status::INVALID),
                };
                match s.parse::<f32>() {
                    Ok(v) => {
                        let mut status = Status::OK;
                        if v.is_infinite() {
                            status |= Status::OVERFLOW | Status::INEXACT;
                        } else if v == 0.0 {
                            // The Finite arm excludes ±0 input, so a
                            // zero result means the magnitude rounded
                            // away: underflow.
                            status |= Status::UNDERFLOW | Status::INEXACT;
                        } else {
                            status |= Status::INEXACT;
                        }
                        (v, status)
                    }
                    // A finite Decimal64 Display always parses; treat
                    // any error as a defensive NaN + INVALID.
                    Err(_) => (f32::NAN, Status::INVALID),
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
            // IEEE 754-2019 §5.4.2: a signaling NaN operand raises
            // INVALID. Rust language-level NaNs are quiet, but a bit
            // pattern reaching here via `f64::from_bits` or FFI can be
            // signaling: among binary64 NaNs, signaling is exactly the
            // quiet bit (mantissa MSB, bit 51) clear. M3 / Agent 5 B6.
            let signaling = x.to_bits() & 0x0008_0000_0000_0000 == 0;
            return (
                Decimal64::NAN,
                if signaling {
                    Status::INVALID
                } else {
                    Status::OK
                },
            );
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

/// Error returned by [`TryFrom<f32>`] / [`TryFrom<f64>`] for
/// [`Decimal64`] when the input is not a finite number.
///
/// [`Decimal64::from_f64`] routes NaN and ±∞ to their `Decimal64`
/// counterparts (silently for quiet NaN, with `INVALID` for sNaN per
/// IEEE 754-2019 §5.4.2). The `TryFrom` impls reject those inputs
/// instead so callers expecting a finite decimal don't have to
/// re-check the result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal64FromFloatError {
    /// The input was NaN.
    NotANumber,
    /// The input was `+∞` or `−∞`.
    Infinite,
}

impl core::fmt::Display for Decimal64FromFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotANumber => f.write_str("cannot convert NaN to Decimal64"),
            Self::Infinite => f.write_str("cannot convert ±∞ to Decimal64"),
        }
    }
}

impl core::error::Error for Decimal64FromFloatError {}

impl TryFrom<f64> for Decimal64 {
    type Error = Decimal64FromFloatError;

    /// Convert a finite `f64` to `Decimal64` using `NearestEven`.
    ///
    /// NaN and ±∞ are rejected; finite values flow through
    /// [`Decimal64::from_f64`] at `NearestEven` (the `Status` from
    /// the underlying conversion is discarded, matching the parent's
    /// `TryFrom` shape — callers needing the status should call
    /// `from_f64` directly).
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_nan() {
            return Err(Decimal64FromFloatError::NotANumber);
        }
        if value.is_infinite() {
            return Err(Decimal64FromFloatError::Infinite);
        }
        let (d, _) = Self::from_f64(value, RoundingMode::NearestEven);
        Ok(d)
    }
}

impl TryFrom<f32> for Decimal64 {
    type Error = Decimal64FromFloatError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::try_from(f64::from(value))
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
        assert_eq!(Decimal64::ZERO.to_f64(RoundingMode::NearestEven).0, 0.0);
        assert!(Decimal64::NEG_ZERO
            .to_f64(RoundingMode::NearestEven)
            .0
            .is_sign_negative());
        assert_eq!(Decimal64::ONE.to_f64(RoundingMode::NearestEven).0, 1.0);
        assert_eq!(Decimal64::NEG_ONE.to_f64(RoundingMode::NearestEven).0, -1.0);
        assert_eq!(from_int(15, -1).to_f64(RoundingMode::NearestEven).0, 1.5);
        assert_eq!(from_int(-2, 0).to_f64(RoundingMode::NearestEven).0, -2.0);
    }

    #[test]
    fn to_f64_specials() {
        assert!(Decimal64::NAN.to_f64(RoundingMode::NearestEven).0.is_nan());
        assert_eq!(
            Decimal64::INFINITY.to_f64(RoundingMode::NearestEven).0,
            f64::INFINITY
        );
        assert_eq!(
            Decimal64::NEG_INFINITY.to_f64(RoundingMode::NearestEven).0,
            f64::NEG_INFINITY
        );
    }

    #[test]
    fn to_f64_signaling_nan_raises_invalid() {
        // IEEE 754-2019 §5.4.2: convertFormat on a signaling NaN
        // raises INVALID and yields a quiet NaN. A quiet NaN passes
        // through clean. Mirrors Decimal128 commit 67bd45c.
        let (v, status) = Decimal64::SIGNALING_NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(status, Status::INVALID);

        let (v, status) = Decimal64::NAN.to_f64(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(status, Status::OK);
    }

    #[test]
    fn to_f32_specials() {
        // Mirror to_f64_specials / to_f64_signaling_nan_raises_invalid
        // on the new (f32, Status) signature.
        let (v, s) = Decimal64::SIGNALING_NAN.to_f32(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(s, Status::INVALID);

        let (v, s) = Decimal64::NAN.to_f32(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(s, Status::OK);

        let (v, _) = Decimal64::INFINITY.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, f32::INFINITY);
        let (v, _) = Decimal64::NEG_INFINITY.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, f32::NEG_INFINITY);

        let (v, _) = Decimal64::ZERO.to_f32(RoundingMode::NearestEven);
        assert_eq!(v.to_bits(), 0u32);
        let (v, _) = Decimal64::NEG_ZERO.to_f32(RoundingMode::NearestEven);
        assert_eq!(v.to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn to_f32_is_correctly_rounded_not_double_rounded() {
        // The correctly rounded f32 of an exact decimal equals
        // parsing that decimal straight into f32 (str → f32 is
        // correctly rounded). Going through f64 first rounds twice
        // and misses by one ULP on half-ULP boundary cases. 8589973000
        // is the classic pet case: the first half-ULP in [2^33, 2^34),
        // where the f64 intermediate nudges the result across the f32
        // boundary. The bit-exact comparison would fail on the old
        // `to_f64(..) as f32` path.
        for s in [
            "1",
            "-1",
            "3.5",
            "0.1",
            "-0.1",
            "1.234567890123456",
            "1E-30",
            "1E+30",
            "8589973000",
        ] {
            let d = Decimal64::parse_str(s, RoundingMode::NearestEven)
                .unwrap()
                .0;
            let (got, status) = d.to_f32(RoundingMode::NearestEven);
            let want: f32 = s.parse().expect("decimal literal parses as f32");
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "to_f32({s}): got {got:?}, want correctly-rounded {want:?}"
            );
            assert!(status.inexact(), "finite to_f32 carries INEXACT");
        }
    }

    #[test]
    fn to_f32_overflow_and_underflow() {
        // 1E+100 is inside Decimal64's range (E_MAX 384) but far
        // above f32::MAX, so it overflows to ±∞ with OVERFLOW.
        let big = Decimal64::parse_str("1E+100", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (v, s) = big.to_f32(RoundingMode::NearestEven);
        assert!(v.is_infinite() && !v.is_sign_negative());
        assert!(s.overflow() && s.inexact());

        // 1E-100 is representable in Decimal64 but rounds to zero in
        // f32 (below the f32 subnormal floor), raising UNDERFLOW.
        let small = Decimal64::parse_str("1E-100", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (v, s) = small.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, 0.0_f32);
        assert!(s.underflow() && s.inexact());
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
    fn from_f64_signaling_bit_pattern_raises_invalid() {
        // M3 / Agent 5 B6. A quiet f64 NaN is OK; a signaling bit
        // pattern (quiet bit clear, mantissa non-zero) raises INVALID
        // per IEEE 754-2019 §5.4.2.
        let (d, s) = Decimal64::from_f64(f64::NAN, RoundingMode::NearestEven);
        assert!(d.is_quiet_nan());
        assert_eq!(s, Status::OK);

        let snan = f64::from_bits(0x7FF0_0000_0000_0001);
        assert!(snan.is_nan());
        let (d, s) = Decimal64::from_f64(snan, RoundingMode::NearestEven);
        assert!(d.is_quiet_nan());
        assert_eq!(s, Status::INVALID);

        // Negative signaling pattern too.
        let neg_snan = f64::from_bits(0xFFF0_0000_0000_0002);
        let (_, s) = Decimal64::from_f64(neg_snan, RoundingMode::NearestEven);
        assert_eq!(s, Status::INVALID);
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
            let parsed = Decimal64::parse_str(s, RoundingMode::NearestEven)
                .unwrap()
                .0;
            let as_f64 = parsed.to_f64(RoundingMode::NearestEven).0;
            let (back, _) = Decimal64::from_f64(as_f64, RoundingMode::NearestEven);
            // Numerically equal (cohort may differ).
            assert_eq!(
                parsed.partial_cmp(back).0,
                Some(core::cmp::Ordering::Equal),
                "round-trip failed for {s}"
            );
        }
    }

    // ---- TryFrom<f64> / TryFrom<f32> ------------------------------------

    #[test]
    fn try_from_f64_finite_succeeds() {
        // The TryFrom contract: finite inputs accept and route through
        // `from_f64` at `NearestEven`. The exact round-trip back to f64
        // is governed by `to_f64`'s native precision (the naive `coef ·
        // pow10_f64(exp)` path can drift by 1 ULP on values like 1.5),
        // which is out of scope here. Verify only what `TryFrom` adds:
        // a finite f64 maps to a finite Decimal64 equal to the
        // `from_f64`-direct result.
        let d = Decimal64::try_from(1.5_f64).unwrap();
        let (direct, _) = Decimal64::from_f64(1.5_f64, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), direct.to_bits());
        assert!(d.is_finite());
        assert!(!d.is_zero());
    }

    #[test]
    fn try_from_f64_nan_rejects() {
        assert_eq!(
            Decimal64::try_from(f64::NAN),
            Err(Decimal64FromFloatError::NotANumber)
        );
    }

    #[test]
    fn try_from_f64_infinity_rejects() {
        assert_eq!(
            Decimal64::try_from(f64::INFINITY),
            Err(Decimal64FromFloatError::Infinite)
        );
        assert_eq!(
            Decimal64::try_from(f64::NEG_INFINITY),
            Err(Decimal64FromFloatError::Infinite)
        );
    }

    #[test]
    fn try_from_f64_zero_succeeds() {
        let pos = Decimal64::try_from(0.0_f64).unwrap();
        let neg = Decimal64::try_from(-0.0_f64).unwrap();
        assert!(pos.is_zero());
        assert!(neg.is_zero());
        assert!(neg.is_sign_negative());
        assert!(!pos.is_sign_negative());
    }

    #[test]
    fn try_from_f32_routes_through_f64() {
        // f32 path widens to f64 and reuses the f64 impl.
        let d = Decimal64::try_from(1.5_f32).unwrap();
        let (direct, _) = Decimal64::from_f64(f64::from(1.5_f32), RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), direct.to_bits());
        assert!(d.is_finite());
        assert_eq!(
            Decimal64::try_from(f32::NAN),
            Err(Decimal64FromFloatError::NotANumber)
        );
        assert_eq!(
            Decimal64::try_from(f32::INFINITY),
            Err(Decimal64FromFloatError::Infinite)
        );
    }

    #[test]
    fn from_float_error_display() {
        // Render through Display into the local BufWriter and confirm
        // the message text. Keeps the test no_std-clean by avoiding
        // `alloc::format!`.
        let mut buf = [0u8; 64];
        let mut w = BufWriter {
            buf: &mut buf,
            len: 0,
        };
        core::write!(w, "{}", Decimal64FromFloatError::NotANumber).unwrap();
        let len = w.len;
        assert_eq!(
            core::str::from_utf8(&buf[..len]).unwrap(),
            "cannot convert NaN to Decimal64"
        );

        let mut buf2 = [0u8; 64];
        let mut w2 = BufWriter {
            buf: &mut buf2,
            len: 0,
        };
        core::write!(w2, "{}", Decimal64FromFloatError::Infinite).unwrap();
        let len2 = w2.len;
        assert_eq!(
            core::str::from_utf8(&buf2[..len2]).unwrap(),
            "cannot convert ±∞ to Decimal64"
        );
    }
}
