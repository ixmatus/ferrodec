//! The rounding strategy that turns a re-runnable variable-precision kernel
//! into a correctly rounded [`Decimal`].
//!
//! A kernel is a pure function of a working precision `wp`: given `wp` it
//! returns a [`Work`] approximating the true value to within `err` units in the
//! last place of its `wp`-digit result, where `err` is a small constant the
//! kernel guarantees by carrying internal guard digits. The strategy decides
//! the correctly rounded result with a bracket test: the true value lies in
//! `[v - err, v + err + 1]` ulp (the `+1` covers the truncated sticky tail), so
//! it rounds both bracket coefficients to the context and, if they agree, that
//! shared value is provably the correct rounding of every value in the bracket,
//! the true value included.
//!
//! [`RoundingStrategy::BoundedZiv`] (the default) re-runs the kernel at a
//! growing guard until the bracket is decisive, with a generous cap and a
//! faithful fallback if the cap is reached (the astronomically unlikely table
//! maker's dilemma case, wrong by at most the bracket width, never silently
//! claimed correct). [`RoundingStrategy::FixedGuard`] runs once at a wide guard;
//! it is the swappable alternative, not proven sufficient at unbounded
//! precision, kept behind the same interface so the default is a one-line
//! choice. This is libmpdec's own technique, so the differential against it
//! stays cohort exact. ADR-0032 rejected Ziv for the fixed-width formats on
//! embedded latency grounds; that argument does not bind a crate that already
//! requires a heap.

use super::work::Work;
use crate::round::round_finite;
use crate::{Context, Decimal, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// How the working precision is chosen when rounding a kernel result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoundingStrategy {
    /// Re-run at a growing guard until the rounding is provably decisive.
    BoundedZiv,
    /// Run once at a fixed wide guard (the swappable alternative).
    FixedGuard,
}

/// The crate default. Flipping it is a one-line, ADR-recorded edit; it is not
/// exposed on [`Context`] for this arc.
pub(crate) const DEFAULT_STRATEGY: RoundingStrategy = RoundingStrategy::BoundedZiv;

/// The first guard width [`RoundingStrategy::BoundedZiv`] tries; doubled on each
/// indecisive pass.
const INITIAL_GUARD: u32 = 8;

/// Maximum number of guard doublings before the faithful fallback. `8 * 2^11`
/// is far more guard than any non-pathological input needs.
const MAX_DOUBLINGS: u32 = 11;

/// The single fixed guard for [`RoundingStrategy::FixedGuard`].
const FIXED_GUARD: u32 = 40;

/// Round a kernel result to `ctx` and pack, per `strat`.
///
/// `kernel(wp)` returns the value computed to `wp` significant digits, accurate
/// to within `err` ulp of the true value at that precision. The general
/// transcendental result is inexact (the exact cases are short-circuited before
/// `finish`), so the rounding always carries `Inexact`.
pub(crate) fn finish<F>(
    ctx: &Context,
    err: u128,
    strat: RoundingStrategy,
    kernel: F,
) -> (Decimal, Status)
where
    F: Fn(u32) -> Work,
{
    match strat {
        RoundingStrategy::BoundedZiv => {
            let mut guard = INITIAL_GUARD;
            for _ in 0..MAX_DOUBLINGS {
                let v = kernel(ctx.precision + guard);
                if let Some(result) = try_round(&v, err, ctx) {
                    return result;
                }
                guard = guard.saturating_mul(2);
            }
            // Cap reached: round the widest computation faithfully. Correct to
            // within the bracket; the named residual exposure in ADR-0040.
            let v = kernel(ctx.precision + guard);
            faithful_round(v, ctx)
        }
        RoundingStrategy::FixedGuard => {
            let v = kernel(ctx.precision + FIXED_GUARD);
            faithful_round(v, ctx)
        }
    }
}

/// Round `v` to `ctx.precision` if the bracket `[v - err, v + err + 1]` rounds
/// to a single value, returning `None` (indecisive) otherwise.
///
/// The two bracket endpoints are rounded as *exact* finite values (no sticky),
/// so the result is the exact rounding of each endpoint; since rounding is
/// monotonic and the true value lies between them, agreement proves that shared
/// value is the true value's correct rounding. The returned value and status
/// then come from rounding the actual kernel result with its real sticky bit,
/// which carries the genuine `Inexact` / `Underflow` / `Overflow` / `Clamped`
/// flags (the brackets establish only that the value is decided).
fn try_round(v: &Work, err: u128, ctx: &Context) -> Option<(Decimal, Status)> {
    let err_db = DecBig::from_u128(err);
    let lo_coeff = if v.coeff.cmp_ref(&err_db) == Ordering::Less {
        DecBig::zero()
    } else {
        v.coeff.sub(&err_db)
    };
    let hi_coeff = v.coeff.add(&DecBig::from_u128(err + 1));
    // `v.exp` is the result's preferred exponent: it sits below the rounded
    // result, forcing neither a trailing-zero strip nor a pad on this path.
    let ideal = v.exp;
    let (lo, _) = round_finite(v.sign, lo_coeff, v.exp, false, ideal, ctx, Status::OK);
    let (hi, _) = round_finite(v.sign, hi_coeff, v.exp, false, ideal, ctx, Status::OK);
    if lo == hi {
        Some(faithful_round(v.clone(), ctx))
    } else {
        None
    }
}

/// Round `v` once with its real sticky bit. The decided result of [`try_round`],
/// and the faithful fallback when the Ziv cap is reached or
/// [`RoundingStrategy::FixedGuard`] is selected.
fn faithful_round(v: Work, ctx: &Context) -> (Decimal, Status) {
    let ideal = v.exp;
    round_finite(v.sign, v.coeff, v.exp, v.sticky, ideal, ctx, Status::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;
    use alloc::string::ToString;

    fn ctx(precision: u32, rounding: Rounding) -> Context {
        Context::new(precision, 9999, -9999, rounding)
    }

    /// A kernel computing `num / den` to the requested precision; a stand-in
    /// for a real transcendental kernel with a clean closed form to check
    /// against `Decimal::divide`.
    fn ratio_kernel(num: i64, den: i64) -> impl Fn(u32) -> Work {
        move |wp| Work::from_i64(num).div_to(&Work::from_i64(den), wp)
    }

    fn dec(n: i64) -> Decimal {
        Decimal::finite(n < 0, DecBig::from_u128(u128::from(n.unsigned_abs())), 0)
    }

    /// The correctly rounded `num / den` via the shipped division op.
    fn divide_ref(num: i64, den: i64, ctx: &Context) -> Decimal {
        dec(num).divide(&dec(den), ctx).0
    }

    #[test]
    fn bounded_ziv_matches_division_half_even() {
        let c = ctx(20, Rounding::HalfEven);
        for (n, d) in [(1, 3), (2, 3), (1, 7), (22, 7), (355, 113), (10, 9)] {
            let (got, st) = finish(&c, 2, RoundingStrategy::BoundedZiv, ratio_kernel(n, d));
            assert_eq!(got, divide_ref(n, d, &c), "{n}/{d}");
            assert!(st.inexact(), "{n}/{d} inexact");
        }
    }

    #[test]
    fn bounded_ziv_matches_division_directed_modes() {
        for mode in [
            Rounding::Down,
            Rounding::Up,
            Rounding::Ceiling,
            Rounding::Floor,
            Rounding::HalfUp,
            Rounding::HalfDown,
        ] {
            let c = ctx(15, mode);
            for (n, d) in [(1, 3), (2, 7), (1, 11), (100, 7)] {
                let (got, _) = finish(&c, 2, RoundingStrategy::BoundedZiv, ratio_kernel(n, d));
                assert_eq!(got, divide_ref(n, d, &c), "{n}/{d} mode {mode:?}");
            }
        }
    }

    #[test]
    fn fixed_guard_agrees_with_bounded_on_easy_inputs() {
        let c = ctx(12, Rounding::HalfEven);
        let (a, _) = finish(&c, 2, RoundingStrategy::BoundedZiv, ratio_kernel(1, 3));
        let (b, _) = finish(&c, 2, RoundingStrategy::FixedGuard, ratio_kernel(1, 3));
        assert_eq!(a, b);
        assert_eq!(a.to_string(), "0.333333333333");
    }

    #[test]
    fn normal_inexact_carries_only_inexact() {
        // A normal-range non-terminating result raises Inexact and nothing else.
        let c = ctx(9, Rounding::HalfEven);
        let (_, st) = finish(&c, 2, RoundingStrategy::BoundedZiv, ratio_kernel(1, 3));
        assert!(st.inexact() && !st.underflow() && !st.overflow() && !st.clamped());
    }
}
