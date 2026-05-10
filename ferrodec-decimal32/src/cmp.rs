//! IEEE 754-2019 comparison and ordering for [`Decimal32`].
//!
//! * [`Decimal32::partial_cmp`] — §5.6.1 numeric comparison. Returns
//!   `(Option<Ordering>, Status)` so signaling-NaN inputs can raise
//!   `INVALID` while the comparison still reports incomparability via
//!   `None`.
//! * [`Decimal32::total_cmp`] — §5.10 totalOrder predicate. Returns
//!   `Ordering` directly (always defined, NaN payload-aware).
//! * [`Decimal32::compare_total_magnitude`] — §5.10
//!   totalOrderMag: the totalOrder predicate applied to `|a|` vs
//!   `|b|`.
//! * [`Decimal32::min`] / [`Decimal32::max`] — §5.3.1 minimum and
//!   maximum operations. NaN propagates per the §6.2.3 rule.

use core::cmp::Ordering;

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::Status;

impl Decimal32 {
    /// IEEE 754-2019 §5.6.1 numeric comparison.
    ///
    /// Returns `(None, Status::INVALID)` if either operand is a
    /// signaling NaN; `(None, Status::OK)` if either is a quiet NaN;
    /// otherwise `(Some(Ordering), Status::OK)`. `+0` and `−0`
    /// compare equal numerically.
    #[must_use]
    pub fn partial_cmp(self, other: Self) -> (Option<Ordering>, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        let a_snan = matches!(ca, Class::SignalingNaN { .. });
        let b_snan = matches!(cb, Class::SignalingNaN { .. });
        let a_nan = a_snan || matches!(ca, Class::QuietNaN { .. });
        let b_nan = b_snan || matches!(cb, Class::QuietNaN { .. });

        if a_nan || b_nan {
            let status = if a_snan || b_snan {
                Status::INVALID
            } else {
                Status::OK
            };
            return (None, status);
        }

        (Some(numeric_cmp_non_nan(ca, cb)), Status::OK)
    }

    /// IEEE 754-2019 §5.10 totalOrder predicate.
    ///
    /// Defines a unique ordering across the entire `Decimal32` value
    /// space. Highlights:
    ///
    /// * Negative quiet NaN < negative signaling NaN < negative
    ///   infinity < negative finite < `−0` < `+0` < positive finite
    ///   < positive infinity < positive signaling NaN < positive
    ///   quiet NaN.
    /// * Within a sign / NaN-kind class, NaN payloads order
    ///   ascending (lower payload first).
    /// * Cohorts of the same numeric value are distinguished by
    ///   biased exponent: positive values use ascending biased exp;
    ///   negative values use descending biased exp (so the
    ///   "wider" — longer-coefficient-with-trailing-zeros — encoding
    ///   compares less).
    ///
    /// Always defined; never raises a status flag.
    #[must_use]
    pub fn total_cmp(self, other: Self) -> Ordering {
        total_cmp_inner(self, other)
    }

    /// IEEE 754-2019 §5.10 totalOrderMag predicate: the totalOrder
    /// predicate applied to `|self|` and `|other|`.
    #[must_use]
    pub fn compare_total_magnitude(self, other: Self) -> Ordering {
        total_cmp_inner(self.abs(), other.abs())
    }

    /// `min(x, y)` per IEEE 754-2019 §9.6 `minimumNumber` (GDA's
    /// `min` / decTest's `min`).
    ///
    /// * Any signaling NaN poisons the result: returns NaN with
    ///   `INVALID` raised.
    /// * Both operands quiet NaN → NaN.
    /// * One operand quiet NaN, the other finite → the finite
    ///   operand (qNaN is "missing value").
    /// * Otherwise → the operand that is *smaller in totalOrder*.
    ///   This handles equal-magnitude cohort tie-breaking (e.g.
    ///   `min(+0, −0) = −0`, `min(1.0, 1.00) = 1.0`).
    ///
    /// Matches `Decimal128`'s `min` exactly so values flow across
    /// precisions without semantic drift.
    #[inline]
    #[must_use]
    pub fn min(self, other: Self) -> (Self, Status) {
        if self.is_signaling_nan() || other.is_signaling_nan() {
            return (Self::NAN, Status::INVALID);
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

    /// `max(x, y)` — symmetric to [`Decimal32::min`]; matches IEEE
    /// 754-2019 §9.6 `maximumNumber`.
    #[inline]
    #[must_use]
    pub fn max(self, other: Self) -> (Self, Status) {
        if self.is_signaling_nan() || other.is_signaling_nan() {
            return (Self::NAN, Status::INVALID);
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
}

/// Numeric compare assuming neither operand is NaN.
fn numeric_cmp_non_nan(a: Class, b: Class) -> Ordering {
    use Class::Zero;

    let sign_a = sign_of_class(a);
    let sign_b = sign_of_class(b);

    let a_zero = matches!(a, Zero { .. });
    let b_zero = matches!(b, Zero { .. });

    // +0 == -0 numerically.
    if a_zero && b_zero {
        return Ordering::Equal;
    }

    // Different signs: positive > negative (zeros already handled).
    if sign_a != sign_b {
        return if sign_a {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }

    // Same sign: compare magnitudes; result reverses for negatives.
    let mag = magnitude_cmp_non_nan(a, b);
    if sign_a {
        mag.reverse()
    } else {
        mag
    }
}

/// Compare magnitudes (absolute values) of two non-NaN classes.
fn magnitude_cmp_non_nan(a: Class, b: Class) -> Ordering {
    use Class::{Finite, Infinity, Zero};

    match (a, b) {
        (Zero { .. }, Zero { .. }) => Ordering::Equal,
        (Zero { .. }, _) => Ordering::Less,
        (_, Zero { .. }) => Ordering::Greater,
        (Infinity { .. }, Infinity { .. }) => Ordering::Equal,
        (Infinity { .. }, _) => Ordering::Greater,
        (_, Infinity { .. }) => Ordering::Less,
        (
            Finite {
                biased_exp: ea,
                coefficient: ca,
                ..
            },
            Finite {
                biased_exp: eb,
                coefficient: cb,
                ..
            },
        ) => finite_magnitude_cmp(ca, ea as i32 - BIAS as i32, cb, eb as i32 - BIAS as i32),
        // Mixed Zero × non-Finite or NaN already handled by the
        // dispatcher; remaining combinations are unreachable here.
        _ => unreachable!("nan_propagate handles NaN before magnitude_cmp"),
    }
}

fn finite_magnitude_cmp(coef_a: u32, exp_a: i32, coef_b: u32, exp_b: i32) -> Ordering {
    let da = decimal_digit_count(coef_a);
    let db = decimal_digit_count(coef_b);
    let adj_a = exp_a + da as i32 - 1;
    let adj_b = exp_b + db as i32 - 1;
    if adj_a != adj_b {
        return adj_a.cmp(&adj_b);
    }
    // Same adjusted exponent. Pad the shorter coefficient with
    // trailing zeros so both sit at the same digit count, then
    // compare directly.
    let (na, nb) = if da < db {
        let factor = 10u64.pow(db - da);
        (u64::from(coef_a) * factor, u64::from(coef_b))
    } else {
        let factor = 10u64.pow(da - db);
        (u64::from(coef_a), u64::from(coef_b) * factor)
    };
    na.cmp(&nb)
}

/// Sign of a non-NaN class.
fn sign_of_class(c: Class) -> bool {
    match c {
        Class::Zero { sign, .. } | Class::Infinity { sign } | Class::Finite { sign, .. } => sign,
        Class::QuietNaN { sign, .. } | Class::SignalingNaN { sign, .. } => sign,
    }
}

fn total_cmp_inner(a: Decimal32, b: Decimal32) -> Ordering {
    let ca = classify_bits(a.0);
    let cb = classify_bits(b.0);

    // Sort key (sign-aware NaN-aware ranking).
    fn rank(c: Class) -> i8 {
        match c {
            // Negative quiet NaN < negative signaling NaN < ...
            Class::QuietNaN { sign: true, .. } => -5,
            Class::SignalingNaN { sign: true, .. } => -4,
            Class::Infinity { sign: true } => -3,
            Class::Finite { sign: true, .. } | Class::Zero { sign: true, .. } => -2,
            // -0 ranks below +0; we'll refine within rank(-2) for
            // finites and within the zero subcategory for zeros.
            Class::Zero { sign: false, .. } | Class::Finite { sign: false, .. } => 2,
            Class::Infinity { sign: false } => 3,
            Class::SignalingNaN { sign: false, .. } => 4,
            Class::QuietNaN { sign: false, .. } => 5,
        }
    }

    let ra = rank(ca);
    let rb = rank(cb);
    if ra != rb {
        return ra.cmp(&rb);
    }

    // Same rank: refine.
    match (ca, cb) {
        (
            Class::QuietNaN { sign, payload: pa },
            Class::QuietNaN {
                sign: _,
                payload: pb,
            },
        )
        | (
            Class::SignalingNaN { sign, payload: pa },
            Class::SignalingNaN {
                sign: _,
                payload: pb,
            },
        ) => {
            // Negative-NaN: descending payload (higher payload <
            //               lower); positive-NaN: ascending payload.
            if sign {
                pb.cmp(&pa)
            } else {
                pa.cmp(&pb)
            }
        }
        (Class::Infinity { .. }, Class::Infinity { .. }) => Ordering::Equal,
        (
            Class::Zero {
                biased_exp: ea,
                sign: sa,
            },
            Class::Zero {
                biased_exp: eb,
                sign: sb,
            },
        ) => {
            if sa != sb {
                // -0 < +0
                if sa {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            } else if sa {
                // Both -0: descending biased_exp (higher exp first).
                eb.cmp(&ea)
            } else {
                // Both +0: ascending biased_exp.
                ea.cmp(&eb)
            }
        }
        (
            Class::Finite {
                biased_exp: ea,
                coefficient: ca_,
                sign: sa,
            },
            Class::Finite {
                biased_exp: eb,
                coefficient: cb_,
                sign: sb,
            },
        ) => {
            // Same sign (else rank would differ).
            debug_assert_eq!(sa, sb);
            let mag =
                finite_magnitude_cmp(ca_, ea as i32 - BIAS as i32, cb_, eb as i32 - BIAS as i32);
            if mag == Ordering::Equal {
                // Same numeric value, different cohort: compare
                // biased_exp. Positive: ascending. Negative:
                // descending.
                if sa {
                    eb.cmp(&ea)
                } else {
                    ea.cmp(&eb)
                }
            } else if sa {
                mag.reverse()
            } else {
                mag
            }
        }
        // Mixed Zero / Finite within rank ±2.
        (Class::Zero { sign: sa, .. }, Class::Finite { .. }) => {
            // Zero and non-zero same-sign finite share rank ±2;
            // |zero| < |finite|.
            if sa {
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
        _ => unreachable!("rank handled all other class pairings"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn partial_cmp_basic() {
        let (cmp, s) = from_int(1, 0).partial_cmp(from_int(2, 0));
        assert_eq!(cmp, Some(Ordering::Less));
        assert!(s.is_ok());

        let (cmp, _) = from_int(2, 0).partial_cmp(from_int(1, 0));
        assert_eq!(cmp, Some(Ordering::Greater));

        let (cmp, _) = from_int(1, 0).partial_cmp(from_int(1, 0));
        assert_eq!(cmp, Some(Ordering::Equal));
    }

    #[test]
    fn partial_cmp_signs() {
        // -1 < +1
        let (cmp, _) = from_int(-1, 0).partial_cmp(from_int(1, 0));
        assert_eq!(cmp, Some(Ordering::Less));

        // -2 < -1
        let (cmp, _) = from_int(-2, 0).partial_cmp(from_int(-1, 0));
        assert_eq!(cmp, Some(Ordering::Less));
    }

    #[test]
    fn partial_cmp_zero_cohorts_numerically_equal() {
        // +0 == -0
        let (cmp, _) = Decimal32::ZERO.partial_cmp(Decimal32::NEG_ZERO);
        assert_eq!(cmp, Some(Ordering::Equal));

        // 0E+0 == 0E-1 (numerically)
        let z1 = Decimal32::try_new(0, 0).unwrap();
        let z2 = Decimal32::try_new(0, -1).unwrap();
        let (cmp, _) = z1.partial_cmp(z2);
        assert_eq!(cmp, Some(Ordering::Equal));
    }

    #[test]
    fn partial_cmp_finite_cohorts() {
        // 1.0 == 1.00 numerically
        let a = from_int(10, -1);
        let b = from_int(100, -2);
        let (cmp, _) = a.partial_cmp(b);
        assert_eq!(cmp, Some(Ordering::Equal));
    }

    #[test]
    fn partial_cmp_infinity() {
        let (cmp, _) = Decimal32::INFINITY.partial_cmp(from_int(1, 0));
        assert_eq!(cmp, Some(Ordering::Greater));

        let (cmp, _) = Decimal32::NEG_INFINITY.partial_cmp(from_int(1, 0));
        assert_eq!(cmp, Some(Ordering::Less));

        let (cmp, _) = Decimal32::INFINITY.partial_cmp(Decimal32::INFINITY);
        assert_eq!(cmp, Some(Ordering::Equal));
    }

    #[test]
    fn partial_cmp_nan() {
        let (cmp, s) = Decimal32::NAN.partial_cmp(from_int(1, 0));
        assert_eq!(cmp, None);
        assert!(s.is_ok());

        let (cmp, s) = Decimal32::SIGNALING_NAN.partial_cmp(from_int(1, 0));
        assert_eq!(cmp, None);
        assert!(s.invalid());

        let (cmp, _) = from_int(1, 0).partial_cmp(Decimal32::NAN);
        assert_eq!(cmp, None);
    }

    #[test]
    fn total_cmp_negative_quiet_nan_lowest() {
        let neg_nan = Decimal32::NAN.neg();
        assert_eq!(neg_nan.total_cmp(Decimal32::NEG_INFINITY), Ordering::Less);
        assert_eq!(neg_nan.total_cmp(Decimal32::INFINITY), Ordering::Less);
    }

    #[test]
    fn total_cmp_zeros_distinguished() {
        // -0 < +0 in totalOrder.
        assert_eq!(
            Decimal32::NEG_ZERO.total_cmp(Decimal32::ZERO),
            Ordering::Less
        );
        assert_eq!(
            Decimal32::ZERO.total_cmp(Decimal32::NEG_ZERO),
            Ordering::Greater
        );
    }

    #[test]
    fn total_cmp_cohort_order_positive() {
        // 1.00 (= 100E-2) and 1.0 (= 10E-1) are numerically equal but
        // total_cmp distinguishes them by biased_exp ascending for
        // positive values: 100E-2 has lower biased_exp than 10E-1.
        let wide = from_int(100, -2);
        let narrow = from_int(10, -1);
        // wide has biased_exp = BIAS-2 = 99; narrow has biased_exp =
        // BIAS-1 = 100. wide.biased < narrow.biased so wide < narrow.
        assert_eq!(wide.total_cmp(narrow), Ordering::Less);
    }

    #[test]
    fn min_max_basic() {
        let (r, _) = from_int(1, 0).min(from_int(2, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        let (r, _) = from_int(1, 0).max(from_int(2, 0));
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
    }

    #[test]
    fn min_max_zero_signs() {
        // min(+0, -0) = -0
        let (r, _) = Decimal32::ZERO.min(Decimal32::NEG_ZERO);
        assert!(r.is_zero() && r.is_sign_negative());

        // max(+0, -0) = +0
        let (r, _) = Decimal32::ZERO.max(Decimal32::NEG_ZERO);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn min_max_nan() {
        // §9.6 minimumNumber: a *quiet* NaN is "missing value", so
        // min(qNaN, x) returns x with no exception. Both operands
        // qNaN → NaN.
        let (r, s) = Decimal32::NAN.min(from_int(1, 0));
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());
        assert!(s.is_ok());

        let (r, s) = from_int(7, 0).max(Decimal32::NAN);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
        assert!(s.is_ok());

        let (r, s) = Decimal32::NAN.min(Decimal32::NAN);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        // sNaN still poisons: result is NaN + INVALID regardless of
        // the other operand.
        let (r, s) = Decimal32::SIGNALING_NAN.min(from_int(1, 0));
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = from_int(1, 0).max(Decimal32::SIGNALING_NAN);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn compare_total_magnitude_ignores_sign() {
        // |-3| == |+3|
        assert_eq!(
            from_int(-3, 0).compare_total_magnitude(from_int(3, 0)),
            Ordering::Equal
        );
        // |-2| < |3|
        assert_eq!(
            from_int(-2, 0).compare_total_magnitude(from_int(3, 0)),
            Ordering::Less
        );
    }
}
