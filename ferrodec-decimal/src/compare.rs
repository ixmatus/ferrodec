//! Comparison, selection, and sign-copy operations.
//!
//! `compare` is the numeric comparison (NaN-aware, returning a NaN when either
//! operand is a NaN); `compare_total` is the IEEE 754 total ordering, which
//! orders every value including NaNs and never returns a NaN. `maxnum` and
//! `minnum` select an operand (ignoring a quiet NaN) and round it to the
//! context. The
//! copy operations manipulate only the sign bit and take no context.

use crate::arith::{nan_result, quiet_from};
use crate::{Context, Decimal, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

impl Decimal {
    /// Numeric comparison: `-1`, `0`, or `1` as a decimal, or a NaN when
    /// either operand is a NaN (a signaling NaN also signals invalid).
    #[must_use]
    pub fn compare(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        (ordering_to_decimal(numeric_cmp(self, other)), Status::OK)
    }

    /// The IEEE 754 total ordering of `self` and `other`: `-1`, `0`, or `1`.
    /// Every value is ordered, including NaNs (by sign then payload), so this
    /// never returns a NaN and takes no context.
    #[must_use]
    pub fn compare_total(&self, other: &Self) -> Decimal {
        ordering_to_decimal(total_cmp(self, other))
    }

    /// The larger of `self` and `other`, rounded to the context (the General
    /// Decimal Arithmetic `max` / IEEE 754-2019 `maximumNumber` operation). A
    /// quiet NaN operand is ignored in favor of a number; a signaling NaN
    /// signals invalid.
    ///
    /// Named `maxnum`, not `max`, because [`Decimal`] now implements [`Ord`]
    /// (ADR-0055), whose provided `Ord::max` takes `self` by value and would
    /// otherwise shadow this context-aware operation at every value receiver.
    #[must_use]
    pub fn maxnum(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        select(self, other, ctx, true)
    }

    /// The smaller of `self` and `other`, rounded to the context (the General
    /// Decimal Arithmetic `min` / IEEE 754-2019 `minimumNumber` operation), with
    /// the same NaN handling, and the same `Ord`-collision naming rationale, as
    /// [`maxnum`](Self::maxnum).
    #[must_use]
    pub fn minnum(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        select(self, other, ctx, false)
    }

    /// `self` with a positive sign (no context, no rounding).
    #[must_use]
    pub fn copy_abs(&self) -> Decimal {
        self.with_sign(false)
    }

    /// `self` with its sign inverted (no context, no rounding).
    #[must_use]
    pub fn copy_negate(&self) -> Decimal {
        self.with_sign(!self.is_negative())
    }

    /// `self` with the sign of `other` (no context, no rounding).
    #[must_use]
    pub fn copy_sign(&self, other: &Self) -> Decimal {
        self.with_sign(other.is_negative())
    }

    /// An unaltered copy of `self`. Unlike the other copy operations this
    /// touches nothing, not even the sign, and a signaling NaN stays signaling;
    /// it never signals.
    #[must_use]
    pub fn copy(&self) -> Decimal {
        self.clone()
    }

    /// Like [`compare`](Self::compare) but a quiet NaN operand also signals
    /// invalid, so every NaN raises `Invalid_operation`. The numeric result is
    /// otherwise identical.
    #[must_use]
    pub fn compare_signal(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some((nan, _)) = nan_result(self, other, ctx) {
            return (nan, Status::INVALID);
        }
        (ordering_to_decimal(numeric_cmp(self, other)), Status::OK)
    }

    /// The IEEE 754 total ordering of the magnitudes `|self|` and `|other|`:
    /// `-1`, `0`, or `1`. Like [`compare_total`](Self::compare_total) but on the
    /// absolute values, so it never returns a NaN and takes no context.
    #[must_use]
    pub fn compare_total_mag(&self, other: &Self) -> Decimal {
        ordering_to_decimal(total_cmp(&self.copy_abs(), &other.copy_abs()))
    }

    /// The operand with the larger magnitude, rounded to the context; on equal
    /// magnitude this is [`maxnum`](Self::maxnum). NaN handling matches `maxnum`
    /// (a quiet NaN is ignored, a signaling NaN signals invalid).
    #[must_use]
    pub fn max_magnitude(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        select_magnitude(self, other, ctx, true)
    }

    /// The operand with the smaller magnitude, rounded to the context; on equal
    /// magnitude this is [`minnum`](Self::minnum), with the same NaN handling.
    #[must_use]
    pub fn min_magnitude(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        select_magnitude(self, other, ctx, false)
    }

    /// Whether `self` and `other` have the same quantum (exponent): the decimal
    /// `1` if so, else `0`. Two NaNs share a quantum, as do two infinities; a
    /// NaN and a number do not. Never signals and takes no context.
    #[must_use]
    pub fn same_quantum(&self, other: &Self) -> Decimal {
        let same = match (self.finite_parts(), other.finite_parts()) {
            (Some((_, _, ea)), Some((_, _, eb))) => ea == eb,
            _ => (self.is_nan() && other.is_nan()) || (self.is_infinite() && other.is_infinite()),
        };
        if same {
            Decimal::finite(false, DecBig::from_u32(1), 0)
        } else {
            Decimal::zero()
        }
    }
}

/// `Ord` is the IEEE 754 `totalOrder` (the same relation as
/// [`compare_total`](Decimal::compare_total)), not the numeric comparison.
/// totalOrder is a total order on *every* value, NaNs and infinities included,
/// so the impl is total and never panics, and it is lawful against the derived
/// structural [`Eq`]: `total_cmp` returns `Equal` exactly when the operands are
/// structurally identical (same sign, same category, equal magnitude *and*
/// equal exponent for finites, equal payload for NaNs), so
/// `a == b ⟺ a.cmp(&b) == Equal` holds. A derived `Ord` would instead compare
/// the `Repr` fields lexicographically (sign, then coefficient, then exponent),
/// which is numerically meaningless; hence the hand-written delegation.
///
/// totalOrder distinguishes cohort members (`1.0` ranks above `1.00`) and
/// signed zeros (`-0` below `+0`); for distinct numeric constants it coincides
/// with numeric order. Numeric (cohort-collapsing, NaN-aware) comparison stays
/// available through [`compare`](Decimal::compare) and
/// [`compare_total`](Decimal::compare_total).
impl PartialOrd for Decimal {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        total_cmp(self, other)
    }
}

fn ordering_to_decimal(ord: Ordering) -> Decimal {
    match ord {
        Ordering::Less => Decimal::finite(true, DecBig::from_u32(1), 0),
        Ordering::Equal => Decimal::finite(false, DecBig::zero(), 0),
        Ordering::Greater => Decimal::finite(false, DecBig::from_u32(1), 0),
    }
}

/// Numeric comparison of two non-NaN values (signed, with `-0 == +0`).
pub(crate) fn numeric_cmp(a: &Decimal, b: &Decimal) -> Ordering {
    let (sa, sb) = (a.is_negative(), b.is_negative());
    match (a.is_infinite(), b.is_infinite()) {
        (true, true) => {
            return match (sa, sb) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            }
        }
        (true, false) => {
            return if sa {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (false, true) => {
            return if sb {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (false, false) => {}
    }
    let (sa, ca, ea) = a.finite_parts().expect("finite");
    let (sb, cb, eb) = b.finite_parts().expect("finite");
    if ca.is_zero() && cb.is_zero() {
        return Ordering::Equal;
    }
    if sa != sb {
        return if sa {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let mag = abs_value_cmp(ca, ea, cb, eb);
    if sa {
        mag.reverse()
    } else {
        mag
    }
}

/// Compare the numeric magnitudes of two finite coefficients.
fn abs_value_cmp(ca: &DecBig, ea: i32, cb: &DecBig, eb: i32) -> Ordering {
    match (ca.is_zero(), cb.is_zero()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (false, false) => {}
    }
    // Compare adjusted exponents first; only equal magnitudes need alignment,
    // and then the exponent gap is bounded by the digit counts.
    let adj_a = i64::from(ea) + ca.decimal_digit_count() as i64 - 1;
    let adj_b = i64::from(eb) + cb.decimal_digit_count() as i64 - 1;
    if adj_a != adj_b {
        return adj_a.cmp(&adj_b);
    }
    let min_e = ea.min(eb);
    ca.mul_pow10((ea - min_e) as u32)
        .cmp_ref(&cb.mul_pow10((eb - min_e) as u32))
}

/// Total-order category (for a positive sign): numbers and infinities rank
/// below signaling NaNs, which rank below quiet NaNs.
fn total_category(d: &Decimal) -> u8 {
    if d.is_signaling_nan() {
        1
    } else if d.is_nan() {
        2
    } else {
        0
    }
}

/// IEEE 754 total ordering.
fn total_cmp(a: &Decimal, b: &Decimal) -> Ordering {
    let (sa, sb) = (a.is_negative(), b.is_negative());
    if sa != sb {
        return if sa {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let ord = total_rank_positive(a, b);
    if sa {
        ord.reverse()
    } else {
        ord
    }
}

/// Total ordering within one sign, as if both operands were positive.
fn total_rank_positive(a: &Decimal, b: &Decimal) -> Ordering {
    let (ca, cb) = (total_category(a), total_category(b));
    if ca != cb {
        return ca.cmp(&cb);
    }
    if ca == 0 {
        match (a.is_infinite(), b.is_infinite()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                let (_, pca, ea) = a.finite_parts().expect("finite");
                let (_, pcb, eb) = b.finite_parts().expect("finite");
                // Equal numeric value: the larger exponent ranks higher.
                match abs_value_cmp(pca, ea, pcb, eb) {
                    Ordering::Equal => ea.cmp(&eb),
                    other => other,
                }
            }
        }
    } else {
        // Two NaNs of the same class: order by payload.
        let (_, _, pa) = a.nan_parts().expect("nan");
        let (_, _, pb) = b.nan_parts().expect("nan");
        pa.cmp_ref(pb)
    }
}

/// Shared `maxnum` / `minnum`: ignore a quiet NaN, signal invalid on a
/// signaling NaN, and otherwise pick by numeric value (breaking ties with the
/// total order), then round the chosen operand to the context.
fn select(a: &Decimal, b: &Decimal, ctx: &Context, is_max: bool) -> (Decimal, Status) {
    if a.is_signaling_nan() {
        return (quiet_from(a, ctx), Status::INVALID);
    }
    if b.is_signaling_nan() {
        return (quiet_from(b, ctx), Status::INVALID);
    }
    match (a.is_nan(), b.is_nan()) {
        (true, true) => return (quiet_from(a, ctx), Status::OK),
        (true, false) => return rounded_pick(b, ctx),
        (false, true) => return rounded_pick(a, ctx),
        (false, false) => {}
    }
    let pick_a = match numeric_cmp(a, b) {
        Ordering::Greater => is_max,
        Ordering::Less => !is_max,
        Ordering::Equal => {
            let total = total_cmp(a, b);
            if is_max {
                total == Ordering::Greater
            } else {
                total == Ordering::Less
            }
        }
    };
    if pick_a {
        rounded_pick(a, ctx)
    } else {
        rounded_pick(b, ctx)
    }
}

/// Shared `maxMagnitude` / `minMagnitude`: pick the operand with the larger (or
/// smaller) magnitude, breaking an equal-magnitude tie with the value-based
/// `maxnum` / `minnum`, then round the pick to the context. NaN handling
/// matches [`select`].
fn select_magnitude(a: &Decimal, b: &Decimal, ctx: &Context, is_max: bool) -> (Decimal, Status) {
    if a.is_signaling_nan() {
        return (quiet_from(a, ctx), Status::INVALID);
    }
    if b.is_signaling_nan() {
        return (quiet_from(b, ctx), Status::INVALID);
    }
    match (a.is_nan(), b.is_nan()) {
        (true, true) => return (quiet_from(a, ctx), Status::OK),
        (true, false) => return rounded_pick(b, ctx),
        (false, true) => return rounded_pick(a, ctx),
        (false, false) => {}
    }
    let pick_a = match numeric_cmp(&a.copy_abs(), &b.copy_abs()) {
        Ordering::Greater => is_max,
        Ordering::Less => !is_max,
        // Equal magnitude: defer to the value-based max / min for the sign and
        // cohort tie-break.
        Ordering::Equal => return select(a, b, ctx, is_max),
    };
    if pick_a {
        rounded_pick(a, ctx)
    } else {
        rounded_pick(b, ctx)
    }
}

/// Round the chosen operand to the context, preserving a zero's sign. `plus`
/// rounds to the context but resolves a zero's sign through the add-from-zero
/// rule (so `+(-0)` is `+0`); maxnum / minnum return the selected operand
/// keeping its own sign, so a zero result is rebuilt with it (the exponent and status from
/// the rounding stand).
fn rounded_pick(d: &Decimal, ctx: &Context) -> (Decimal, Status) {
    let (r, status) = d.plus(ctx);
    if d.is_zero() {
        if let Some((_, coeff, exp)) = r.finite_parts() {
            return (Decimal::finite(d.is_negative(), coeff.clone(), exp), status);
        }
    }
    (r, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;

    fn ctx() -> Context {
        Context::new(
            core::num::NonZeroU32::new(9).unwrap(),
            9999,
            -9999,
            Rounding::HalfEven,
        )
    }

    fn fin(sign: bool, coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(sign, DecBig::from_u128(coeff), exp)
    }

    fn minus_one() -> Decimal {
        fin(true, 1, 0)
    }
    fn zero() -> Decimal {
        fin(false, 0, 0)
    }
    fn one() -> Decimal {
        fin(false, 1, 0)
    }

    #[test]
    fn compare_numeric() {
        let c = ctx();
        assert_eq!(
            fin(false, 2, 0).compare(&fin(false, 3, 0), &c).0,
            minus_one()
        );
        assert_eq!(fin(false, 3, 0).compare(&fin(false, 2, 0), &c).0, one());
        // 1.0 == 1.00 numerically; -0 == +0.
        assert_eq!(
            fin(false, 10, -1).compare(&fin(false, 100, -2), &c).0,
            zero()
        );
        assert_eq!(fin(true, 0, 0).compare(&fin(false, 0, 0), &c).0, zero());
        // NaN comparison yields NaN.
        assert!(Decimal::quiet_nan(false, DecBig::zero())
            .compare(&one(), &c)
            .0
            .is_nan());
    }

    #[test]
    fn compare_total_orders_cohorts_and_nans() {
        // 1.00 < 1.0 in the total order (smaller exponent ranks lower).
        assert_eq!(
            fin(false, 100, -2).compare_total(&fin(false, 10, -1)),
            minus_one()
        );
        // -0 < +0 in the total order.
        assert_eq!(
            fin(true, 0, 0).compare_total(&fin(false, 0, 0)),
            minus_one()
        );
        // A signaling NaN ranks above every number; a quiet NaN above that.
        let qn = Decimal::quiet_nan(false, DecBig::zero());
        let sn = Decimal::signaling_nan(false, DecBig::zero());
        assert_eq!(one().compare_total(&sn), minus_one());
        assert_eq!(sn.compare_total(&qn), minus_one());
    }

    #[test]
    fn max_min_and_nan_handling() {
        let c = ctx();
        assert_eq!(
            fin(false, 2, 0).maxnum(&fin(false, 7, 0), &c).0,
            fin(false, 7, 0)
        );
        assert_eq!(
            fin(false, 2, 0).minnum(&fin(false, 7, 0), &c).0,
            fin(false, 2, 0)
        );
        // A quiet NaN is ignored.
        let qn = Decimal::quiet_nan(false, DecBig::zero());
        assert_eq!(qn.maxnum(&fin(false, 5, 0), &c).0, fin(false, 5, 0));
        // A signaling NaN signals invalid.
        let sn = Decimal::signaling_nan(false, DecBig::zero());
        assert!(sn.maxnum(&fin(false, 5, 0), &c).1.invalid());
    }

    #[test]
    fn copy_operations() {
        let neg = fin(true, 5, -1);
        assert_eq!(neg.copy_abs(), fin(false, 5, -1));
        assert_eq!(neg.copy_negate(), fin(false, 5, -1));
        assert_eq!(fin(false, 5, -1).copy_sign(&neg), fin(true, 5, -1));
        // Copy operates on NaNs too, touching only the sign.
        let qn = Decimal::quiet_nan(false, DecBig::from_u32(3));
        assert!(qn.copy_negate().is_negative());
    }

    #[test]
    fn max_min_preserve_negative_zero() {
        let c = ctx();
        // The selected operand keeps its own sign; rounding the pick via `plus`
        // would resolve -0 to +0, so a zero is rebuilt with the operand's sign.
        assert_eq!(
            fin(true, 0, 0).maxnum(&fin(true, 0, 0), &c).0,
            fin(true, 0, 0)
        );
        assert_eq!(
            fin(false, 0, 0).minnum(&fin(true, 0, 0), &c).0,
            fin(true, 0, 0)
        );
        // Equal negative zeros of different exponent: max returns -0.0 (E-1).
        assert_eq!(
            fin(true, 0, 0).maxnum(&fin(true, 0, -1), &c).0,
            fin(true, 0, -1)
        );
    }

    #[test]
    fn compare_signal_invalid_on_any_nan() {
        let c = ctx();
        // A quiet NaN, which `compare` passes with OK, signals invalid here.
        let qn = Decimal::quiet_nan(false, DecBig::zero());
        let (r, s) = qn.compare_signal(&one(), &c);
        assert!(r.is_nan() && s.invalid());
        // A normal comparison is unaffected.
        assert_eq!(
            fin(false, 2, 0).compare_signal(&fin(false, 3, 0), &c).0,
            minus_one()
        );
    }

    #[test]
    fn compare_total_magnitude_orders_by_abs() {
        // |-1| > |0|; |-2| < |3|.
        assert_eq!(fin(true, 1, 0).compare_total_mag(&zero()), one());
        assert_eq!(
            fin(true, 2, 0).compare_total_mag(&fin(false, 3, 0)),
            minus_one()
        );
    }

    #[test]
    fn magnitude_selection() {
        let c = ctx();
        // maxMagnitude picks the larger magnitude regardless of sign.
        assert_eq!(
            fin(true, 2, 0).max_magnitude(&fin(false, 1, 0), &c).0,
            fin(true, 2, 0)
        );
        // minMagnitude picks the smaller magnitude.
        assert_eq!(
            fin(true, 2, 0).min_magnitude(&fin(false, 1, 0), &c).0,
            fin(false, 1, 0)
        );
        // Equal magnitude defers to the value-based max / min.
        assert_eq!(
            fin(true, 1, 0).max_magnitude(&fin(false, 1, 0), &c).0,
            fin(false, 1, 0)
        );
    }

    #[test]
    fn same_quantum_compares_exponents() {
        assert_eq!(fin(false, 100, -1).same_quantum(&fin(false, 5, -1)), one());
        assert_eq!(fin(false, 1, 0).same_quantum(&fin(false, 1, 1)), zero());
        // Two NaNs (any kind) share a quantum; a NaN and a number do not.
        let qn = Decimal::quiet_nan(false, DecBig::zero());
        assert_eq!(
            qn.same_quantum(&Decimal::signaling_nan(false, DecBig::zero())),
            one()
        );
        assert_eq!(qn.same_quantum(&one()), zero());
        // Two infinities share a quantum.
        assert_eq!(
            Decimal::infinity(false).same_quantum(&Decimal::infinity(true)),
            one()
        );
    }

    #[test]
    fn ord_agrees_with_compare_total() {
        // `Ord` is the totalOrder, so `cmp` must match `compare_total`'s decimal
        // sign across the representative cases.
        let cases = [
            (fin(false, 100, -2), fin(false, 10, -1)), // 1.00 < 1.0 (cohort)
            (fin(true, 0, 0), fin(false, 0, 0)),       // -0 < +0
            (fin(false, 2, 0), fin(false, 3, 0)),      // 2 < 3
            (fin(true, 5, 0), fin(false, 1, 0)),       // -5 < 1
        ];
        for (a, b) in cases {
            assert_eq!(a.cmp(&b), Ordering::Less);
            assert_eq!(a.compare_total(&b), minus_one());
            assert_eq!(b.cmp(&a), Ordering::Greater);
            assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
        }
    }

    #[test]
    fn ord_is_lawful_against_eq() {
        // `a == b  ⟺  a.cmp(&b) == Equal` for the structural `Eq`.
        // A cohort pair is distinct under totalOrder, so it is *not* `Equal`.
        let (a, b) = (fin(false, 10, -1), fin(false, 100, -2)); // 1.0 vs 1.00
        assert_ne!(a, b);
        assert_ne!(a.cmp(&b), Ordering::Equal);
        // -0 and +0 are likewise distinct structurally and under the order.
        let (nz, pz) = (fin(true, 0, 0), fin(false, 0, 0));
        assert_ne!(nz, pz);
        assert_ne!(nz.cmp(&pz), Ordering::Equal);
        // Identical reprs compare Equal.
        assert_eq!(one(), one());
        assert_eq!(one().cmp(&one()), Ordering::Equal);
    }

    #[test]
    fn ord_sorts_and_keys_a_btree() {
        use alloc::vec::Vec;
        let mut v: Vec<Decimal> = alloc::vec![
            fin(false, 3, 0),
            fin(true, 1, 0),
            fin(false, 0, 0),
            fin(false, 2, 0),
            Decimal::infinity(false),
            Decimal::infinity(true),
        ];
        v.sort();
        assert_eq!(
            v,
            alloc::vec![
                Decimal::infinity(true),
                fin(true, 1, 0),
                fin(false, 0, 0),
                fin(false, 2, 0),
                fin(false, 3, 0),
                Decimal::infinity(false),
            ]
        );
        // Round-trip distinct values through a BTreeSet (exercises `Ord` keying).
        let set: alloc::collections::BTreeSet<Decimal> =
            [one(), zero(), minus_one()].into_iter().collect();
        assert_eq!(set.len(), 3);
        assert!(set.contains(&zero()));
    }

    #[test]
    fn ord_totally_orders_nans_without_panic() {
        let qn = Decimal::quiet_nan(false, DecBig::zero());
        let sn = Decimal::signaling_nan(false, DecBig::zero());
        // A number ranks below a signaling NaN, which ranks below a quiet NaN.
        assert_eq!(one().cmp(&sn), Ordering::Less);
        assert_eq!(sn.cmp(&qn), Ordering::Less);
        assert_eq!(qn.cmp(&qn), Ordering::Equal);
        // Infinities are ordered against finites.
        assert_eq!(Decimal::infinity(false).cmp(&one()), Ordering::Greater);
        assert_eq!(Decimal::infinity(true).cmp(&one()), Ordering::Less);
    }

    #[test]
    fn copy_is_pure_identity() {
        // copy preserves sign and payload and does not quiet a signaling NaN.
        let sn = Decimal::signaling_nan(true, DecBig::from_u32(7));
        assert_eq!(sn.copy(), sn);
        assert!(sn.copy().is_signaling_nan());
    }
}
