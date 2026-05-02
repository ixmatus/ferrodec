//! IEEE 754 comparison predicates: `partial_cmp`, `total_cmp`, `min`, `max`.
//!
//! Two distinct comparisons are exposed:
//!
//! * [`Decimal128::partial_cmp`] is the IEEE 754 §5.11 numeric comparison.
//!   NaN inputs yield `None`. A signaling NaN raises `INVALID`. `+0` and
//!   `−0` compare equal, as do members of the same numeric cohort.
//! * [`Decimal128::total_cmp`] is the IEEE 754 §5.10 `totalOrder` predicate.
//!   Every bit pattern has a unique position. `−sNaN < −qNaN < −∞ < … <
//!   −0 < +0 < … < +∞ < +qNaN < +sNaN`. Equal-magnitude finite cohorts are
//!   ordered by exponent: smaller exponent first if positive, larger
//!   exponent first if negative.
//!
//! `min` and `max` follow IEEE 754-2019 §9.6 `minimum`/`maximum`: NaN
//! propagates (a NaN operand poisons the result, raising `INVALID` if
//! signaling); otherwise the numerically-smaller / -larger value wins,
//! with `−0 < +0`.

use core::cmp::Ordering;

use crate::bid::{classify_bits, decimal_digit_count, pow10, Class};
use crate::decimal::Decimal128;
use crate::status::Status;

impl Decimal128 {
    /// IEEE 754 §5.11 numeric comparison.
    ///
    /// Returns `None` if either operand is NaN. The accompanying `Status`
    /// raises `INVALID` if either operand is a signaling NaN; otherwise
    /// the status is `OK`.
    #[inline]
    #[must_use]
    pub fn partial_cmp(self, other: Self) -> (Option<Ordering>, Status) {
        let mut status = Status::OK;
        if self.is_signaling_nan() || other.is_signaling_nan() {
            status |= Status::INVALID;
        }
        if self.is_nan() || other.is_nan() {
            return (None, status);
        }
        (Some(numeric_cmp(self, other)), status)
    }

    /// IEEE 754 §5.10 `totalOrder` predicate, returned as an [`Ordering`].
    ///
    /// This is a *total* order on every 128-bit pattern — there is no NaN
    /// poisoning, no `Option`, and no `Status`. Every two values compare
    /// as `Less`, `Equal`, or `Greater`.
    ///
    /// Equality under `total_cmp` is strictly stronger than IEEE numeric
    /// equality: two values that compare numerically equal but live in
    /// different cohorts (e.g. `1.0E+1` vs `10.0E+0`) compare as ordered,
    /// not equal.
    #[inline]
    #[must_use]
    pub fn total_cmp(self, other: Self) -> Ordering {
        let a = classify_bits(self.0);
        let b = classify_bits(other.0);
        let ra = total_order_rank(&a);
        let rb = total_order_rank(&b);
        if ra != rb {
            return ra.cmp(&rb);
        }
        // Same major rank — break ties.
        match (a, b) {
            (
                Class::SignalingNaN { sign, payload: pa },
                Class::SignalingNaN {
                    sign: _,
                    payload: pb,
                },
            )
            | (
                Class::QuietNaN { sign, payload: pa },
                Class::QuietNaN {
                    sign: _,
                    payload: pb,
                },
            ) => signed_payload_cmp(sign, pa, pb),
            (Class::Infinity { .. }, Class::Infinity { .. }) => Ordering::Equal,
            (
                Class::Zero {
                    sign,
                    biased_exp: ea,
                },
                Class::Zero {
                    sign: _,
                    biased_exp: eb,
                },
            ) => zero_cohort_cmp(sign, ea, eb),
            (
                Class::Finite {
                    sign,
                    biased_exp: ea,
                    coefficient: ca,
                },
                Class::Finite {
                    sign: _,
                    biased_exp: eb,
                    coefficient: cb,
                },
            ) => finite_total_cmp(sign, ea, ca, eb, cb),
            // Mixed cases at the same rank shouldn't happen — but stay total.
            _ => Ordering::Equal,
        }
    }

    /// IEEE 754 `minimum(x, y)`. NaN is propagated.
    ///
    /// * If either operand is a NaN, the result is NaN. Signaling-NaN
    ///   operands raise `INVALID`.
    /// * Otherwise returns the operand with the smaller numeric value
    ///   (`−0` is less than `+0`).
    #[inline]
    #[must_use]
    pub fn min(self, other: Self) -> (Self, Status) {
        let mut status = Status::OK;
        if self.is_signaling_nan() || other.is_signaling_nan() {
            status |= Status::INVALID;
        }
        if self.is_nan() || other.is_nan() {
            return (Self::NAN, status);
        }
        let result = match numeric_cmp(self, other) {
            Ordering::Less | Ordering::Equal => self,
            Ordering::Greater => other,
        };
        (result, status)
    }

    /// IEEE 754 `maximum(x, y)`. NaN is propagated.
    #[inline]
    #[must_use]
    pub fn max(self, other: Self) -> (Self, Status) {
        let mut status = Status::OK;
        if self.is_signaling_nan() || other.is_signaling_nan() {
            status |= Status::INVALID;
        }
        if self.is_nan() || other.is_nan() {
            return (Self::NAN, status);
        }
        let result = match numeric_cmp(self, other) {
            Ordering::Greater | Ordering::Equal => self,
            Ordering::Less => other,
        };
        (result, status)
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// Major-class ordering. Ranks are dense from `−5` (-sNaN) to `+5` (+sNaN).
///
/// Inside each rank, additional tie-breaking is applied (cohorts, payloads).
const fn total_order_rank(c: &Class) -> i8 {
    match c {
        Class::SignalingNaN { sign: true, .. } => -5,
        Class::QuietNaN { sign: true, .. } => -4,
        Class::Infinity { sign: true } => -3,
        Class::Finite { sign: true, .. } => -2,
        Class::Zero { sign: true, .. } => -1,
        Class::Zero { sign: false, .. } => 1,
        Class::Finite { sign: false, .. } => 2,
        Class::Infinity { sign: false } => 3,
        Class::QuietNaN { sign: false, .. } => 4,
        Class::SignalingNaN { sign: false, .. } => 5,
    }
}

/// NaN payload tie-breaker.
///
/// For positive NaNs, smaller payload precedes; for negative NaNs, larger
/// payload precedes. (Mirrors the cohort rule: bigger "magnitude" further
/// from zero.)
fn signed_payload_cmp(sign: bool, pa: u128, pb: u128) -> Ordering {
    if sign {
        pb.cmp(&pa)
    } else {
        pa.cmp(&pb)
    }
}

/// Cohort tie-breaker for two zeros of the same sign.
///
/// IEEE 754: smaller exponent precedes when positive; larger exponent
/// precedes when negative.
fn zero_cohort_cmp(sign: bool, ea: u32, eb: u32) -> Ordering {
    if sign {
        eb.cmp(&ea)
    } else {
        ea.cmp(&eb)
    }
}

/// `total_cmp` between two same-sign finite, non-zero values.
fn finite_total_cmp(sign: bool, ea: u32, ca: u128, eb: u32, cb: u128) -> Ordering {
    let mag = magnitude_cmp(ca, ea, cb, eb);
    if mag != Ordering::Equal {
        return if sign { mag.reverse() } else { mag };
    }
    // Same numeric magnitude, different cohort.
    if sign {
        eb.cmp(&ea)
    } else {
        ea.cmp(&eb)
    }
}

/// IEEE 754 numeric comparison for non-NaN inputs.
///
/// Any two non-NaN values compare with a defined ordering: numerically
/// equal cohorts compare equal, and `+0 == −0`.
fn numeric_cmp(a: Decimal128, b: Decimal128) -> Ordering {
    let ca = classify_bits(a.0);
    let cb = classify_bits(b.0);
    match (ca, cb) {
        (Class::Infinity { sign: sa }, Class::Infinity { sign: sb }) => {
            // -Inf == -Inf, +Inf == +Inf, -Inf < +Inf
            match (sa, sb) {
                (true, true) | (false, false) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
            }
        }
        (Class::Infinity { sign: true }, _) => Ordering::Less,
        (Class::Infinity { sign: false }, _) => Ordering::Greater,
        (_, Class::Infinity { sign: true }) => Ordering::Greater,
        (_, Class::Infinity { sign: false }) => Ordering::Less,
        (Class::Zero { .. }, Class::Zero { .. }) => Ordering::Equal,
        (Class::Zero { .. }, Class::Finite { sign: sb, .. }) => {
            if sb {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (Class::Finite { sign: sa, .. }, Class::Zero { .. }) => {
            if sa {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (
            Class::Finite {
                sign: sa,
                biased_exp: ea,
                coefficient: cab,
            },
            Class::Finite {
                sign: sb,
                biased_exp: eb,
                coefficient: cbb,
            },
        ) => {
            if sa != sb {
                return if sa {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            let mag = magnitude_cmp(cab, ea, cbb, eb);
            if sa {
                mag.reverse()
            } else {
                mag
            }
        }
        // NaN handled at the caller; reaching here means a class we don't
        // expect from a non-NaN input. Fall back to Equal to stay total.
        _ => Ordering::Equal,
    }
}

/// Compare magnitudes of two non-zero finite values without overflow.
///
/// Strategy:
/// 1. Each value's "decimal scale" is `digit_count(c) + biased_exp`. If
///    these differ, the one with larger scale has larger magnitude.
/// 2. Otherwise the digit-count delta equals the exponent delta, so we can
///    scale the smaller-exponent coefficient up by `10^delta` and compare —
///    the result has at most `PRECISION` digits, which fits in `u128`.
fn magnitude_cmp(ca: u128, ea: u32, cb: u128, eb: u32) -> Ordering {
    let da = decimal_digit_count(ca);
    let db = decimal_digit_count(cb);
    let scale_a = da as i64 + ea as i64;
    let scale_b = db as i64 + eb as i64;
    if scale_a != scale_b {
        return scale_a.cmp(&scale_b);
    }
    // Same decimal scale, possibly different cohorts.
    match ea.cmp(&eb) {
        Ordering::Less => {
            // ea < eb ⇒ ca has more digits ⇒ scale up cb to ea.
            let diff = eb - ea;
            ca.cmp(&(cb * pow10(diff)))
        }
        Ordering::Greater => {
            let diff = ea - eb;
            (ca * pow10(diff)).cmp(&cb)
        }
        Ordering::Equal => ca.cmp(&cb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_total_lt(a: Decimal128, b: Decimal128) {
        assert_eq!(
            a.total_cmp(b),
            Ordering::Less,
            "expected {a:?} < {b:?} under total_cmp"
        );
        assert_eq!(
            b.total_cmp(a),
            Ordering::Greater,
            "expected {b:?} > {a:?} under total_cmp"
        );
    }

    #[test]
    fn total_cmp_reflexive_for_constants() {
        let consts = [
            Decimal128::ZERO,
            Decimal128::NEG_ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::TEN,
            Decimal128::MAX,
            Decimal128::MIN,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
            Decimal128::SIGNALING_NAN,
        ];
        for c in consts {
            assert_eq!(c.total_cmp(c), Ordering::Equal);
        }
    }

    #[test]
    fn total_cmp_orders_extremes() {
        // -sNaN < -qNaN < -Inf < MIN < -1 < -0 < 0 < 1 < MAX < +Inf < qNaN < sNaN
        let order = [
            Decimal128::SIGNALING_NAN.neg(),
            Decimal128::NAN.neg(),
            Decimal128::NEG_INFINITY,
            Decimal128::MIN,
            Decimal128::NEG_ONE,
            Decimal128::NEG_ZERO,
            Decimal128::ZERO,
            Decimal128::ONE,
            Decimal128::MAX,
            Decimal128::INFINITY,
            Decimal128::NAN,
            Decimal128::SIGNALING_NAN,
        ];
        for w in order.windows(2) {
            assert_total_lt(w[0], w[1]);
        }
    }

    #[test]
    fn neg_zero_total_lt_pos_zero() {
        assert_total_lt(Decimal128::NEG_ZERO, Decimal128::ZERO);
    }

    #[test]
    fn partial_cmp_zero_equality() {
        let (ord, st) = Decimal128::ZERO.partial_cmp(Decimal128::NEG_ZERO);
        assert_eq!(ord, Some(Ordering::Equal));
        assert!(st.is_ok());
    }

    #[test]
    fn partial_cmp_quiet_nan_returns_none_no_flag() {
        let (ord, st) = Decimal128::ZERO.partial_cmp(Decimal128::NAN);
        assert_eq!(ord, None);
        assert!(st.is_ok());
        let (ord, st) = Decimal128::NAN.partial_cmp(Decimal128::ZERO);
        assert_eq!(ord, None);
        assert!(st.is_ok());
    }

    #[test]
    fn partial_cmp_signaling_nan_raises_invalid() {
        let (ord, st) = Decimal128::ONE.partial_cmp(Decimal128::SIGNALING_NAN);
        assert_eq!(ord, None);
        assert!(st.invalid());
        let (ord, st) = Decimal128::SIGNALING_NAN.partial_cmp(Decimal128::ONE);
        assert_eq!(ord, None);
        assert!(st.invalid());
    }

    #[test]
    fn partial_cmp_orders_finite_values() {
        assert_eq!(
            Decimal128::NEG_ONE
                .partial_cmp(Decimal128::ONE)
                .0
                .unwrap(),
            Ordering::Less
        );
        assert_eq!(
            Decimal128::ONE
                .partial_cmp(Decimal128::NEG_ONE)
                .0
                .unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            Decimal128::ONE.partial_cmp(Decimal128::ONE).0.unwrap(),
            Ordering::Equal
        );
    }

    #[test]
    fn partial_cmp_infinities() {
        assert_eq!(
            Decimal128::NEG_INFINITY
                .partial_cmp(Decimal128::INFINITY)
                .0
                .unwrap(),
            Ordering::Less
        );
        assert_eq!(
            Decimal128::INFINITY
                .partial_cmp(Decimal128::INFINITY)
                .0
                .unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            Decimal128::INFINITY
                .partial_cmp(Decimal128::MAX)
                .0
                .unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            Decimal128::NEG_INFINITY
                .partial_cmp(Decimal128::MIN)
                .0
                .unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn partial_cmp_equates_cohorts() {
        // Build two encodings of "1": (coefficient=1, exp=BIAS) and
        // (coefficient=10, exp=BIAS-1) — both numerically equal 1.
        use crate::bid;
        let a = Decimal128::from_bits(bid::pack_finite(false, bid::BIAS, 1));
        let b = Decimal128::from_bits(bid::pack_finite(false, bid::BIAS - 1, 10));
        assert_eq!(a.partial_cmp(b).0.unwrap(), Ordering::Equal);
        // But under total_cmp they're ordered (cohort tie-break: smaller exp first for positive).
        assert_eq!(a.total_cmp(b), Ordering::Greater);
        assert_eq!(b.total_cmp(a), Ordering::Less);
    }

    #[test]
    fn min_max_basic() {
        let (lo, st) = Decimal128::ONE.min(Decimal128::TEN);
        assert!(st.is_ok());
        assert_eq!(lo.partial_cmp(Decimal128::ONE).0.unwrap(), Ordering::Equal);

        let (hi, st) = Decimal128::ONE.max(Decimal128::TEN);
        assert!(st.is_ok());
        assert_eq!(hi.partial_cmp(Decimal128::TEN).0.unwrap(), Ordering::Equal);
    }

    #[test]
    fn min_max_nan_propagation() {
        let (r, st) = Decimal128::ONE.min(Decimal128::NAN);
        assert!(r.is_nan());
        assert!(st.is_ok());

        let (r, st) = Decimal128::ONE.max(Decimal128::SIGNALING_NAN);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn magnitude_cmp_handles_large_scale_diff() {
        // 1E+100 > 9.999E+99 (numerically): ca=1, ea=BIAS+100; cb=9999, eb=BIAS+96
        use crate::bid;
        let a = Decimal128::from_bits(bid::pack_finite(false, bid::BIAS + 100, 1));
        let b = Decimal128::from_bits(bid::pack_finite(false, bid::BIAS + 96, 9999));
        assert_eq!(a.partial_cmp(b).0.unwrap(), Ordering::Greater);
        assert_eq!(b.partial_cmp(a).0.unwrap(), Ordering::Less);
    }
}
