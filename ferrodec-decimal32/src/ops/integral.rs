//! `roundToIntegral{Exact,TiesToEven,TiesAway,TowardPositive,TowardNegative,TowardZero}`
//! and the convenience wrappers `floor` / `ceil` / `trunc` / `round`.
//!
//! IEEE 754-2019 §5.3 specifies a family of "round to integer"
//! operations — one per rounding direction, plus `roundToIntegralExact`
//! which signals `INEXACT` when the input had a non-zero fractional
//! part. The GDA preferred quantum for the result is
//! `max(unbiased_in, 0)`: an already-integer input keeps its existing
//! quantum, and a fractional input is rounded into an integer at
//! quantum `0`.
//!
//! These ops never overflow into infinity and never raise UNDERFLOW;
//! `INVALID` is raised only when the input is a signaling NaN.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient,
    BIAS, COEFFICIENT_LIMIT,
};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
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

    /// Special-case shim for Kani (ADR-0016): resolves the
    /// non-finite / zero classes of `roundToIntegral`, returning `None`
    /// for finite operands so CBMC never has to encode the digit-drop
    /// loop. The finite path's correctness is carried by the exact
    /// oracle in `tests/property_integral.rs` and the rounding-decision
    /// proofs; the harnesses in `src/verify/integral.rs` exercise only
    /// this shim. Routes the special classes through the real kernel
    /// (its NaN / Infinity / Zero arms are loop-free), so the proof is
    /// about production behaviour, not a reimplementation.
    #[cfg(kani)]
    #[must_use]
    pub fn round_to_integral_special_only_for_kani(self) -> Option<(Self, Status)> {
        round_to_integral_special_cases(self)
    }
}

/// The non-finite and zero arms of `roundToIntegral`, factored out so
/// they can be proven in isolation (ADR-0016): this function is
/// **loop-free**, so the Kani shim
/// [`Decimal32::round_to_integral_special_only_for_kani`] can route
/// every special class through real production logic without CBMC ever
/// touching the finite digit-drop loops. Returns `None` for a finite
/// operand (the caller then runs the finite path). The result of these
/// classes does not depend on the rounding direction.
fn round_to_integral_special_cases(x: Decimal32) -> Option<(Decimal32, Status)> {
    match classify_bits(x.to_bits()) {
        Class::SignalingNaN { sign, payload } => Some((
            // sNaN → quiet NaN + INVALID, per IEEE 754-2019 §6.2.
            Decimal32::from_bits(pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { .. } | Class::Infinity { .. } => Some((x, Status::OK)),
        Class::Zero { sign, biased_exp } => {
            // Preferred quantum: max(unbiased, 0). If unbiased ≥ 0 the
            // input already has the right quantum; otherwise pull it
            // up to 0.
            let unbiased = biased_exp as i32 - BIAS as i32;
            if unbiased >= 0 {
                Some((x, Status::OK))
            } else {
                Some((
                    Decimal32::from_bits(pack_finite(
                        sign,
                        BiasedExp::ZERO_QUANTUM,
                        Coefficient::ZERO,
                    )),
                    Status::OK,
                ))
            }
        }
        Class::Finite { .. } => None,
    }
}

fn round_to_integral_kernel(
    x: Decimal32,
    rm: RoundingMode,
    signal_inexact: bool,
) -> (Decimal32, Status) {
    if let Some(special) = round_to_integral_special_cases(x) {
        return special;
    }
    match classify_bits(x.to_bits()) {
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
                // |x| < 1. The integer part is 0; rounding may bump it
                // to ±1 depending on `rm` and the discarded digits.
                let mut sticky = false;
                let mut round_digit = 0u32;
                let mut cur = coefficient;
                for i in 0..drop {
                    let r = cur % 10;
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
                let coef_out = u32::from(round_up);
                let status = if signal_inexact && (round_digit != 0 || sticky) {
                    Status::INEXACT
                } else {
                    Status::OK
                };
                return (
                    Decimal32::from_bits(pack_finite(
                        sign,
                        BiasedExp::ZERO_QUANTUM,
                        Coefficient::try_new(coef_out).expect("coef_out is 0 or 1"),
                    )),
                    status,
                );
            }

            // drop < digits: drop `drop` low digits, rounding per `rm`.
            let mut cur = coefficient;
            let mut sticky = false;
            let mut round_digit = 0u32;
            for i in 0..drop {
                let r = cur % 10;
                if i == drop - 1 {
                    round_digit = r;
                } else if r != 0 {
                    sticky = true;
                }
                cur /= 10;
            }
            let last_kept = cur % 10;
            let round_up = should_round_up_int(rm, sign, last_kept, round_digit, sticky);
            let mut new_coef = if round_up { cur + 1 } else { cur };

            // Carry: if `new_coef` reached 10^7, normalise by shifting
            // up one decade. The coefficient is now 10^6 with quantum
            // `+1` instead of `0` — the value is unchanged.
            let mut biased_out = BiasedExp::ZERO_QUANTUM;
            if new_coef >= COEFFICIENT_LIMIT {
                new_coef /= 10;
                biased_out =
                    BiasedExp::try_from_unbiased(1).expect("quantum +1 is in the encodable range");
            }

            let status = if signal_inexact && (round_digit != 0 || sticky) {
                Status::INEXACT
            } else {
                Status::OK
            };
            (
                Decimal32::from_bits(pack_finite(
                    sign,
                    biased_out,
                    Coefficient::try_new(new_coef)
                        .expect("new_coef < COEFFICIENT_LIMIT after carry"),
                )),
                status,
            )
        }
        // sNaN / qNaN / Infinity / Zero are resolved above by
        // `round_to_integral_special_cases`.
        _ => unreachable!("non-finite handled by round_to_integral_special_cases"),
    }
}

/// Match the rules in `ops::round::should_round_up`. Duplicated
/// locally to keep the integral-only path self-contained (the parent
/// `Decimal128` kernel makes the same deliberate choice).
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

#[cfg(all(test, feature = "fmt"))]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal32 {
        Decimal32::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn d_eq(a: Decimal32, b: Decimal32) -> bool {
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
        let (n, _) = Decimal32::NAN.round_to_integral(RoundingMode::NearestEven);
        assert!(n.is_nan());
        let (s, st) = Decimal32::SIGNALING_NAN.round_to_integral(RoundingMode::NearestEven);
        assert!(s.is_nan());
        assert!(st.invalid());
        let (i, _) = Decimal32::INFINITY.round_to_integral(RoundingMode::NearestEven);
        assert!(i.is_infinite());
    }

    #[test]
    fn integer_input_unchanged_quantum() {
        let x = parse("12300"); // quantum 0
        let (r, _) = x.round_to_integral(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn integer_input_high_quantum_preserved() {
        // 1.23e5 — already integer with quantum +3.
        let x = parse("1.23e5");
        let (r, _) = x.round_to_integral(RoundingMode::NearestEven);
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
        assert!(d_eq(parse("9.5").round(), parse("10")));
        assert!(d_eq(parse("-9.5").round(), parse("-10")));
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
        let neg_zero = Decimal32::NEG_ZERO;
        assert!(neg_zero.floor().is_sign_negative());
        assert!(neg_zero.ceil().is_sign_negative());
        assert!(!Decimal32::ZERO.floor().is_sign_negative());
    }
}
