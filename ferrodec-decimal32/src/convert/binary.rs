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

    /// Lossy convert to `f32`, returning `(value, Status)`.
    ///
    /// This takes the direct decimal-string path: format `self` with
    /// [`core::fmt::Display`] into a stack buffer, then parse that
    /// with `str::parse::<f32>`, which is correctly rounded. One
    /// rounding step yields the single correctly-rounded decimal to
    /// binary32 value the IEEE 754-2019 §5.4.2 convertFormat
    /// operation prescribes. The previous `ToPrimitive::to_f32`
    /// delegated through `to_f64(...).0 as f32`; for Decimal32 that
    /// route happens to agree numerically, since the 7-digit
    /// coefficient fits f64's 53-bit significand exactly and
    /// [`Decimal32::to_f64`] is exact, but it swallowed the
    /// signaling-NaN signal and dropped the `Status` channel. The
    /// direct path restores both. A general decimal to binary32
    /// conversion through a binary64 intermediate rounds twice and
    /// can miss the correctly rounded f32 by one ULP; this path
    /// avoids that class of error structurally rather than by
    /// relying on the coefficient-width coincidence.
    ///
    /// Specials map straight through: `qNaN → f32::NAN`, `±∞ →
    /// f32::±INFINITY`, `±0 → ±0.0`. Signaling NaN inputs raise
    /// `Status::INVALID` and return a quiet `f32::NAN` per IEEE
    /// 754-2019 §5.4.2 and §7.2 invalid operation. A finite value
    /// that overflows f32 returns `±∞` with `OVERFLOW | INEXACT`; one
    /// that underflows to zero returns `±0.0` with `UNDERFLOW |
    /// INEXACT`; otherwise the result carries `INEXACT` (exactness is
    /// not separately detected, matching [`Decimal32::to_f64`]).
    ///
    /// The `_rm` parameter is accepted for API parity with the spec
    /// convertFormat operation; f32's native round-to-nearest-even
    /// governs the parse step.
    ///
    /// API change: this method is new, and the `ToPrimitive::to_f32`
    /// delegate now routes through it rather than through f64. This is
    /// a breaking change for downstream callers that depended on the
    /// old double-rounded bit pattern.
    #[must_use]
    pub fn to_f32(self, _rm: RoundingMode) -> (f32, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { .. } => (f32::NAN, Status::INVALID),
            Class::QuietNaN { .. } => (f32::NAN, Status::OK),
            Class::Infinity { sign: false } => (f32::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (f32::NEG_INFINITY, Status::OK),
            Class::Zero { sign, .. } => (if sign { -0.0 } else { 0.0 }, Status::OK),
            Class::Finite { .. } => {
                // A finite Decimal32 in Display notation is short. The
                // 7-digit coefficient plus sign, decimal point, and an
                // `E±NN` exponent (E_MAX 96, so at most two exponent
                // digits) is ~13 chars in scientific form; the plain
                // form for tiny magnitudes is sign + `0.` + up to six
                // leading zeros + seven digits, ~16 chars. A 32-byte
                // buffer is over 2× the worst case, so the write
                // cannot overflow on any libcore version we know of.
                let mut buf = [0u8; 32];
                let mut writer = BufWriter {
                    buf: &mut buf,
                    len: 0,
                };
                if write!(writer, "{self}").is_err() {
                    // Unreachable for any finite Decimal32 at 32
                    // bytes; defensive rather than return a wrong
                    // value if a future Display widens.
                    return (f32::NAN, Status::INVALID);
                }
                let len = writer.len;
                let s = match core::str::from_utf8(&buf[..len]) {
                    Ok(s) => s,
                    // Decimal32 Display always emits ASCII.
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
                    // A finite Decimal32 Display always parses; treat
                    // any error as a defensive NaN + INVALID.
                    Err(_) => (f32::NAN, Status::INVALID),
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
            // IEEE 754-2019 §5.4.2: a signaling NaN operand raises
            // INVALID. Rust language level NaNs are quiet, but a bit
            // pattern reaching here through `f64::from_bits` or FFI
            // can be signaling: among binary64 NaNs, signaling is
            // exactly the quiet bit (mantissa MSB, bit 51) clear.
            // M3, the Decimal64 M3 shape.
            let signaling = x.to_bits() & 0x0008_0000_0000_0000 == 0;
            return (
                Decimal32::NAN,
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

    /// Construct a `Decimal32` from an `f32`, rounding by `rm`.
    ///
    /// Widens to `f64` and reuses [`Decimal32::from_f64`]; the widening
    /// is exact (every `f32` is representable in `f64`), so no precision
    /// is lost before the decimal rounding step. Returns `(value,
    /// Status)` with the same special-case handling as `from_f64`
    /// (signaling NaN bit patterns raise `INVALID`).
    #[must_use]
    pub fn from_f32(x: f32, rm: RoundingMode) -> (Self, Status) {
        Self::from_f64(f64::from(x), rm)
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
/// [`Decimal32`] when the input is not a finite number.
///
/// [`Decimal32::from_f64`] routes NaN and ±∞ to their `Decimal32`
/// counterparts (silently for quiet NaN, with `INVALID` for sNaN per
/// IEEE 754-2019 §5.4.2). The `TryFrom` impls reject those inputs
/// instead so callers expecting a finite decimal don't have to
/// re-check the result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal32FromFloatError {
    /// The input was NaN.
    NotANumber,
    /// The input was `+∞` or `−∞`.
    Infinite,
}

impl core::fmt::Display for Decimal32FromFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotANumber => f.write_str("cannot convert NaN to Decimal32"),
            Self::Infinite => f.write_str("cannot convert ±∞ to Decimal32"),
        }
    }
}

impl core::error::Error for Decimal32FromFloatError {}

impl TryFrom<f64> for Decimal32 {
    type Error = Decimal32FromFloatError;

    /// Convert a finite `f64` to `Decimal32` using `NearestEven`.
    ///
    /// NaN and ±∞ are rejected; finite values flow through
    /// [`Decimal32::from_f64`] at `NearestEven` (the `Status` from
    /// the underlying conversion is discarded — callers needing the
    /// status should call `from_f64` directly). Very-large finite
    /// f64 magnitudes saturate to `±∞` per the standard f64 → decimal
    /// conversion behaviour; the caller must check `is_finite` if
    /// that distinction matters.
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_nan() {
            return Err(Decimal32FromFloatError::NotANumber);
        }
        if value.is_infinite() {
            return Err(Decimal32FromFloatError::Infinite);
        }
        let (d, _) = Self::from_f64(value, RoundingMode::NearestEven);
        Ok(d)
    }
}

impl TryFrom<f32> for Decimal32 {
    type Error = Decimal32FromFloatError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::try_from(f64::from(value))
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
    fn to_f32_specials() {
        // Mirror to_f64_specials / to_f64_signaling_nan_raises_invalid
        // on the new (f32, Status) signature.
        let (v, s) = Decimal32::SIGNALING_NAN.to_f32(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(s, Status::INVALID);

        let (v, s) = Decimal32::NAN.to_f32(RoundingMode::NearestEven);
        assert!(v.is_nan());
        assert_eq!(s, Status::OK);

        let (v, _) = Decimal32::INFINITY.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, f32::INFINITY);
        let (v, _) = Decimal32::NEG_INFINITY.to_f32(RoundingMode::NearestEven);
        assert_eq!(v, f32::NEG_INFINITY);

        let (v, _) = Decimal32::ZERO.to_f32(RoundingMode::NearestEven);
        assert_eq!(v.to_bits(), 0u32);
        let (v, _) = Decimal32::NEG_ZERO.to_f32(RoundingMode::NearestEven);
        assert_eq!(v.to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn to_f32_is_correctly_rounded() {
        // `to_f32` must yield the single correctly rounded binary32
        // value of the exact decimal (IEEE 754-2019 §5.4.2). The
        // reference for "correctly rounded" is Rust's `str → f32`,
        // which is itself correctly rounded, so parsing the same
        // decimal literal gives the value `to_f32` must match
        // bit-for-bit.
        //
        // 7038531E-32 is the witness worth calling out. It is a
        // representable Decimal32 (coefficient 7_038_531 ≤ 9_999_999,
        // adjusted exponent inside the encoded range). Its exact
        // value 7038531 × 10^-32 lies just below the f32 midpoint
        // between 0x15ae43fd and 0x15ae43fe, so the correctly rounded
        // f32 is 0x15ae43fd. Parsing the decimal straight into f64
        // and then casting (decimal → f64 via str → f32) rounds twice
        // and lands on 0x15ae43fe, one ULP high. This `to_f32` takes
        // the direct decimal-string path and lands on 0x15ae43fd.
        // Independent witness: comparing the true value against the
        // midpoint, 7038531 < mid × 10^32 (= 7038531.000000001), so it
        // rounds toward the smaller candidate.
        for s in [
            "1",
            "-1",
            "3.5",
            "0.1",
            "-0.1",
            "1234567",
            "1E-30",
            "1E+30",
            "7038531E-32",
        ] {
            let d = Decimal32::parse_str(s, RoundingMode::NearestEven)
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

        // Pin the correctly rounded bit pattern of the 7038531E-32
        // witness explicitly.
        let d = Decimal32::parse_str("7038531E-32", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (direct, _) = d.to_f32(RoundingMode::NearestEven);
        assert_eq!(direct.to_bits(), 0x15ae_43fd);

        // No 7-digit Decimal32 exposes a decimal → f32 vs
        // `to_f64(..) as f32` divergence: unlike Decimal64, whose
        // 16-digit coefficients exceed f64 precision, every
        // representable Decimal32 has a 7-digit coefficient that f64's
        // 53-bit significand holds exactly, so [`Decimal32::to_f64`]
        // returns the exact value and `as f32` then rounds that exact
        // value once. The H7 fix still matters: it removes the lossy
        // `as f32` cast from `ToPrimitive`, restores the signaling-NaN
        // INVALID signal the old path swallowed, and gives a
        // `(f32, Status)` surface. An exhaustive sweep of the full
        // 7-digit coefficient × exponent grid (offline) found zero
        // bit-pattern divergences between the two routes, which is why
        // this test pins correct rounding directly rather than against
        // the f64 route.
        let via_to_f64 = d.to_f64(RoundingMode::NearestEven).0 as f32;
        assert_eq!(
            direct.to_bits(),
            via_to_f64.to_bits(),
            "Decimal32 to_f64 is exact, so the routes agree here"
        );
    }

    #[test]
    fn to_f32_overflow_and_underflow() {
        // Decimal32::MAX (9.999999E+96) is far above f32::MAX (~3.4E38),
        // so it overflows to +∞ with OVERFLOW | INEXACT.
        let (v, s) = Decimal32::MAX.to_f32(RoundingMode::NearestEven);
        assert!(v.is_infinite() && !v.is_sign_negative());
        assert!(s.overflow() && s.inexact());

        // 1E-60 is representable in Decimal32 but rounds to zero in f32
        // (below the f32 subnormal floor ~1.4E-45), raising UNDERFLOW.
        let small = Decimal32::parse_str("1E-60", RoundingMode::NearestEven)
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
    fn from_f64_signaling_nan_raises_invalid() {
        // M3: a binary64 signaling NaN (quiet bit, bit 51, clear)
        // raises INVALID per IEEE 754-2019 §5.4.2.
        let snan = f64::from_bits(0x7FF4_0000_0000_0000);
        assert!(snan.is_nan());
        let (d, s) = Decimal32::from_f64(snan, RoundingMode::NearestEven);
        assert!(d.is_nan() && s.invalid());
        // A quiet NaN (quiet bit set) passes through with OK.
        let (d, s) = Decimal32::from_f64(f64::NAN, RoundingMode::NearestEven);
        assert!(d.is_nan() && s.is_ok());
        let qnan = f64::from_bits(0x7FF8_0000_0000_0001);
        let (d, s) = Decimal32::from_f64(qnan, RoundingMode::NearestEven);
        assert!(d.is_nan() && s.is_ok());
    }

    #[test]
    fn from_f32_widens_through_f64() {
        // from_f32 widens exactly to f64 then reuses from_f64, so the
        // result is bit-identical to calling from_f64 on the widened
        // value.
        let (d, _) = Decimal32::from_f32(2.5_f32, RoundingMode::NearestEven);
        let (direct, _) = Decimal32::from_f64(f64::from(2.5_f32), RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), direct.to_bits());
        assert_eq!(
            d.partial_cmp(from_int(25, -1)).0,
            Some(core::cmp::Ordering::Equal)
        );

        // Specials route through identically.
        let (n, _) = Decimal32::from_f32(f32::NAN, RoundingMode::NearestEven);
        assert!(n.is_quiet_nan());
        let (i, _) = Decimal32::from_f32(f32::INFINITY, RoundingMode::NearestEven);
        assert!(i.is_infinite() && !i.is_sign_negative());
        let (z, _) = Decimal32::from_f32(-0.0_f32, RoundingMode::NearestEven);
        assert_eq!(z.to_bits(), Decimal32::NEG_ZERO.to_bits());
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

    // ---- TryFrom<f64> / TryFrom<f32> ------------------------------------

    #[test]
    fn try_from_f64_finite_succeeds() {
        // The TryFrom contract: finite inputs accept and route through
        // `from_f64` at `NearestEven`. Round-trip back to f64 is
        // governed by `to_f64`'s native precision, out of scope here.
        let d = Decimal32::try_from(1.5_f64).unwrap();
        let (direct, _) = Decimal32::from_f64(1.5_f64, RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), direct.to_bits());
        assert!(d.is_finite());
        assert!(!d.is_zero());
    }

    #[test]
    fn try_from_f64_nan_rejects() {
        assert_eq!(
            Decimal32::try_from(f64::NAN),
            Err(Decimal32FromFloatError::NotANumber)
        );
    }

    #[test]
    fn try_from_f64_infinity_rejects() {
        assert_eq!(
            Decimal32::try_from(f64::INFINITY),
            Err(Decimal32FromFloatError::Infinite)
        );
        assert_eq!(
            Decimal32::try_from(f64::NEG_INFINITY),
            Err(Decimal32FromFloatError::Infinite)
        );
    }

    #[test]
    fn try_from_f64_zero_succeeds() {
        let pos = Decimal32::try_from(0.0_f64).unwrap();
        let neg = Decimal32::try_from(-0.0_f64).unwrap();
        assert!(pos.is_zero());
        assert!(neg.is_zero());
        assert!(neg.is_sign_negative());
        assert!(!pos.is_sign_negative());
    }

    #[test]
    fn try_from_f32_routes_through_f64() {
        // f32 path widens to f64 and reuses the f64 impl.
        let d = Decimal32::try_from(1.5_f32).unwrap();
        let (direct, _) = Decimal32::from_f64(f64::from(1.5_f32), RoundingMode::NearestEven);
        assert_eq!(d.to_bits(), direct.to_bits());
        assert!(d.is_finite());
        assert_eq!(
            Decimal32::try_from(f32::NAN),
            Err(Decimal32FromFloatError::NotANumber)
        );
        assert_eq!(
            Decimal32::try_from(f32::INFINITY),
            Err(Decimal32FromFloatError::Infinite)
        );
    }

    #[test]
    fn from_float_error_display() {
        let mut buf = [0u8; 64];
        let mut w = BufWriter {
            buf: &mut buf,
            len: 0,
        };
        core::write!(w, "{}", Decimal32FromFloatError::NotANumber).unwrap();
        let len = w.len;
        assert_eq!(
            core::str::from_utf8(&buf[..len]).unwrap(),
            "cannot convert NaN to Decimal32"
        );

        let mut buf2 = [0u8; 64];
        let mut w2 = BufWriter {
            buf: &mut buf2,
            len: 0,
        };
        core::write!(w2, "{}", Decimal32FromFloatError::Infinite).unwrap();
        let len2 = w2.len;
        assert_eq!(
            core::str::from_utf8(&buf2[..len2]).unwrap(),
            "cannot convert ±∞ to Decimal32"
        );
    }
}
