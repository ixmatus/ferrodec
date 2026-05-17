//! Bounded-domain kernel-equivalence proof for the rounding core
//! (S7, ADR-0021).
//!
//! [`crate::ops::round::bounded_kernel::round_to_p_digits`] is the
//! width-bounded analogue of Steps 1–3 of `round_and_pack_finite` (drop
//! excess digits, decide, carry across a power of ten). These harnesses
//! prove it equals an independently derived reference — the digit count
//! recovered by repeated division rather than the kernel's comparison
//! ladder, and the decision taken from a fresh transcription of the
//! IEEE 754-2019 §4.3.3 table rather than the production
//! `should_round_up` — over the decimal32 kernel shape (`p = 7`,
//! `coef < 10^9`).
//!
//! Why this is in Kani scope while the production pipeline is not: the
//! production core runs on `U256` with a `decimal_digit_count` walk and
//! a `div_rem10` drop loop sized by the full 34-digit domain, which
//! CBMC cannot encode (ADR-0016). The rounding *logic* is
//! width-independent; the kernel reproduces it at `u32` width, loop-free
//! (constant power-of-ten divisors), which is what makes the SAT
//! instance tractable — an earlier `u128`, loop-based draft did not
//! terminate. Stratified one harness per `RoundingMode` so each
//! instance stays small (the plan's fallback ladder).

use crate::ops::round::bounded_kernel::{pow10_u32, round_to_p_digits};
use crate::status::RoundingMode;

/// Decimal32 kernel shape. `10^9` is the largest power of ten that fits
/// `u32`; with `p = 7` it admits up to a two-digit drop, exercising the
/// round / sticky split and the power-of-ten carry past `p`.
const COEF_LIMIT: u32 = 1_000_000_000; // 10^9
const P: u32 = 7;

/// Digit count by repeated division — a different method to the
/// kernel's comparison ladder, so the proof is not a tautology. Bounded
/// by 10 iterations for `u32`.
fn ref_digits(mut n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut d = 0;
    while n != 0 {
        n /= 10;
        d += 1;
    }
    d
}

/// IEEE 754-2019 §4.3.3, transcribed independently of
/// `should_round_up` (and structured per mode so it does not mirror
/// that function's control flow). Identical in intent to S6's
/// `spec_round_up`; duplicated so the kernel proof is independent of
/// the production decision as well as the digit-count method.
fn spec_round_up(
    rm: RoundingMode,
    sign: bool,
    last_kept_lsb: u32,
    round_digit: u32,
    sticky: bool,
) -> bool {
    let any_dropped = round_digit != 0 || sticky;
    match rm {
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => any_dropped && !sign,
        RoundingMode::TowardNegative => any_dropped && sign,
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::NearestEven => {
            if round_digit > 5 {
                true
            } else if round_digit < 5 {
                false
            } else {
                sticky || last_kept_lsb % 2 == 1
            }
        }
    }
}

/// Independent reference for `round_to_p_digits`: digit count by
/// division, decision by [`spec_round_up`]. Same closed-form digit
/// split as the kernel (a single constant-divisor division is already
/// the simplest faithful form), so the proof's force is the digit-count
/// method and the decision being independent.
fn spec_round_to_p(coef: u32, p: u32, sign: bool, rm: RoundingMode) -> (u32, i32) {
    let digits = ref_digits(coef);
    if digits <= p {
        return (coef, 0);
    }
    let drop = digits - p;
    let divisor = pow10_u32(drop);
    let below = pow10_u32(drop - 1);
    let kept = coef / divisor;
    let dropped = coef % divisor;
    let round_digit = (dropped / below) % 10;
    let sticky = dropped % below != 0;

    let mut rounded = kept;
    let mut exp_delta = 0i32;
    if spec_round_up(rm, sign, kept % 10, round_digit, sticky) {
        rounded += 1;
        if rounded == pow10_u32(p) {
            rounded /= 10;
            exp_delta = 1;
        }
    }
    (rounded, exp_delta)
}

macro_rules! kernel_equiv_proof {
    ($name:ident, $rm:expr) => {
        #[kani::proof]
        #[kani::unwind(11)]
        fn $name() {
            let coef: u32 = kani::any();
            kani::assume(coef < COEF_LIMIT);
            let sign: bool = kani::any();
            assert_eq!(
                round_to_p_digits(coef, P, sign, $rm),
                spec_round_to_p(coef, P, sign, $rm),
            );
        }
    };
}

kernel_equiv_proof!(round_kernel_equiv_nearest_even, RoundingMode::NearestEven);
kernel_equiv_proof!(round_kernel_equiv_nearest_away, RoundingMode::NearestAway);
kernel_equiv_proof!(round_kernel_equiv_toward_zero, RoundingMode::TowardZero);
kernel_equiv_proof!(
    round_kernel_equiv_toward_positive,
    RoundingMode::TowardPositive
);
kernel_equiv_proof!(
    round_kernel_equiv_toward_negative,
    RoundingMode::TowardNegative
);
