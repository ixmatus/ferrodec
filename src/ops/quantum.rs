//! IEEE 754-2019 quantum-related operations (§5.3, §5.10):
//! `quantize`, `same_quantum`, `scaleb`, `logb`, `next_up`, `next_down`,
//! `compare_total_magnitude`, `radix`.
//!
//! These operations deal with the *quantum* exponent of a `Decimal128` — the
//! power of ten that the stored coefficient is multiplied by. For a BID-128
//! value `c × 10^q`, `q` is the quantum exponent (`biased_exp − BIAS`).

use core::cmp::Ordering;

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, Class, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::nan_from;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};
use ferrodec_ieee::should_round_up;

/// GDA `scaleb` second-operand magnitude limit: `2 × (E_max + precision)`.
/// For Decimal128: `2 × (6144 + 34) = 12356`. Anything beyond this raises
/// `INVALID` regardless of the first operand's value.
const SCALEB_N_LIMIT: u32 = 2 * (6144 + 34);

impl Decimal128 {
    /// IEEE 754-2019 §5.3.4 `quantize(x, y, rm)`.
    ///
    /// Returns a value numerically equivalent to `self`, rounded using `rm`,
    /// with the same quantum exponent as `target`. Raises `INVALID` if the
    /// rescaled coefficient would require more than 34 decimal digits.
    ///
    /// Special cases:
    /// * Either operand is NaN → NaN (INVALID if signaling).
    /// * `self` is ±∞ and `target` is ±∞ → `self`.
    /// * Any other Inf/finite mismatch → NaN + INVALID.
    ///
    /// See [`Decimal128::same_quantum`] for the predicate test.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::{Decimal128, RoundingMode};
    ///
    /// let x = Decimal128::try_new(1234, -3).unwrap();   // 1.234
    /// let target = Decimal128::try_new(1, -2).unwrap(); // 0.01 quantum
    /// let (r, st) = x.quantize(target, RoundingMode::NearestEven);
    /// assert!(r.same_quantum(target));
    /// assert!(st.inexact());
    /// ```
    #[must_use]
    pub fn quantize(self, target: Self, rm: RoundingMode) -> (Self, Status) {
        let snan = self.is_signaling_nan() || target.is_signaling_nan();
        if self.is_nan() || target.is_nan() {
            // Signaling-NaN priority, then operand order (self, target),
            // matching the arithmetic propagation and decTest (e.g.
            // dqqua675 `quantize NaN95 sNaN93 -> NaN93`). `nan_from`
            // preserves the chosen operand's sign and payload and
            // quietens it; a signaling operand additionally raises
            // INVALID.
            let src = if self.is_signaling_nan() {
                self
            } else if target.is_signaling_nan() {
                target
            } else if self.is_nan() {
                self
            } else {
                target
            };
            return (
                nan_from(src),
                if snan { Status::INVALID } else { Status::OK },
            );
        }
        if self.is_infinite() {
            return if target.is_infinite() {
                (self, Status::OK)
            } else {
                (Self::NAN, Status::INVALID)
            };
        }
        if target.is_infinite() {
            return (Self::NAN, Status::INVALID);
        }

        // Both finite (including zero).
        let (sign, self_bexp, self_coef) = decode_finite(self);
        let (_, tgt_bexp, _) = decode_finite(target);

        if tgt_bexp == self_bexp {
            return (
                Self::from_bits(pack_finite(sign, tgt_bexp, self_coef)),
                Status::OK,
            );
        }

        if tgt_bexp < self_bexp {
            // Target exponent is lower → multiply coefficient by 10^delta.
            let delta = self_bexp - tgt_bexp;
            if self_coef == 0 {
                return (Self::from_bits(pack_finite(sign, tgt_bexp, 0)), Status::OK);
            }
            let new_digits = decimal_digit_count(self_coef) + delta;
            if new_digits > 34 {
                return (Self::NAN, Status::INVALID);
            }
            // delta ≤ 33 here (since new_digits ≤ 34 and digit_count ≥ 1).
            let new_coef = self_coef * pow10(delta);
            return (
                Self::from_bits(pack_finite(sign, tgt_bexp, new_coef)),
                Status::OK,
            );
        }

        // Target exponent is higher → divide coefficient by 10^delta, rounding.
        let delta = tgt_bexp - self_bexp;
        if self_coef == 0 {
            return (Self::from_bits(pack_finite(sign, tgt_bexp, 0)), Status::OK);
        }
        let (kept, round_digit, sticky) = divide_by_pow10(self_coef, delta);
        let inexact = round_digit != 0 || sticky;
        let last_lsb = (kept % 10) as u32;
        let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
        let new_coef = kept + u128::from(round_up);
        // Dividing then rounding +1 can never reach COEFFICIENT_LIMIT.
        debug_assert!(new_coef < COEFFICIENT_LIMIT);
        let status = if inexact { Status::INEXACT } else { Status::OK };
        (
            Self::from_bits(pack_finite(sign, tgt_bexp, new_coef)),
            status,
        )
    }

    /// Return `true` if `self` and `other` have the same quantum exponent.
    ///
    /// * Both NaN (any kind) → `true`.
    /// * Both ±∞ → `true`.
    /// * NaN with non-NaN, or Inf with finite/zero → `false`.
    /// * Two finite or zero values → `true` iff their biased exponents match.
    ///
    /// No status flags are raised.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // 1.23 and 4.56 share quantum 10^-2.
    /// let a = Decimal128::try_new(123, -2).unwrap();
    /// let b = Decimal128::try_new(456, -2).unwrap();
    /// assert!(a.same_quantum(b));
    ///
    /// // 1.2 has quantum 10^-1 — different cohort.
    /// let c = Decimal128::try_new(12, -1).unwrap();
    /// assert!(!a.same_quantum(c));
    /// ```
    #[inline]
    #[must_use]
    pub fn same_quantum(self, other: Self) -> bool {
        use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};
        match (classify_bits(self.0), classify_bits(other.0)) {
            (QuietNaN { .. } | SignalingNaN { .. }, QuietNaN { .. } | SignalingNaN { .. }) => true,
            (Infinity { .. }, Infinity { .. }) => true,
            (
                Zero { biased_exp: a, .. } | Finite { biased_exp: a, .. },
                Zero { biased_exp: b, .. } | Finite { biased_exp: b, .. },
            ) => a == b,
            _ => false,
        }
    }

    /// IEEE 754-2019 §5.3.2 `scaleB(x, n, rm)`.
    ///
    /// Returns `self × 10^n` using `rm` for any rounding that becomes
    /// necessary at the exponent boundaries. For finite `self` where the
    /// result exponent is in range, the operation is always exact (no
    /// rounding occurs). Overflow and underflow are detected and flagged.
    ///
    /// `|n|` is bounded by `2 × (E_max + precision) = 12356`; values
    /// outside that range raise `INVALID` per the General Decimal
    /// Arithmetic spec.
    ///
    /// Signaling NaN raises `INVALID`; quiet NaN and ±∞ pass through.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::{Decimal128, RoundingMode};
    ///
    /// // 7 × 10^3 = 7000 (exact, quantum shifted by +3).
    /// let seven = Decimal128::try_new(7, 0).unwrap();
    /// let (r, st) = seven.scaleb(3, RoundingMode::NearestEven);
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::try_new(7, 3).unwrap().to_bits());
    /// ```
    #[must_use]
    pub fn scaleb(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if self.is_nan() {
            return (self, Status::OK);
        }
        // GDA §5.3: rhs whose magnitude exceeds 2 × (E_max + precision)
        // is treated as a non-representable scale and raises INVALID.
        // Decimal128 → 2 × (6144 + 34) = 12356. The check precedes the
        // ±∞ pass-through so e.g. `scaleb(+∞, 1_000_000)` is invalid,
        // matching libdfp behaviour.
        if n.unsigned_abs() > SCALEB_N_LIMIT {
            return (Self::NAN, Status::INVALID);
        }
        if self.is_infinite() {
            return (self, Status::OK);
        }
        let (sign, bexp, coef) = match classify_bits(self.0) {
            Class::Zero { sign, biased_exp } => {
                // Zero: shift the quantum, clamped to the storable range. If
                // the clamp moved the quantum, raise §7.4 Clamped: the zero
                // is exact at every exponent (fd-61r / ADR-0048).
                let shifted = biased_exp as i64 + n as i64;
                let new_bexp = shifted.clamp(0, BIASED_EXP_MAX as i64);
                let status = if new_bexp == shifted {
                    Status::OK
                } else {
                    Status::CLAMPED
                };
                return (
                    Self::from_bits(pack_finite(sign, new_bexp as u32, 0)),
                    status,
                );
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!(),
        };

        // Compute in i64 to avoid i32 overflow for extreme n.
        let new_unbiased: i64 = bexp as i64 - BIAS as i64 + n as i64;
        // Clamp to a range that round_and_pack_finite can handle; anything
        // beyond [-10_000, 10_000] is guaranteed overflow or underflow.
        let clamped = new_unbiased.clamp(-10_000, 10_000) as i32;
        round_and_pack_finite(
            U256::from_u128(coef),
            clamped,
            clamped,
            sign,
            false,
            rm,
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.3 `logB(x)`.
    ///
    /// Returns `floor(log10(|x|))` as an exact integer `Decimal128`.
    ///
    /// * `logb(±∞)` → `+∞`.
    /// * `logb(±0)` → `−∞` with `DIV_BY_ZERO` raised.
    /// * `logb(qNaN)` → the NaN propagated unchanged.
    /// * `logb(sNaN)` → quiet NaN + `INVALID`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // floor(log10(10)) = 1.
    /// let (r, st) = Decimal128::TEN.logb();
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    /// ```
    #[must_use]
    pub fn logb(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if self.is_nan() {
            return (self, Status::OK);
        }
        if self.is_infinite() {
            return (Self::INFINITY, Status::OK);
        }
        match classify_bits(self.0) {
            Class::Zero { .. } => (Self::NEG_INFINITY, Status::DIV_BY_ZERO),
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let digits = decimal_digit_count(coefficient) as i32;
                let unbiased = biased_exp as i32 - BIAS as i32;
                // adjusted_exp = floor(log10(|x|)) ∈ [−6176, 6144]
                let adjusted_exp = digits - 1 + unbiased;
                let (neg, mag) = if adjusted_exp < 0 {
                    (true, (-adjusted_exp) as u128)
                } else {
                    (false, adjusted_exp as u128)
                };
                (Self::from_bits(pack_finite(neg, BIAS, mag)), Status::OK)
            }
            _ => unreachable!(),
        }
    }

    /// IEEE 754-2019 §5.3.1 `nextUp(x)`.
    ///
    /// The smallest representable `Decimal128` that compares greater than
    /// `self` in the numeric sense.
    ///
    /// * `next_up(−∞)` → `Decimal128::MIN`.
    /// * `next_up(±0)` → `Decimal128::MIN_POSITIVE`.
    /// * `next_up(+∞)` → `+∞`.
    /// * `next_up(sNaN)` → quiet NaN + `INVALID`.
    /// * `next_up(qNaN)` → the NaN unchanged.
    ///
    /// The result's quantum exponent is the lowest one in which `self`'s
    /// numeric value can be represented at full 34-digit precision (or 0
    /// for subnormals). This matches IEEE / GDA: every adjacent value is
    /// reachable in a single step, regardless of which cohort `self` was
    /// encoded in.
    ///
    /// See [`Decimal128::next_down`] for the symmetric op.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // The smallest value strictly greater than ±0 is MIN_POSITIVE.
    /// let (r, st) = Decimal128::ZERO.next_up();
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::MIN_POSITIVE.to_bits());
    /// ```
    #[must_use]
    pub fn next_up(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if self.is_nan() {
            return (self, Status::OK);
        }
        if self.is_zero() {
            return (Self::MIN_POSITIVE, Status::OK);
        }
        if self.is_infinite() {
            // Includes non-canonical infinity encodings (type field
            // 0b11110, trailing bits non-zero). Bit-equality against
            // `Self::INFINITY` would miss those and the `unreachable!()`
            // below would fire on a Form-A Inf with junk bits.
            return (
                if self.is_sign_negative() {
                    Self::MIN
                } else {
                    Self::INFINITY
                },
                Status::OK,
            );
        }
        let (sign, bexp, coef) = match classify_bits(self.0) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!(),
        };
        // Renormalise to the lowest representable cohort: maximum
        // coefficient width (34 digits), bounded below by biased_exp = 0
        // (subnormal floor). Without this, an in-cohort ±1 step skips the
        // adjacent value when `self` was stored at a higher quantum.
        let digits = decimal_digit_count(coef);
        let expand = (34u32 - digits).min(bexp);
        let new_coef = coef * pow10(expand);
        let new_bexp = bexp - expand;
        if !sign {
            // Positive: ULP up. Spills into next decade at 10^34.
            if new_coef + 1 < COEFFICIENT_LIMIT {
                return (
                    Self::from_bits(pack_finite(false, new_bexp, new_coef + 1)),
                    Status::OK,
                );
            }
            if new_bexp == BIASED_EXP_MAX {
                return (Self::INFINITY, Status::OK);
            }
            return (
                Self::from_bits(pack_finite(false, new_bexp + 1, COEFFICIENT_LIMIT / 10)),
                Status::OK,
            );
        }
        // Negative: ULP toward zero. Mirror image of the positive spill:
        // when new_coef is COEFFICIENT_LIMIT/10 (= 10^33, the smallest
        // 34-digit coefficient) and we still have bexp headroom, the
        // numerically adjacent value lives one cohort *finer* — at
        // (new_bexp - 1, COEFFICIENT_LIMIT - 1). Without this, ULP at e.g.
        // -1 would be 10^-33 instead of the correct 10^-34.
        if new_coef == COEFFICIENT_LIMIT / 10 && new_bexp > 0 {
            return (
                Self::from_bits(pack_finite(true, new_bexp - 1, COEFFICIENT_LIMIT - 1)),
                Status::OK,
            );
        }
        if new_coef > 1 {
            return (
                Self::from_bits(pack_finite(true, new_bexp, new_coef - 1)),
                Status::OK,
            );
        }
        // new_coef == 1 with new_bexp == 0: -MIN_POSITIVE → -0 *with the
        // same quantum* (exponent -6176), not the canonical -0 (quantum 0).
        (Self::from_bits(pack_finite(true, 0, 0)), Status::OK)
    }

    /// IEEE 754-2019 §5.3.1 `nextDown(x) = −nextUp(−x)`.
    ///
    /// See [`Decimal128::next_up`] for the symmetric op.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // The largest finite value strictly less than +∞ is MAX.
    /// let (r, st) = Decimal128::INFINITY.next_down();
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::MAX.to_bits());
    /// ```
    #[must_use]
    pub fn next_down(self) -> (Self, Status) {
        let (up, st) = self.neg().next_up();
        (up.neg(), st)
    }

    /// IEEE 754-2019 §5.10 `compareTotalMagnitude(x, y)`.
    ///
    /// Total-order comparison of `|x|` and `|y|`, equivalent to
    /// `x.abs().total_cmp(y.abs())`. No status flags are raised.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::cmp::Ordering;
    /// use ferrodec::Decimal128;
    ///
    /// // |+1| == |-1| in total-magnitude order.
    /// assert_eq!(
    ///     Decimal128::ONE.compare_total_magnitude(Decimal128::NEG_ONE),
    ///     Ordering::Equal,
    /// );
    /// // |1| < |10|.
    /// assert_eq!(
    ///     Decimal128::ONE.compare_total_magnitude(Decimal128::TEN),
    ///     Ordering::Less,
    /// );
    /// ```
    #[inline]
    #[must_use]
    pub fn compare_total_magnitude(self, other: Self) -> Ordering {
        self.abs().total_cmp(other.abs())
    }

    /// IEEE 754-2019 §5.3.4: the radix of the floating-point format.
    ///
    /// Always `10` for `Decimal128`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// assert_eq!(Decimal128::radix(), 10);
    /// ```
    #[inline]
    #[must_use]
    pub const fn radix() -> u32 {
        10
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// Decode a finite-or-zero `Decimal128` into `(sign, biased_exp, coefficient)`.
#[inline]
fn decode_finite(x: Decimal128) -> (bool, u32, u128) {
    match classify_bits(x.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => unreachable!(),
    }
}

/// `10^k` for `k ≤ 38`.
#[inline]
fn pow10(k: u32) -> u128 {
    10u128.pow(k)
}

/// Divide `coef` by `10^n`, returning `(quotient, round_digit, sticky)`.
///
/// `round_digit` is the most-significant dropped digit (the one immediately
/// below the kept portion). `sticky` is `true` if any digit below the round
/// digit was non-zero.
fn divide_by_pow10(coef: u128, n: u32) -> (u128, u32, bool) {
    let mut c = coef;
    let mut sticky = false;
    let mut round_digit = 0u32;
    for i in 0..n {
        let r = (c % 10) as u32;
        c /= 10;
        if i + 1 == n {
            round_digit = r;
        } else if r != 0 {
            sticky = true;
        }
    }
    (c, round_digit, sticky)
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::RoundingMode;

    #[cfg(feature = "fmt")]
    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn num_eq(a: Decimal128, b: Decimal128) -> bool {
        matches!(a.partial_cmp(b).0, Some(Ordering::Equal))
    }

    // --- quantize ---

    #[test]
    #[cfg(feature = "fmt")]
    fn quantize_exact_rescale_up() {
        // 1.23E-5 quantized to 1E-7 → 123E-7 (exact, no rounding)
        let x = parse("1.23E-5");
        let target = parse("1E-7");
        let (r, st) = x.quantize(target, RoundingMode::NearestEven);
        assert!(num_eq(r, x), "value must be unchanged: {r:?}");
        assert!(st.is_ok());
        // Quantum of result must match target.
        assert!(r.same_quantum(target));
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn quantize_rounds_down() {
        // 1.234 quantized to 1E-2 (two decimal places) → 1.23 (truncated toward zero)
        let x = parse("1.234");
        let target = parse("1E-2");
        let (r, st) = x.quantize(target, RoundingMode::TowardZero);
        assert!(num_eq(r, parse("1.23")));
        assert!(st.inexact());
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn quantize_rounds_nearest_even() {
        let x = parse("1.235");
        let target = parse("1E-2");
        let (r, st) = x.quantize(target, RoundingMode::NearestEven);
        // 1.235 → 1.24 (round half to even: 3 is odd so round up)
        assert!(num_eq(r, parse("1.24")));
        assert!(st.inexact());
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn quantize_exact_no_inexact() {
        let x = parse("1.230");
        let target = parse("1E-2");
        let (r, st) = x.quantize(target, RoundingMode::NearestEven);
        assert!(num_eq(r, parse("1.23")));
        assert!(st.is_ok(), "exact rescale should not raise INEXACT");
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn quantize_invalid_overflow() {
        // MAX has 34-digit coefficient at biased_exp BIASED_EXP_MAX.
        // Quantizing to biased_exp BIASED_EXP_MAX-1 (quantum 6110) would
        // require 35 digits → INVALID.
        let x = Decimal128::MAX;
        let target = parse("1E+6110"); // biased_exp = 12286 = BIASED_EXP_MAX - 1
        let (r, st) = x.quantize(target, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn quantize_inf_inf() {
        let (r, st) =
            Decimal128::INFINITY.quantize(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.is_ok());
    }

    #[test]
    fn quantize_inf_finite_invalid() {
        let (r, st) = Decimal128::INFINITY.quantize(Decimal128::ONE, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn quantize_nan_propagates() {
        let (r, st) = Decimal128::NAN.quantize(Decimal128::ONE, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.is_ok());
        let (r, st) =
            Decimal128::SIGNALING_NAN.quantize(Decimal128::ONE, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn quantize_zero_adjusts_quantum() {
        // +0 at any quantum → 0 at target quantum.
        let z = Decimal128::ZERO;
        let target = Decimal128::from_bits(crate::bid::pack_finite(false, crate::bid::BIAS - 3, 0));
        let (r, st) = z.quantize(target, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.same_quantum(target));
        assert!(st.is_ok());
    }

    // --- same_quantum ---

    #[test]
    #[cfg(feature = "fmt")]
    fn same_quantum_matching_exponents() {
        let a = parse("1.23");
        let b = parse("4.56");
        assert!(a.same_quantum(b));
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn same_quantum_different_exponents() {
        let a = parse("1.2");
        let b = parse("1.23");
        assert!(!a.same_quantum(b));
    }

    #[test]
    fn same_quantum_nan_nan() {
        assert!(Decimal128::NAN.same_quantum(Decimal128::SIGNALING_NAN));
    }

    #[test]
    fn same_quantum_inf_inf() {
        assert!(Decimal128::INFINITY.same_quantum(Decimal128::NEG_INFINITY));
    }

    #[test]
    fn same_quantum_inf_finite() {
        assert!(!Decimal128::INFINITY.same_quantum(Decimal128::ONE));
        assert!(!Decimal128::ONE.same_quantum(Decimal128::INFINITY));
    }

    // --- scaleb ---

    #[test]
    fn scaleb_shift_in_range() {
        // 1.23 × 10^2 = 123
        let x = Decimal128::from_bits(crate::bid::pack_finite(false, crate::bid::BIAS - 2, 123));
        let (r, st) = x.scaleb(2, RoundingMode::NearestEven);
        assert!(st.is_ok());
        assert!(num_eq(
            r,
            Decimal128::from_bits(crate::bid::pack_finite(false, crate::bid::BIAS, 123))
        ));
    }

    #[test]
    fn scaleb_overflow() {
        let (r, st) = Decimal128::MAX.scaleb(1, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.overflow());
    }

    #[test]
    fn scaleb_underflow() {
        // MIN_POSITIVE × 10^-1 → zero with UNDERFLOW.
        let (r, st) = Decimal128::MIN_POSITIVE.scaleb(-1, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(st.underflow());
    }

    #[test]
    fn scaleb_nan_snan() {
        let (r, st) = Decimal128::NAN.scaleb(1, RoundingMode::NearestEven);
        assert!(r.is_nan() && st.is_ok());
        let (r, st) = Decimal128::SIGNALING_NAN.scaleb(1, RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid());
    }

    // --- logb ---

    #[test]
    fn logb_one() {
        let (r, st) = Decimal128::ONE.logb();
        assert!(num_eq(r, Decimal128::ZERO));
        assert!(st.is_ok());
    }

    #[test]
    fn logb_ten() {
        let (r, st) = Decimal128::TEN.logb();
        assert!(num_eq(r, Decimal128::ONE));
        assert!(st.is_ok());
    }

    #[test]
    #[cfg(feature = "fmt")]
    fn logb_small() {
        // logb(0.001) = −3
        let x = parse("0.001");
        let (r, st) = x.logb();
        assert!(num_eq(r, parse("-3")));
        assert!(st.is_ok());
    }

    #[test]
    fn logb_zero() {
        let (r, st) = Decimal128::ZERO.logb();
        assert!(r == Decimal128::NEG_INFINITY);
        assert!(st.div_by_zero());
    }

    #[test]
    fn logb_infinity() {
        let (r, st) = Decimal128::INFINITY.logb();
        assert!(r == Decimal128::INFINITY);
        assert!(st.is_ok());
    }

    #[test]
    fn logb_snan() {
        let (r, st) = Decimal128::SIGNALING_NAN.logb();
        assert!(r.is_nan() && st.invalid());
    }

    // --- next_up / next_down ---

    #[test]
    fn next_up_zero() {
        let (r, st) = Decimal128::ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal128::MIN_POSITIVE.to_bits());
        assert!(st.is_ok());
        let (r, _) = Decimal128::NEG_ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal128::MIN_POSITIVE.to_bits());
    }

    #[test]
    fn next_up_neg_infinity() {
        let (r, st) = Decimal128::NEG_INFINITY.next_up();
        assert_eq!(r.to_bits(), Decimal128::MIN.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_pos_infinity() {
        let (r, st) = Decimal128::INFINITY.next_up();
        assert_eq!(r.to_bits(), Decimal128::INFINITY.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_non_canonical_infinity() {
        // Non-canonical Inf encodings (type field 11110, trailing bits
        // non-zero) decode as Class::Infinity but don't bit-equal the
        // canonical INFINITY constant. Pre-fix, next_up's
        // `self == Self::INFINITY` check missed them and the
        // `unreachable!()` arm in the classify_bits match panicked.
        // Surface bug Kani found in `next_up_special_dispatch`.
        let dirty_pos = Decimal128::from_bits(Decimal128::INFINITY.to_bits() | 1);
        let (r, st) = dirty_pos.next_up();
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.is_ok());

        let dirty_neg = Decimal128::from_bits(Decimal128::NEG_INFINITY.to_bits() | 1);
        let (r, st) = dirty_neg.next_up();
        // next_up(-∞) = MIN regardless of which Inf encoding came in.
        assert_eq!(r.to_bits(), Decimal128::MIN.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_positive_normalises_then_increments() {
        // ONE = (false, BIAS, 1) — 1 digit. Normalise to 34 digits:
        // coef = 10^33, bexp = BIAS-33. Then ULP up: coef = 10^33 + 1.
        // Numerically: 1 → 1.000_000_000_000_000_000_000_000_000_000_001.
        let (r, st) = Decimal128::ONE.next_up();
        let expected = Decimal128::from_bits(crate::bid::pack_finite(
            false,
            crate::bid::BIAS - 33,
            10u128.pow(33) + 1,
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_positive_decade_spill() {
        // (false, BIAS, COEFFICIENT_LIMIT-1) → (false, BIAS+1, COEFFICIENT_LIMIT/10)
        let x = Decimal128::from_bits(crate::bid::pack_finite(
            false,
            crate::bid::BIAS,
            COEFFICIENT_LIMIT - 1,
        ));
        let (r, st) = x.next_up();
        let expected = Decimal128::from_bits(crate::bid::pack_finite(
            false,
            crate::bid::BIAS + 1,
            COEFFICIENT_LIMIT / 10,
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_negative_spills_into_finer_cohort() {
        // NEG_ONE = (true, BIAS, 1). Normalise: coef = 10^33, bexp = BIAS-33.
        // Mirror of the positive overflow spill: the coefficient is at the
        // bottom of its cohort (10^33), so the numerically adjacent value
        // lives one cohort *finer* — bexp = BIAS-34, coef = 10^34 - 1.
        // Numerically: −1 → −0.999_999_999_999_999_999_999_999_999_999_9999
        // (34 nines, ULP = 10^−34, not 10^−33).
        let (r, st) = Decimal128::NEG_ONE.next_up();
        let expected = Decimal128::from_bits(crate::bid::pack_finite(
            true,
            crate::bid::BIAS - 34,
            10u128.pow(34) - 1,
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_negative_no_spill() {
        // -2 = (true, BIAS, 2). Normalise: coef = 2×10^33, bexp = BIAS-33.
        // 2×10^33 ≠ COEFFICIENT_LIMIT/10, so no spill — just decrement.
        // Numerically: −2 → −1.999_999_999_999_999_999_999_999_999_999_999.
        let neg2 = Decimal128::from_bits(crate::bid::pack_finite(true, crate::bid::BIAS, 2));
        let (r, st) = neg2.next_up();
        let expected = Decimal128::from_bits(crate::bid::pack_finite(
            true,
            crate::bid::BIAS - 33,
            2 * 10u128.pow(33) - 1,
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_neg_min_positive() {
        // −MIN_POSITIVE → −0
        let (r, st) = Decimal128::MIN_POSITIVE.neg().next_up();
        assert!(r.is_zero() && r.is_sign_negative());
        assert!(st.is_ok());
    }

    #[test]
    fn next_down_symmetric() {
        // nextDown(x) = −nextUp(−x)
        let (up, _) = Decimal128::ONE.neg().next_up();
        let (dn, _) = Decimal128::ONE.next_down();
        assert_eq!(up.neg().to_bits(), dn.to_bits());
    }

    #[test]
    fn next_down_max_to_finite() {
        // nextDown(+∞) should be MAX.
        let (r, st) = Decimal128::INFINITY.next_down();
        assert_eq!(r.to_bits(), Decimal128::MAX.to_bits());
        assert!(st.is_ok());
    }

    #[test]
    fn next_up_snan() {
        let (r, st) = Decimal128::SIGNALING_NAN.next_up();
        assert!(r.is_nan() && st.invalid());
        let (r, st) = Decimal128::NAN.next_up();
        assert!(r.is_nan() && st.is_ok());
    }

    // --- compare_total_magnitude ---

    #[test]
    fn compare_total_magnitude_ignores_sign() {
        let a = Decimal128::ONE;
        let b = Decimal128::NEG_ONE;
        // |1| == |−1| in total order (same bit pattern for magnitude)
        assert_eq!(a.compare_total_magnitude(b), Ordering::Equal);
    }

    #[test]
    fn compare_total_magnitude_orders_by_abs() {
        assert_eq!(
            Decimal128::ONE.compare_total_magnitude(Decimal128::TEN),
            Ordering::Less
        );
        assert_eq!(
            Decimal128::NEG_INFINITY.compare_total_magnitude(Decimal128::ONE),
            Ordering::Greater
        );
    }

    // --- radix ---

    #[test]
    fn radix_is_ten() {
        assert_eq!(Decimal128::radix(), 10);
    }
}
