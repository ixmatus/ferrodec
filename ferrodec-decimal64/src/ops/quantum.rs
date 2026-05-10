//! Quantum-manipulating operations for [`Decimal64`].
//!
//! * [`Decimal64::quantize`] — IEEE 754-2019 §5.3.3: rescale `self`
//!   to have the same quantum as `target`, rounding by `rm`. Raises
//!   `INVALID` when the result cannot fit in the format with the
//!   target's quantum.
//! * [`Decimal64::scaleb`] — IEEE 754-2019 §5.3.3: multiply `self`
//!   by `10^n` where `n` is an integer.
//! * [`Decimal64::logb`] — IEEE 754-2019 §5.3.3: integer log base
//!   10 of `|self|` — equivalently, the value's adjusted exponent.
//! * [`Decimal64::next_up`] / [`Decimal64::next_down`] — §5.3.1
//!   navigation operations to the next representable value.

use crate::bid::{
    classify_bits, decimal_digit_count, Class, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, PRECISION,
};
use crate::decimal::Decimal64;
use ferrodec_ieee::{should_round_up, RoundingMode, Status};

use super::round::round_and_pack_finite;

const POW10_U64: [u64; 10] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
];

impl Decimal64 {
    /// IEEE 754-2019 §5.3.3 `quantize(self, target)`: returns a value
    /// numerically equal to `self` (after rounding by `rm`) but with
    /// the same quantum exponent as `target`.
    ///
    /// `INVALID` is raised when `target`'s quantum is incompatible
    /// (the rescaled coefficient would exceed `PRECISION` digits) or
    /// when either operand is NaN with sNaN semantics. Infinity
    /// quantizes to itself only when `target` is also infinity;
    /// otherwise INVALID.
    #[must_use]
    pub fn quantize(self, target: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(target.0);

        if let Class::SignalingNaN { sign, payload } = ca {
            return (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            );
        }
        if let Class::SignalingNaN { sign, payload } = cb {
            return (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            );
        }
        if let Class::QuietNaN { sign, payload } = ca {
            return (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            );
        }
        if let Class::QuietNaN { sign, payload } = cb {
            return (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            );
        }

        // Infinity: only well-defined when both are infinities (with
        // the result preserving the sign of self).
        if let Class::Infinity { sign } = ca {
            if matches!(cb, Class::Infinity { .. }) {
                return (
                    Decimal64::from_bits(crate::bid::pack_infinity(sign)),
                    Status::OK,
                );
            }
            return (Decimal64::NAN, Status::INVALID);
        }
        if matches!(cb, Class::Infinity { .. }) {
            // self finite, target infinity → INVALID.
            return (Decimal64::NAN, Status::INVALID);
        }

        // Both finite (or self zero).
        let (sign, biased_self, coef) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let target_biased = match cb {
            Class::Finite { biased_exp, .. } | Class::Zero { biased_exp, .. } => biased_exp,
            _ => unreachable!(),
        };

        let target_q = target_biased as i32 - BIAS as i32;
        let self_q = biased_self as i32 - BIAS as i32;

        // Preferred quantum for quantize is target's quantum. Pass
        // target_q as both unbiased_exp (effectively "where the
        // coefficient lives") and q_preferred only when the
        // coefficient already sits at target_q; otherwise we need to
        // round.
        //
        // Strategy: pass coef and self_q; round_and_pack will shift
        // toward target_q (pad on inexact, strip on exact within the
        // PRECISION budget). But the function's strip logic only
        // fires up to PRECISION digits; for cases that need shifting
        // outside that envelope we have to handle directly.

        // Step 1: rescale to target_q.
        if target_q == self_q {
            // Already at the right quantum; pack as-is.
            return (
                Decimal64::from_bits(crate::bid::pack_finite(sign, target_biased, coef)),
                Status::OK,
            );
        }

        if target_q > self_q {
            // Drop digits (round). Number of decimal places to drop:
            // target_q - self_q.
            let drop = (target_q - self_q) as u32;
            // After dropping `drop` digits we may have empty
            // coefficient (zero); rounding may or may not bump it.
            let digits: u32 = if coef == 0 {
                0
            } else {
                decimal_digit_count(coef)
            };
            let (kept, round_digit, sticky): (u64, u32, bool) = if drop >= digits {
                // All digits dropped. round_digit is the MSD if
                // drop == digits, 0 (with sticky) if drop > digits.
                if drop == digits && coef != 0 {
                    let kept = 0u64;
                    let mut c = coef;
                    let mut sticky = false;
                    while c >= 10 {
                        if c % 10 != 0 {
                            sticky = true;
                        }
                        c /= 10;
                    }
                    (kept, c as u32, sticky)
                } else {
                    (0, 0, coef != 0)
                }
            } else {
                let mut c = coef;
                let mut sticky = false;
                let mut round_digit = 0u32;
                for i in 0..drop {
                    let r = (c % 10) as u32;
                    c /= 10;
                    if i == drop - 1 {
                        round_digit = r;
                    } else if r != 0 {
                        sticky = true;
                    }
                }
                (c, round_digit, sticky)
            };

            let mut status = Status::OK;
            if round_digit != 0 || sticky {
                status |= Status::INEXACT;
            }
            let last_lsb = (kept % 10) as u32;
            let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
            let final_coef = if round_up { kept + 1 } else { kept };

            // Renormalise if rounding crossed COEFFICIENT_LIMIT (e.g.
            // 9999999 → 10000000). Per the GDA spec, if the rounded
            // coefficient exceeds PRECISION digits at the target
            // quantum, the operation is INVALID.
            if final_coef >= COEFFICIENT_LIMIT {
                // Could carry into a different cohort, but for
                // quantize the user demanded target_q; we cannot move.
                // The result with the rounded value at target_q would
                // need (PRECISION + 1) digits, which violates the
                // format. Per GDA, INVALID.
                return (Decimal64::NAN, Status::INVALID);
            }

            return (
                Decimal64::from_bits(crate::bid::pack_finite(sign, target_biased, final_coef)),
                status,
            );
        }

        // target_q < self_q: pad coef with trailing zeros.
        let pad = (self_q - target_q) as u32;
        let new_digits: u32 = if coef == 0 {
            pad
        } else {
            decimal_digit_count(coef) + pad
        };
        if new_digits > PRECISION {
            // Coefficient would not fit in PRECISION digits at the
            // target quantum: INVALID per GDA.
            return (Decimal64::NAN, Status::INVALID);
        }
        if (pad as usize) >= POW10_U64.len() {
            // Cannot pad that much without u64 overflow; INVALID.
            return (Decimal64::NAN, Status::INVALID);
        }
        let new_coef = coef * POW10_U64[pad as usize];
        debug_assert!(new_coef < COEFFICIENT_LIMIT);
        (
            Decimal64::from_bits(crate::bid::pack_finite(sign, target_biased, new_coef)),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.3 `scaleB(self, n)`: returns `self * 10^n`.
    ///
    /// Equivalent to shifting the unbiased exponent by `n`. NaN
    /// propagates per the §6.2.3 rule; `±∞` and `±0` pass through
    /// unchanged. Out-of-range exponents trigger overflow / underflow
    /// via the standard `round_and_pack_finite` path.
    #[must_use]
    pub fn scaleb(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign } => (
                Decimal64::from_bits(crate::bid::pack_infinity(sign)),
                Status::OK,
            ),
            Class::Zero { sign, biased_exp } => {
                let q = biased_exp as i32 - BIAS as i32 + n;
                round_and_pack_finite(0, q, q, sign, false, rm, Status::OK)
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let q = biased_exp as i32 - BIAS as i32 + n;
                round_and_pack_finite(coefficient, q, q, sign, false, rm, Status::OK)
            }
        }
    }

    /// IEEE 754-2019 §5.3.3 `logB(self)`: integer base-10 logarithm
    /// of `|self|` returned as a `Decimal64` at quantum 0.
    ///
    /// * `±0` → `−∞` + `DIV_BY_ZERO`.
    /// * `±∞` → `+∞`.
    /// * NaN → propagated NaN (sNaN raises INVALID).
    /// * Otherwise: `floor(log10(|self|))` = adjusted exponent
    ///   = `Q(self) + digits(coef) − 1`.
    #[must_use]
    pub fn logb(self) -> (Self, Status) {
        let class = classify_bits(self.0);
        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal64::INFINITY, Status::OK),
            Class::Zero { .. } => (Decimal64::NEG_INFINITY, Status::DIV_BY_ZERO),
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let q = biased_exp as i32 - BIAS as i32;
                let adj = q + decimal_digit_count(coefficient) as i32 - 1;
                if adj >= 0 {
                    (
                        Decimal64::try_new(i64::from(adj), 0).unwrap_or(Decimal64::ZERO),
                        Status::OK,
                    )
                } else {
                    (
                        Decimal64::try_new(i64::from(adj), 0).unwrap_or(Decimal64::NEG_ZERO),
                        Status::OK,
                    )
                }
            }
        }
    }

    /// IEEE 754-2019 §5.3.1 `nextUp(self)`: the next representable
    /// `Decimal64` strictly greater than `self`, navigating along
    /// the `Decimal64` number line (signed-zero distinguished, +∞
    /// stays at +∞, −∞ moves to MIN, NaN propagates).
    ///
    /// Returns `(value, Status)`. A signaling-NaN input is quieted
    /// and raises `INVALID`; all other inputs return `Status::OK`.
    /// Per IEEE 754-2019 §5.3.1 the finite-to-+∞ transition does
    /// not raise OVERFLOW.
    #[must_use]
    pub fn next_up(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            if let Class::SignalingNaN { sign, payload } = classify_bits(self.0) {
                return (
                    Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                );
            }
        }
        if self.is_nan() {
            return (self, Status::OK);
        }
        if self.is_zero() {
            return (Decimal64::MIN_POSITIVE, Status::OK);
        }
        if self.is_infinite() {
            return (
                if self.is_sign_negative() {
                    Decimal64::MIN
                } else {
                    Decimal64::INFINITY
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
        // Renormalise to the lowest representable cohort: expand the
        // coefficient toward PRECISION digits, bounded by biased_exp
        // = 0 (the subnormal floor). Without this, an in-cohort ±1
        // step at a high-quantum input would skip over numerically
        // adjacent values (`next_up(5)` would return `6`, not the
        // actual ULP `5.000000000000001`).
        let digits = crate::bid::decimal_digit_count(coef);
        let expand = (PRECISION - digits).min(bexp);
        let new_coef = coef * crate::bid::pow10(expand);
        let new_bexp = bexp - expand;
        if !sign {
            if new_coef + 1 < COEFFICIENT_LIMIT {
                return (
                    Decimal64::from_bits(crate::bid::pack_finite(false, new_bexp, new_coef + 1)),
                    Status::OK,
                );
            }
            if new_bexp == BIASED_EXP_MAX {
                return (Decimal64::INFINITY, Status::OK);
            }
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    false,
                    new_bexp + 1,
                    COEFFICIENT_LIMIT / 10,
                )),
                Status::OK,
            );
        }
        if new_coef == COEFFICIENT_LIMIT / 10 && new_bexp > 0 {
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    true,
                    new_bexp - 1,
                    COEFFICIENT_LIMIT - 1,
                )),
                Status::OK,
            );
        }
        if new_coef > 1 {
            return (
                Decimal64::from_bits(crate::bid::pack_finite(true, new_bexp, new_coef - 1)),
                Status::OK,
            );
        }
        (
            Decimal64::from_bits(crate::bid::pack_finite(true, 0, 0)),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.1 `nextDown(self) = −nextUp(−self)`.
    ///
    /// Returns `(value, Status)`. A signaling-NaN input is quieted
    /// and raises `INVALID`; all other inputs return `Status::OK`.
    #[must_use]
    pub fn next_down(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            if let Class::SignalingNaN { sign, payload } = classify_bits(self.0) {
                return (
                    Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                );
            }
        }
        let (r, s) = self.neg().next_up();
        (r.neg(), s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn quantize_pad_with_zeros() {
        // quantize(1, 1E-2) = 1.00 (= 100 × 10^-2)
        let (r, s) = from_int(1, 0).quantize(from_int(1, -2), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 2, 100));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn quantize_round_to_target_quantum() {
        // quantize(1.234, 1E-1) = 1.2 (rounded)
        let (r, s) = from_int(1234, -3).quantize(from_int(1, -1), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 1, 12));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn quantize_overflow_invalid() {
        // quantize(MAX, 1E0) — the rescaled coefficient would have
        // 97 digits at quantum 0, way over PRECISION = 7. INVALID.
        let (r, s) = Decimal64::MAX.quantize(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn quantize_infinity_passthrough() {
        let (r, s) = Decimal64::INFINITY.quantize(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.is_ok());

        let (r, s) = Decimal64::INFINITY.quantize(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn scaleb_basic() {
        // 1.5 × 10^2 = 150
        let (r, _) = from_int(15, -1).scaleb(2, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS + 1, 15));
        assert_eq!(r.to_bits(), expected.to_bits());

        // 5 × 10^-3 = 0.005
        let (r, _) = from_int(5, 0).scaleb(-3, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 3, 5));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn scaleb_overflow_underflow() {
        let (r, s) = Decimal64::MAX.scaleb(10, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());

        let (r, s) = Decimal64::MIN_POSITIVE.scaleb(-100, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn logb_basic() {
        // logb(1) = 0
        let (r, _) = Decimal64::ONE.logb();
        assert_eq!(r.to_bits(), from_int(0, 0).to_bits());

        // logb(100) = 2
        let (r, _) = from_int(100, 0).logb();
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());

        // logb(0.001) = -3
        let (r, _) = from_int(1, -3).logb();
        assert_eq!(r.to_bits(), from_int(-3, 0).to_bits());
    }

    #[test]
    fn logb_specials() {
        let (r, s) = Decimal64::ZERO.logb();
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, _) = Decimal64::INFINITY.logb();
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, s) = Decimal64::NAN.logb();
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_zero_to_min_positive() {
        let (r, s) = Decimal64::ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.to_bits());
        assert!(s.is_ok());

        let (r, s) = Decimal64::NEG_ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_finite_renormalises_to_ulp() {
        // Regression: the previous implementation incremented the
        // coefficient in the *stored* cohort, returning `6` for
        // next_up(5). The IEEE 754 spec requires the *adjacent
        // representable value*, which for 5 stored at the maximum
        // cohort is 5 + 10^-15 = 5.000000000000001.
        let (r, s) = from_int(5, 0).next_up();
        // Expected: 5_000_000_000_000_001 × 10^-15 — biased_exp =
        // BIAS - 15 = 383, coefficient = 5_000_000_000_000_001.
        assert_eq!(
            r.to_bits(),
            Decimal64::from_bits(pack_finite(false, BIAS - 15, 5_000_000_000_000_001)).to_bits()
        );
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_at_infinity() {
        let (r, s) = Decimal64::INFINITY.next_up();
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        // -∞.next_up() = MIN
        let (r, s) = Decimal64::NEG_INFINITY.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn next_down_basic() {
        // next_down(0) = -MIN_POSITIVE
        let (r, s) = Decimal64::ZERO.next_down();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.neg().to_bits());
        assert!(s.is_ok());

        // next_down(MIN_POSITIVE) = 0
        let (r, s) = Decimal64::MIN_POSITIVE.next_down();
        assert!(r.is_zero() && !r.is_sign_negative());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_qnan_propagates() {
        let (r, s) = Decimal64::NAN.next_up();
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_snan_quiets_and_raises_invalid() {
        // Regression: §5.3.1 says a signaling-NaN input must be
        // quieted *and* raise INVALID. The previous signature `pub
        // fn next_up(self) -> Self` couldn't carry the flag.
        let (r, s) = Decimal64::SIGNALING_NAN.next_up();
        assert!(r.is_quiet_nan());
        assert!(!r.is_signaling_nan());
        assert!(s.invalid());
    }

    #[test]
    fn next_down_snan_quiets_and_raises_invalid() {
        let (r, s) = Decimal64::SIGNALING_NAN.next_down();
        assert!(r.is_quiet_nan());
        assert!(!r.is_signaling_nan());
        assert!(s.invalid());
    }
}
