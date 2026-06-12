//! The rounding core: turn an exact (or guard-digit-bearing) intermediate
//! into a context-rounded [`Decimal`] with the appropriate status flags.
//!
//! This mirrors the five-step shape of the fixed-width parent's
//! `round_and_pack_finite` (`ferrodec/src/ops/round.rs`), generalized to a
//! [`DecBig`] coefficient and a runtime [`Context`] precision, and extended
//! with the General Decimal Arithmetic exponent rules (Etiny subnormals,
//! overflow to infinity or the largest finite, and IEEE-style clamping):
//!
//! 1. Drop the digits below the working precision (or below the subnormal
//!    quantum Etiny, whichever drops more) in a single rounding, tracking the
//!    round digit and the sticky bit.
//! 2. Apply the rounding direction via [`Rounding::round_up`].
//! 3. Renormalize when the round-up carried past the precision.
//! 4. Shift toward the ideal exponent: pad an inexact result out to full
//!    precision, or strip the trailing zeros of an exact result.
//! 5. Resolve overflow, subnormal underflow, and clamping, then pack.

use crate::{Context, Decimal, Rounding, Status};
use ferrodec_multiword::DecBig;

/// Round `(-1)^sign * coeff * 10^exp` to the context and pack it.
///
/// `pre_sticky` carries a sticky bit from an earlier exact-digit loss (a
/// non-terminating division remainder); add / subtract / multiply pass
/// `false` because `DecBig` makes their intermediates exact. `ideal_exp` is
/// the operation's preferred exponent (the cohort the result should land in
/// when it is exact). `status` is accumulated, not replaced.
#[allow(clippy::too_many_arguments)]
pub(crate) fn round_finite(
    sign: bool,
    coeff: DecBig,
    exp: i64,
    pre_sticky: bool,
    ideal_exp: i64,
    ctx: &Context,
    mut status: Status,
) -> (Decimal, Status) {
    let p = i64::from(ctx.precision.get());
    let emax = i64::from(ctx.emax);
    let emin = i64::from(ctx.emin);
    let etiny = emin - (p - 1);
    let etop = emax - (p - 1);

    // Exact zero: choose the cohort exponent and clamp it into range.
    if coeff.is_zero() && !pre_sticky {
        let mut e = exp.min(ideal_exp);
        let upper = if ctx.clamp { etop } else { emax };
        if e < etiny {
            e = etiny;
            status |= Status::CLAMPED;
        } else if e > upper {
            e = upper;
            status |= Status::CLAMPED;
        }
        return (Decimal::finite(sign, DecBig::zero(), e as i32), status);
    }

    let mut c = coeff;
    let mut e = exp;

    // Step 1: single-rounding drop to the wider of the precision excess and the
    // subnormal excess (drop enough that the exponent reaches Etiny).
    let digits = c.decimal_digit_count() as i64;
    let precision_excess = (digits - p).max(0);
    let subnormal_excess = (etiny - e).max(0);
    let drop = precision_excess.max(subnormal_excess);
    // Tininess is detected on the pre-rounding adjusted exponent (the General
    // Decimal Arithmetic convention).
    let tiny_pre = e + digits - 1 < emin;

    let mut round_digit = 0u32;
    let mut sticky = pre_sticky;
    if drop > 0 {
        let drop = drop as u32;
        let (kept, rem) = c.div_rem_pow10(drop);
        let (rd, lower) = rem.div_rem_pow10(drop - 1);
        round_digit = rd.to_u128().unwrap_or(0) as u32;
        sticky = sticky || !lower.is_zero();
        c = kept;
        e += i64::from(drop);
    }
    let inexact = round_digit != 0 || sticky;
    if inexact {
        status |= Status::INEXACT;
        if tiny_pre {
            status |= Status::UNDERFLOW;
        }
    }

    // Step 2 + 3: apply the rounding direction and renormalize a carry.
    let last_kept = c.div_rem10().1;
    if ctx.rounding.round_up(sign, last_kept, round_digit, sticky) {
        c = c.add(&DecBig::from_u32(1));
        if c.decimal_digit_count() as i64 > p {
            // The bump carried past precision (e.g. 999 -> 1000): drop the new
            // trailing zero and lift the exponent. The dropped digit is zero.
            let (kept, _zero) = c.div_rem10();
            c = kept;
            e += 1;
        }
    }

    // A nonzero intermediate that rounds away to zero (only reachable deep in
    // the subnormal range) is still a zero whose exponent must be tidied into
    // range, exactly as an exact zero is (the path above): clamp into
    // [Etiny, upper], signaling Clamped when constrained. This is independent of
    // `ctx.clamp` (zeros always tidy their exponent), and the Inexact /
    // Underflow already accumulated from the rounding remain.
    if c.is_zero() {
        let mut z = exp.min(ideal_exp);
        let upper = if ctx.clamp { etop } else { emax };
        if z < etiny {
            z = etiny;
            status |= Status::CLAMPED;
        } else if z > upper {
            z = upper;
            status |= Status::CLAMPED;
        }
        return (Decimal::finite(sign, DecBig::zero(), z as i32), status);
    }

    // Step 4: shift toward the ideal exponent.
    if !c.is_zero() {
        let cur_digits = c.decimal_digit_count() as i64;
        let down_target = ideal_exp.max(etiny);
        if e > down_target {
            // Pad with trailing zeros (lowering the exponent) up to precision.
            let shift = (e - down_target).min(p - cur_digits).max(0);
            if shift > 0 {
                c = c.mul_pow10(shift as u32);
                e -= shift;
            }
        } else if e < ideal_exp && !inexact {
            // Strip trailing zeros (raising the exponent) on an exact result.
            let mut want = ideal_exp - e;
            while want > 0 {
                let (q, r) = c.div_rem10();
                if r != 0 {
                    break;
                }
                c = q;
                e += 1;
                want -= 1;
            }
        }
    }

    // Step 5: overflow.
    let d2 = c.decimal_digit_count() as i64;
    let adj = e + d2 - 1;
    if adj > emax {
        status |= Status::OVERFLOW | Status::INEXACT;
        return if overflow_to_infinity(ctx.rounding, sign) {
            (Decimal::infinity(sign), status)
        } else {
            // Largest finite magnitude Nmax = (10^p - 1) * 10^Etop.
            let nmax = DecBig::pow10(ctx.precision.get()).sub(&DecBig::from_u32(1));
            (Decimal::finite(sign, nmax, etop as i32), status)
        };
    }

    // Clamp the exponent down into [.., Etop] when requested, padding zeros.
    if ctx.clamp && !c.is_zero() && e > etop {
        let pad = (e - etop) as u32;
        c = c.mul_pow10(pad);
        e -= i64::from(pad);
        status |= Status::CLAMPED;
    }

    (Decimal::finite(sign, c, e as i32), status)
}

/// Whether an overflow rounds to infinity (`true`) or to the largest finite
/// magnitude Nmax (`false`), per the specification's overflow rule.
fn overflow_to_infinity(rounding: Rounding, sign: bool) -> bool {
    match rounding {
        // Round toward zero never reaches infinity.
        Rounding::Down | Rounding::ZeroFiveUp => false,
        // Round toward +infinity: a positive overflow goes to +Infinity, a
        // negative one to -Nmax.
        Rounding::Ceiling => !sign,
        // Round toward -infinity: the mirror image.
        Rounding::Floor => sign,
        // The to-nearest modes and round-away overflow to infinity.
        Rounding::HalfEven | Rounding::HalfUp | Rounding::HalfDown | Rounding::Up => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wide context that never overflows or underflows for these inputs.
    fn ctx(precision: u32, rounding: Rounding) -> Context {
        Context::new(
            core::num::NonZeroU32::new(precision).unwrap(),
            1_000_000,
            -1_000_000,
            rounding,
        )
    }

    fn round(coeff: u128, exp: i64, ideal: i64, ctx: &Context) -> Decimal {
        round_finite(
            false,
            DecBig::from_u128(coeff),
            exp,
            false,
            ideal,
            ctx,
            Status::OK,
        )
        .0
    }

    fn fin(coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(false, DecBig::from_u128(coeff), exp)
    }

    #[test]
    fn precision_rounding_half_even() {
        let c = ctx(3, Rounding::HalfEven);
        // 12345 -> 123|45, round digit 4 < 5: stays 123, exp +2.
        assert_eq!(round(12345, 0, 0, &c), fin(123, 2));
        // 12355 -> 123|55, round digit 5 with sticky: up to 124.
        assert_eq!(round(12355, 0, 0, &c), fin(124, 2));
        // 12350 -> exact half, last kept 3 odd: round to even (124).
        assert_eq!(round(12350, 0, 0, &c), fin(124, 2));
        // 12450 -> exact half, last kept 4 even: stays 124.
        assert_eq!(round(12450, 0, 0, &c), fin(124, 2));
    }

    #[test]
    fn precision_rounding_directed() {
        // Down truncates; Up rounds any nonzero tail away.
        assert_eq!(round(12399, 0, 0, &ctx(3, Rounding::Down)), fin(123, 2));
        assert_eq!(round(12301, 0, 0, &ctx(3, Rounding::Up)), fin(124, 2));
        // Carry past precision: 9999 at precision 3 rounds to 1000 -> 100 E1.
        assert_eq!(round(9999, 0, 0, &ctx(3, Rounding::HalfEven)), fin(100, 2));
    }

    #[test]
    fn exact_within_precision_keeps_ideal_exponent() {
        let c = ctx(9, Rounding::HalfEven);
        // Exact 6, ideal exponent 0: no padding, no stripping.
        assert_eq!(round(6, 0, 0, &c), fin(6, 0));
        // Exact value at exp -2 but ideal 0: strip trailing zeros up to ideal.
        // 1500 E-2 (=15.00) with ideal 0 -> strip to 15 E0? No: stripping only
        // raises the exponent while the value is unchanged; 1500E-2 = 15, and
        // ideal 0 means land at 15E0.
        assert_eq!(round(1500, -2, 0, &c), fin(15, 0));
    }

    #[test]
    fn inexact_pads_to_full_precision() {
        // 1 / ... style: an inexact result pads trailing zeros to precision.
        // Here simulate by rounding 1000000 (7 digits) at precision 5 with a
        // nonzero tail so it is inexact, ideal exponent far below.
        let c = ctx(5, Rounding::HalfEven);
        // 1234567 -> 12346 (round half even on 67) exp +2, ideal -10:
        // inexact, so pad toward ideal up to 5 digits (already 5) -> no pad.
        assert_eq!(round(1234567, 0, -10, &c), fin(12346, 2));
    }

    #[test]
    fn overflow_to_infinity_and_nmax() {
        // Precision 3, Emax 5: Nmax = 999 E3 (= 9.99e5), Etop = 3.
        let inf_ctx = Context::new(
            core::num::NonZeroU32::new(3).unwrap(),
            5,
            -5,
            Rounding::HalfEven,
        );
        // 1 E6 overflows: half-even -> +Infinity.
        let (d, s) = round_finite(
            false,
            DecBig::from_u32(1),
            6,
            false,
            6,
            &inf_ctx,
            Status::OK,
        );
        assert!(d.is_infinite() && s.overflow() && s.inexact());
        // Round-down overflow -> Nmax (999 E3).
        let down_ctx = inf_ctx.with_rounding(Rounding::Down);
        let (d2, s2) = round_finite(
            false,
            DecBig::from_u32(1),
            6,
            false,
            6,
            &down_ctx,
            Status::OK,
        );
        assert_eq!(d2, fin(999, 3));
        assert!(s2.overflow());
    }

    #[test]
    fn subnormal_single_rounding_and_underflow() {
        // Precision 3, Emin -5: Etiny = Emin - (p-1) = -7.
        let c = Context::new(
            core::num::NonZeroU32::new(3).unwrap(),
            5,
            -5,
            Rounding::HalfEven,
        );
        // 12345 E-10 has adjusted exponent -6 < Emin (subnormal) and exponent
        // -10 < Etiny, so it drops three digits to reach Etiny (-7): 12|345 ->
        // 12 (round digit 3 keeps it), a subnormal inexact result -> Underflow.
        let (d, s) = round_finite(
            false,
            DecBig::from_u128(12345),
            -10,
            false,
            -10,
            &c,
            Status::OK,
        );
        assert_eq!(d, fin(12, -7));
        assert!(s.underflow() && s.inexact());

        // A result whose adjusted exponent equals Emin is normal, not
        // subnormal: 12345 E-9 rounds to 123 E-7 but raises no underflow.
        let (d2, s2) = round_finite(
            false,
            DecBig::from_u128(12345),
            -9,
            false,
            -9,
            &c,
            Status::OK,
        );
        assert_eq!(d2, fin(123, -7));
        assert!(s2.inexact() && !s2.underflow());
    }

    #[test]
    fn subnormal_round_to_zero_signals_clamped() {
        // Precision 3, Emin -5: Etiny = -7. A value far below Etiny rounds away
        // to zero; its exponent is constrained up to Etiny, so Clamped is
        // signaled with Inexact and Underflow, independent of the clamp flag
        // (a zero always tidies its exponent). Mirrors the exact-zero path.
        let c = Context::new(
            core::num::NonZeroU32::new(3).unwrap(),
            5,
            -5,
            Rounding::HalfEven,
        );
        let (d, s) = round_finite(false, DecBig::from_u128(1), -20, false, -20, &c, Status::OK);
        assert_eq!(d, fin(0, -7));
        assert!(s.clamped() && s.underflow() && s.inexact());
    }
}
