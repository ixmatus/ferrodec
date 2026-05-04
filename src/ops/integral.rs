//! `roundToIntegral{Exact,TiesToEven,TiesAway,TowardPositive,TowardNegative,TowardZero}`
//! and convenience wrappers `floor` / `ceil` / `trunc` / `round`.
//!
//! IEEE 754-2019 §5.3 specifies a family of "round to integer"
//! operations — one per rounding direction, plus `roundToIntegralExact`
//! which signals `INEXACT` when the input had a non-zero fractional
//! part. `Decimal128`'s preferred quantum for the result is
//! `max(unbiased_in, 0)`: an already-integer input is left at its
//! existing quantum, and a fractional input is rounded into an
//! integer at quantum `0`.
//!
//! These ops never overflow into infinity and never raise UNDERFLOW —
//! `INVALID` is only ever raised when the input is a signaling NaN.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_quiet_nan, Class, BIAS,
    COEFFICIENT_LIMIT,
};
use crate::decimal::Decimal128;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Round to integer per `rm`. **Does not** raise `INEXACT` even
    /// when the input was non-integer. IEEE 754-2019
    /// `roundToIntegralTiesToEven` / `roundToIntegralTowardX`
    /// (selected by `rm`).
    #[must_use]
    pub fn round_to_integral(self, rm: RoundingMode) -> (Self, Status) {
        round_to_integral_kernel(self, rm, false)
    }

    /// Same as [`Self::round_to_integral`] but signals `INEXACT` if
    /// the input had a non-zero fractional part. IEEE 754-2019
    /// `roundToIntegralExact`.
    #[must_use]
    pub fn round_to_integral_exact(self, rm: RoundingMode) -> (Self, Status) {
        round_to_integral_kernel(self, rm, true)
    }

    /// Largest integer `≤ self`. IEEE 754-2019
    /// `roundToIntegralTowardNegative`. Discards `Status` (per the
    /// `f64::floor` convention) — use [`Self::round_to_integral`]
    /// when status flags matter.
    #[must_use]
    pub fn floor(self) -> Self {
        round_to_integral_kernel(self, RoundingMode::TowardNegative, false).0
    }

    /// Smallest integer `≥ self`. IEEE 754-2019
    /// `roundToIntegralTowardPositive`.
    #[must_use]
    pub fn ceil(self) -> Self {
        round_to_integral_kernel(self, RoundingMode::TowardPositive, false).0
    }

    /// Truncate toward zero. IEEE 754-2019
    /// `roundToIntegralTowardZero`.
    #[must_use]
    pub fn trunc(self) -> Self {
        round_to_integral_kernel(self, RoundingMode::TowardZero, false).0
    }

    /// Round to nearest, ties away from zero. IEEE 754-2019
    /// `roundToIntegralTiesAway`. (Matches `f64::round` semantics —
    /// `0.5 → 1`, `-0.5 → -1`.)
    #[must_use]
    pub fn round(self) -> Self {
        round_to_integral_kernel(self, RoundingMode::NearestAway, false).0
    }

    /// Round to nearest, ties to even. IEEE 754-2019
    /// `roundToIntegralTiesToEven`. (Matches `f64::round_ties_even`
    /// semantics — `0.5 → 0`, `1.5 → 2`.)
    #[must_use]
    pub fn round_ties_even(self) -> Self {
        round_to_integral_kernel(self, RoundingMode::NearestEven, false).0
    }
}

fn round_to_integral_kernel(
    x: Decimal128,
    rm: RoundingMode,
    signal_inexact: bool,
) -> (Decimal128, Status) {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { sign, payload } => {
            // sNaN → quiet NaN + INVALID, per IEEE 754-2019 §6.2.
            (
                Decimal128::from_bits(pack_quiet_nan(sign, payload)),
                Status::INVALID,
            )
        }
        Class::QuietNaN { .. } | Class::Infinity { .. } => (x, Status::OK),
        Class::Zero { sign, biased_exp } => {
            // Preferred quantum: max(unbiased, 0). If unbiased ≥ 0,
            // the input already has the right quantum; otherwise pull
            // it up to 0.
            let unbiased = biased_exp as i32 - BIAS as i32;
            if unbiased >= 0 {
                (x, Status::OK)
            } else {
                (
                    Decimal128::from_bits(pack_finite(sign, BIAS, 0)),
                    Status::OK,
                )
            }
        }
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            if unbiased >= 0 {
                // Already integer at quantum ≥ 0 — return unchanged.
                return (x, Status::OK);
            }
            let drop = (-unbiased) as u32;
            let digits = decimal_digit_count(coefficient);

            if drop >= digits {
                // |x| < 1. The integer part is 0; rounding may bump
                // it to ±1 (depending on `rm` and the discarded
                // fractional digits).
                let mut sticky = false;
                let mut round_digit = 0u32;
                let mut cur = coefficient;
                for i in 0..drop {
                    let r = (cur % 10) as u32;
                    if i == drop - 1 {
                        round_digit = r;
                    } else if r != 0 {
                        sticky = true;
                    }
                    cur /= 10;
                    if cur == 0 {
                        break;
                    }
                }
                let last_kept = 0u32; // result so far is 0
                let round_up = should_round_up_int(rm, sign, last_kept, round_digit, sticky);
                let coef_out: u128 = if round_up { 1 } else { 0 };
                let status = if signal_inexact && (round_digit != 0 || sticky) {
                    Status::INEXACT
                } else {
                    Status::OK
                };
                return (
                    Decimal128::from_bits(pack_finite(sign, BIAS, coef_out)),
                    status,
                );
            }

            // drop < digits: drop `drop` low digits, rounding per `rm`.
            let mut cur = coefficient;
            let mut sticky = false;
            let mut round_digit = 0u32;
            for i in 0..drop {
                let r = (cur % 10) as u32;
                if i == drop - 1 {
                    round_digit = r;
                } else if r != 0 {
                    sticky = true;
                }
                cur /= 10;
            }
            let last_kept = (cur % 10) as u32;
            let round_up = should_round_up_int(rm, sign, last_kept, round_digit, sticky);
            let mut new_coef = if round_up { cur + 1 } else { cur };

            // Carry: if `new_coef` reached 10^34, normalise by shifting
            // up one decade. The coefficient is now 10^33 with quantum
            // `+1` instead of `0` — the value (10^33 · 10^1 = 10^34)
            // is unchanged.
            let mut biased_out = BIAS;
            if new_coef >= COEFFICIENT_LIMIT {
                new_coef /= 10;
                biased_out += 1;
            }

            let status = if signal_inexact && (round_digit != 0 || sticky) {
                Status::INEXACT
            } else {
                Status::OK
            };
            (
                Decimal128::from_bits(pack_finite(sign, biased_out, new_coef)),
                status,
            )
        }
    }
}

/// Match the rules in `ops::round::should_round_up`. Duplicated
/// locally to keep the integral-only path self-contained.
fn should_round_up_int(
    rm: RoundingMode,
    sign: bool,
    last_kept: u32,
    round_digit: u32,
    sticky: bool,
) -> bool {
    let dropped_nonzero = round_digit != 0 || sticky;
    if !dropped_nonzero {
        return false;
    }
    match rm {
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => !sign,
        RoundingMode::TowardNegative => sign,
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::NearestEven => match round_digit.cmp(&5) {
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => sticky || (last_kept & 1) == 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn d_eq(a: Decimal128, b: Decimal128) -> bool {
        let (cmp, _) = a.partial_cmp(b);
        matches!(cmp, Some(core::cmp::Ordering::Equal))
    }

    #[test]
    fn floor_positives() {
        assert!(d_eq(parse("3.7").floor(), parse("3")));
        assert!(d_eq(parse("3").floor(), parse("3")));
        assert!(d_eq(parse("0.1").floor(), parse("0")));
    }

    #[test]
    fn floor_negatives() {
        assert!(d_eq(parse("-3.7").floor(), parse("-4")));
        assert!(d_eq(parse("-3").floor(), parse("-3")));
        assert!(d_eq(parse("-0.1").floor(), parse("-1")));
    }

    #[test]
    fn ceil_positives() {
        assert!(d_eq(parse("3.7").ceil(), parse("4")));
        assert!(d_eq(parse("3").ceil(), parse("3")));
        assert!(d_eq(parse("0.1").ceil(), parse("1")));
    }

    #[test]
    fn ceil_negatives() {
        assert!(d_eq(parse("-3.7").ceil(), parse("-3")));
        assert!(d_eq(parse("-3").ceil(), parse("-3")));
        assert!(d_eq(parse("-0.1").ceil(), parse("0")));
    }

    #[test]
    fn trunc_toward_zero() {
        assert!(d_eq(parse("3.7").trunc(), parse("3")));
        assert!(d_eq(parse("-3.7").trunc(), parse("-3")));
        assert!(d_eq(parse("0.999").trunc(), parse("0")));
        assert!(d_eq(parse("-0.999").trunc(), parse("0")));
    }

    #[test]
    fn round_ties_away() {
        assert!(d_eq(parse("0.5").round(), parse("1")));
        assert!(d_eq(parse("-0.5").round(), parse("-1")));
        assert!(d_eq(parse("1.5").round(), parse("2")));
        assert!(d_eq(parse("2.5").round(), parse("3"))); // ties away
        assert!(d_eq(parse("3.4").round(), parse("3")));
        assert!(d_eq(parse("3.6").round(), parse("4")));
    }

    #[test]
    fn round_ties_to_even() {
        assert!(d_eq(parse("0.5").round_ties_even(), parse("0")));
        assert!(d_eq(parse("1.5").round_ties_even(), parse("2")));
        assert!(d_eq(parse("2.5").round_ties_even(), parse("2"))); // ties to even
        assert!(d_eq(parse("3.5").round_ties_even(), parse("4")));
    }

    #[test]
    fn round_specials() {
        let (n, _) = Decimal128::NAN.round_to_integral(RoundingMode::default());
        assert!(n.is_nan());
        let (s, st) = Decimal128::SIGNALING_NAN.round_to_integral(RoundingMode::default());
        assert!(s.is_nan());
        assert!(st.invalid());
        let (i, _) = Decimal128::INFINITY.round_to_integral(RoundingMode::default());
        assert!(i.is_infinite());
    }

    #[test]
    fn integer_input_unchanged_quantum() {
        let x = parse("12300"); // quantum 0
        let (r, _) = x.round_to_integral(RoundingMode::default());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn integer_input_high_quantum_preserved() {
        // 1.23e10 — already integer with quantum +8.
        let x = parse("1.23e10");
        let (r, _) = x.round_to_integral(RoundingMode::default());
        // Same value, same quantum.
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn round_to_integral_exact_signals_inexact_for_fractional() {
        let x = parse("3.7");
        let (r, st) = x.round_to_integral_exact(RoundingMode::TowardNegative);
        assert!(d_eq(r, parse("3")));
        assert!(st.inexact());
    }

    #[test]
    fn round_to_integral_exact_no_inexact_for_integer() {
        let x = parse("3");
        let (r, st) = x.round_to_integral_exact(RoundingMode::TowardNegative);
        assert!(d_eq(r, parse("3")));
        assert!(!st.inexact());
    }

    #[test]
    fn carry_into_new_decade() {
        // 9.5 rounds to 10 (one extra digit).
        assert!(d_eq(parse("9.5").round(), parse("10")));
        // -9.5 rounds to -10.
        assert!(d_eq(parse("-9.5").round(), parse("-10")));
        // 99.5 rounds to 100.
        assert!(d_eq(parse("99.5").round(), parse("100")));
    }

    #[test]
    fn very_small_inputs() {
        assert!(d_eq(parse("1e-30").floor(), parse("0")));
        assert!(d_eq(parse("1e-30").ceil(), parse("1")));
        assert!(d_eq(parse("-1e-30").floor(), parse("-1")));
        assert!(d_eq(parse("-1e-30").ceil(), parse("0")));
    }

    #[test]
    fn floor_preserves_sign_of_zero() {
        // floor(-0) = -0, ceil(-0) = -0.
        let neg_zero = Decimal128::NEG_ZERO;
        assert!(neg_zero.floor().is_sign_negative());
        assert!(neg_zero.ceil().is_sign_negative());
        // floor(+0) = +0.
        assert!(!Decimal128::ZERO.floor().is_sign_negative());
    }
}
