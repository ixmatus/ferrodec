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
//! `min` and `max` implement IEEE 754-2019 §9.6 `minimumNumber` /
//! `maximumNumber` (the NaN-as-missing-value variant): a quiet NaN
//! operand passes through to the other operand instead of poisoning
//! the result; only a signaling NaN raises `INVALID`. The 754-2019
//! `minimum` / `maximum` operations (where qNaN propagates the NaN)
//! are *not* what ferrodec exposes here, despite the method names —
//! the General Decimal Arithmetic specification's `min` / `max` and
//! the original IEEE 754-2008 `minNum` / `maxNum` both defined the
//! NaN-as-missing-value semantics, and that is what callers expect.
//! `−0 < +0` for the cohort tie-break.
//!
//! [`Decimal128::min_magnitude`] and [`Decimal128::max_magnitude`]
//! are the §9.6 `minimumMagnitudeNumber` / `maximumMagnitudeNumber`
//! operations: the operand of smaller (respectively larger)
//! *numeric* magnitude, deferring to `min` / `max` on an
//! equal-magnitude tie so the sign and cohort rules stay identical.

use core::cmp::Ordering;

use crate::bid::{classify_bits, decimal_digit_count, pow10, Class};
use crate::decimal::Decimal128;
use crate::ops::nan_propagate::nan_from;
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
        // Same major rank — break ties. Each arm pulls *both* operand
        // signs and asserts they agree, because the rank table at
        // `total_order_rank` partitions on sign: any two values at the
        // same rank necessarily have the same sign. Capturing the
        // assertion at the destructure site documents the invariant
        // and makes future rank-table changes fail loud rather than
        // silently produce a non-antisymmetric ordering.
        match (a, b) {
            (
                Class::SignalingNaN {
                    sign: sa,
                    payload: pa,
                },
                Class::SignalingNaN {
                    sign: sb,
                    payload: pb,
                },
            )
            | (
                Class::QuietNaN {
                    sign: sa,
                    payload: pa,
                },
                Class::QuietNaN {
                    sign: sb,
                    payload: pb,
                },
            ) => {
                debug_assert!(sa == sb, "same-rank NaNs must share sign");
                signed_payload_cmp(sa, pa, pb)
            }
            (Class::Infinity { .. }, Class::Infinity { .. }) => Ordering::Equal,
            (
                Class::Zero {
                    sign: sa,
                    biased_exp: ea,
                },
                Class::Zero {
                    sign: sb,
                    biased_exp: eb,
                },
            ) => {
                debug_assert!(sa == sb, "same-rank zeros must share sign");
                zero_cohort_cmp(sa, ea, eb)
            }
            (
                Class::Finite {
                    sign: sa,
                    biased_exp: ea,
                    coefficient: ca,
                },
                Class::Finite {
                    sign: sb,
                    biased_exp: eb,
                    coefficient: cb,
                },
            ) => {
                debug_assert!(sa == sb, "same-rank finites must share sign");
                finite_total_cmp(sa, ea, ca, eb, cb)
            }
            // Mixed cases at the same rank shouldn't happen — but stay total.
            _ => Ordering::Equal,
        }
    }

    /// `min(x, y)` per the General Decimal Arithmetic Specification
    /// (IEEE 754-2008 `minNum`).
    ///
    /// * Any signaling NaN poisons the result: returns NaN with
    ///   `INVALID` raised.
    /// * Both operands quiet NaN → NaN.
    /// * One operand quiet NaN, the other finite → the finite
    ///   operand (qNaN is "missing value").
    /// * Otherwise → the operand that is *smaller in totalOrder*.
    ///   This handles equal-magnitude cohort tie-breaking (e.g.
    ///   `min(0, 0.0) = 0.0`, `min(-0, -0.0) = -0`).
    #[inline]
    #[must_use]
    pub fn min(self, other: Self) -> (Self, Status) {
        // IEEE 754-2019 §6.2.3: when a NaN result comes from an sNaN
        // operand, preserve that operand's payload (signal cleared).
        // First-sNaN wins.
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if other.is_signaling_nan() {
            return (nan_from(other), Status::INVALID);
        }
        if self.is_nan() && other.is_nan() {
            return (Self::NAN, Status::OK);
        }
        if self.is_nan() {
            return (other, Status::OK);
        }
        if other.is_nan() {
            return (self, Status::OK);
        }
        let result = match self.total_cmp(other) {
            Ordering::Less | Ordering::Equal => self,
            Ordering::Greater => other,
        };
        (result, Status::OK)
    }

    /// `max(x, y)` — symmetric to [`Decimal128::min`].
    #[inline]
    #[must_use]
    pub fn max(self, other: Self) -> (Self, Status) {
        // IEEE 754-2019 §6.2.3 sNaN payload preservation; see `min`.
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if other.is_signaling_nan() {
            return (nan_from(other), Status::INVALID);
        }
        if self.is_nan() && other.is_nan() {
            return (Self::NAN, Status::OK);
        }
        if self.is_nan() {
            return (other, Status::OK);
        }
        if other.is_nan() {
            return (self, Status::OK);
        }
        let result = match self.total_cmp(other) {
            Ordering::Greater | Ordering::Equal => self,
            Ordering::Less => other,
        };
        (result, Status::OK)
    }

    /// `minimumMagnitude(x, y)` per IEEE 754-2019 §9.6, the
    /// `minimumMagnitudeNumber` (NaN-as-missing-value) variant
    /// consistent with [`Decimal128::min`] = `minimumNumber`.
    ///
    /// The operand of smaller magnitude wins: `|x|` and `|y|` are
    /// compared *numerically*, so cohort does not matter
    /// (`minMagnitude(1.0, 1.00)` is a magnitude tie, not an
    /// ordering). On equal magnitude the result falls back to
    /// [`Decimal128::min`], so the sign and cohort tie-break match
    /// `minimum` exactly (`minMagnitude(-2, 2) = -2`). NaN handling
    /// is identical to `min`: a signaling NaN poisons the result
    /// with `INVALID` (first-sNaN wins, payload preserved per
    /// §6.2.3); a quiet NaN is the missing value and yields the
    /// other operand; two quiet NaNs yield NaN.
    #[inline]
    #[must_use]
    pub fn min_magnitude(self, other: Self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if other.is_signaling_nan() {
            return (nan_from(other), Status::INVALID);
        }
        if self.is_nan() && other.is_nan() {
            return (Self::NAN, Status::OK);
        }
        if self.is_nan() {
            return (other, Status::OK);
        }
        if other.is_nan() {
            return (self, Status::OK);
        }
        match numeric_cmp(self.abs(), other.abs()) {
            Ordering::Less => (self, Status::OK),
            Ordering::Greater => (other, Status::OK),
            // Magnitudes numerically equal: defer to `minimum`, which
            // applies the totalOrder sign / cohort tie-break (and is
            // already conformance-validated).
            Ordering::Equal => self.min(other),
        }
    }

    /// `maximumMagnitude(x, y)` per IEEE 754-2019 §9.6, the
    /// `maximumMagnitudeNumber` variant; symmetric to
    /// [`Decimal128::min_magnitude`] and falling back to
    /// [`Decimal128::max`] on an equal-magnitude tie
    /// (`maxMagnitude(-2, 2) = 2`).
    #[inline]
    #[must_use]
    pub fn max_magnitude(self, other: Self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if other.is_signaling_nan() {
            return (nan_from(other), Status::INVALID);
        }
        if self.is_nan() && other.is_nan() {
            return (Self::NAN, Status::OK);
        }
        if self.is_nan() {
            return (other, Status::OK);
        }
        if other.is_nan() {
            return (self, Status::OK);
        }
        match numeric_cmp(self.abs(), other.abs()) {
            Ordering::Greater => (self, Status::OK),
            Ordering::Less => (other, Status::OK),
            Ordering::Equal => self.max(other),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// Major-class ordering for IEEE 754-2019 totalOrder.
///
/// The decimal-arithmetic specification places `qNaN` at the outermost
/// rank on both sides, with `sNaN` sitting between `qNaN` and `±∞`:
///
/// ```text
/// -qNaN < -sNaN < -∞ < … < -0 < +0 < … < +∞ < +sNaN < +qNaN
/// ```
///
/// (Equivalently: a quiet NaN is "further from zero" than the
/// matching signaling NaN.)
const fn total_order_rank(c: &Class) -> i8 {
    match c {
        Class::QuietNaN { sign: true, .. } => -5,
        Class::SignalingNaN { sign: true, .. } => -4,
        Class::Infinity { sign: true } => -3,
        Class::Finite { sign: true, .. } => -2,
        Class::Zero { sign: true, .. } => -1,
        Class::Zero { sign: false, .. } => 1,
        Class::Finite { sign: false, .. } => 2,
        Class::Infinity { sign: false } => 3,
        Class::SignalingNaN { sign: false, .. } => 4,
        Class::QuietNaN { sign: false, .. } => 5,
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
        // Per the dec-arith spec: qNaN is outermost on both sides,
        // sNaN sits between qNaN and ±∞:
        //   -qNaN < -sNaN < -Inf < MIN < -1 < -0 < 0 < 1 < MAX < +Inf
        //                                                  < +sNaN < +qNaN
        let order = [
            Decimal128::NAN.neg(),
            Decimal128::SIGNALING_NAN.neg(),
            Decimal128::NEG_INFINITY,
            Decimal128::MIN,
            Decimal128::NEG_ONE,
            Decimal128::NEG_ZERO,
            Decimal128::ZERO,
            Decimal128::ONE,
            Decimal128::MAX,
            Decimal128::INFINITY,
            Decimal128::SIGNALING_NAN,
            Decimal128::NAN,
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
    fn total_cmp_same_rank_nan_payloads_antisymmetric() {
        // Pin the NaN payload tie-breaker contract over both signs and
        // both NaN flavours. The destructure was hardened to assert
        // `sa == sb` because the rank table embeds sign; this test
        // confirms the assertion holds for every `(sign, flavour)`
        // pair the rank table funnels through the NaN arm.
        use crate::bid::{pack_quiet_nan, pack_signaling_nan};
        let pa: u128 = 0x1111;
        let pb: u128 = 0x2222;
        for sign in [false, true] {
            // qNaN at the same sign: smaller payload precedes for
            // positive, larger payload precedes for negative.
            let a = Decimal128::from_bits(pack_quiet_nan(sign, pa));
            let b = Decimal128::from_bits(pack_quiet_nan(sign, pb));
            let want = if sign {
                Ordering::Greater
            } else {
                Ordering::Less
            };
            assert_eq!(a.total_cmp(b), want, "qNaN sign={sign}");
            assert_eq!(b.total_cmp(a), want.reverse(), "qNaN reverse sign={sign}");

            // sNaN, same shape.
            let a = Decimal128::from_bits(pack_signaling_nan(sign, pa));
            let b = Decimal128::from_bits(pack_signaling_nan(sign, pb));
            assert_eq!(a.total_cmp(b), want, "sNaN sign={sign}");
            assert_eq!(b.total_cmp(a), want.reverse(), "sNaN reverse sign={sign}");

            // Equal payload, same sign — strictly Equal.
            let a = Decimal128::from_bits(pack_quiet_nan(sign, pa));
            let b = Decimal128::from_bits(pack_quiet_nan(sign, pa));
            assert_eq!(a.total_cmp(b), Ordering::Equal, "qNaN equal sign={sign}");
        }
    }

    #[test]
    fn total_cmp_same_rank_zero_cohorts_antisymmetric() {
        // Same-shape coverage for the Zero arm. The rank table puts
        // +0 cohorts at +1 and -0 at -1 (sign separates rank), so any
        // two zeros at the same rank share sign by construction.
        use crate::bid::{pack_finite, BIAS, BIASED_EXP_MAX};
        for sign in [false, true] {
            let a = Decimal128::from_bits(pack_finite(sign, 0, 0));
            let b = Decimal128::from_bits(pack_finite(sign, BIASED_EXP_MAX, 0));
            // Smaller exponent precedes for positive zero; larger
            // exponent precedes for negative.
            let want = if sign {
                Ordering::Greater
            } else {
                Ordering::Less
            };
            assert_eq!(a.total_cmp(b), want, "zero cohort sign={sign}");
            assert_eq!(b.total_cmp(a), want.reverse());
            // Reflexive.
            let mid = Decimal128::from_bits(pack_finite(sign, BIAS, 0));
            assert_eq!(mid.total_cmp(mid), Ordering::Equal);
        }
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
            Decimal128::NEG_ONE.partial_cmp(Decimal128::ONE).0.unwrap(),
            Ordering::Less
        );
        assert_eq!(
            Decimal128::ONE.partial_cmp(Decimal128::NEG_ONE).0.unwrap(),
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
            Decimal128::INFINITY.partial_cmp(Decimal128::MAX).0.unwrap(),
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
    fn min_max_nan_handling() {
        // dec-spec / minNum: qNaN treated as "missing value", returns
        // the other operand. sNaN ALWAYS poisons (NaN result + INVALID).
        let (r, st) = Decimal128::ONE.min(Decimal128::NAN);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
        assert!(st.is_ok());

        let (r, st) = Decimal128::ONE.max(Decimal128::SIGNALING_NAN);
        assert!(r.is_nan());
        assert!(st.invalid());

        // Both NaN → NaN.
        let (r, _) = Decimal128::NAN.min(Decimal128::NAN);
        assert!(r.is_nan());
    }

    #[test]
    fn min_max_snan_preserves_payload() {
        // IEEE 754-2019 §6.2.3: the result NaN from an sNaN operand
        // must carry that operand's payload. Pre-fix, min/max returned
        // Self::NAN (canonical payload 0) for any sNaN input.
        use crate::bid::{pack_signaling_nan, T_MASK};
        let payload = 0x0123_4567_89AB_CDEFu128;
        let snan = Decimal128::from_bits(pack_signaling_nan(false, payload));
        let neg_snan = Decimal128::from_bits(pack_signaling_nan(true, payload));

        let (r, st) = Decimal128::ONE.min(snan);
        assert!(r.is_quiet_nan(), "sNaN min result is quieted");
        assert!(st.invalid());
        assert_eq!(r.to_bits() & T_MASK, payload & T_MASK);
        assert!(!Decimal128::from_bits(r.to_bits()).is_sign_negative());

        let (r, st) = Decimal128::ONE.max(neg_snan);
        assert!(r.is_quiet_nan());
        assert!(st.invalid());
        assert_eq!(r.to_bits() & T_MASK, payload & T_MASK);
        assert!(Decimal128::from_bits(r.to_bits()).is_sign_negative());

        // First-sNaN-wins ordering: lhs sNaN takes precedence over rhs.
        let lhs_snan = Decimal128::from_bits(pack_signaling_nan(false, 0xAAAA));
        let rhs_snan = Decimal128::from_bits(pack_signaling_nan(false, 0xBBBB));
        let (r, st) = lhs_snan.min(rhs_snan);
        assert!(st.invalid());
        assert_eq!(r.to_bits() & T_MASK, 0xAAAA);
    }

    #[test]
    fn min_max_magnitude_basic() {
        // decTest ddMaxMag/ddMinMag semantics: smaller / larger
        // numeric magnitude, equal magnitude defers to min / max.
        let two = Decimal128::try_new(2, 0).unwrap();
        let neg_two = two.neg();
        let one = Decimal128::ONE;

        // |−2| > |1| → minMagnitude picks 1, maxMagnitude picks −2.
        let (lo, st) = neg_two.min_magnitude(one);
        assert!(st.is_ok());
        assert_eq!(lo.to_bits(), one.to_bits());
        let (hi, st) = neg_two.max_magnitude(one);
        assert!(st.is_ok());
        assert_eq!(hi.to_bits(), neg_two.to_bits());

        // |−2| == |2| → tie defers to min / max: minimum(−2,2) = −2,
        // maximum(−2,2) = 2.
        assert_eq!(neg_two.min_magnitude(two).0.to_bits(), neg_two.to_bits());
        assert_eq!(two.max_magnitude(neg_two).0.to_bits(), two.to_bits());

        // Cohort tie (1.0 vs 1.00): magnitudes numerically equal, so
        // the result is whatever min / max yields, never a panic or a
        // wrong-cohort pick.
        let a = Decimal128::try_new(10, -1).unwrap(); // 1.0
        let b = Decimal128::try_new(100, -2).unwrap(); // 1.00
        let (m, st) = a.min_magnitude(b);
        assert!(st.is_ok());
        assert_eq!(m.to_bits(), a.min(b).0.to_bits());
    }

    #[test]
    fn min_max_magnitude_nan() {
        // qNaN is the missing value; sNaN poisons with INVALID.
        let (r, st) = Decimal128::ONE.min_magnitude(Decimal128::NAN);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
        assert!(st.is_ok());
        let (r, st) = Decimal128::ONE.max_magnitude(Decimal128::SIGNALING_NAN);
        assert!(r.is_nan());
        assert!(st.invalid());
        let (r, _) = Decimal128::NAN.min_magnitude(Decimal128::NAN);
        assert!(r.is_nan());
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
