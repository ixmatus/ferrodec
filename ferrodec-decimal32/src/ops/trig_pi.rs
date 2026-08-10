//! IEEE 754-2019 §9.2 pi scaled family for [`Decimal32`] (ADR-0061
//! Track D D4): angles measured in revolutions rather than radians.
//!
//! Pure delegation onto the shared `ferrodec-transcend` kernels,
//! which resolve every §9.2.1 special value internally, classify the
//! exact set from the operands, and run the ADR-0059 escalation
//! ladder. The file is structured one function per block, in the
//! order the standard lists them; the forward trio (`sin_pi`,
//! `cos_pi`, `tan_pi`) lands at integration from its own slice.
//!
//! # Special cases (§9.2.1)
//!
//! * `asinPi(±0) = ±0`, `asinPi(±1) = ±1/2`; `|x| > 1` and `±∞` give
//!   a quiet NaN with `INVALID`.
//! * `acosPi(±0) = 1/2`, `acosPi(+1) = +0`, `acosPi(−1) = 1`; the
//!   same domain rule.
//! * `atanPi(±0) = ±0`, `atanPi(±∞) = ±1/2`, `atanPi(±1) = ±1/4`.
//! * `atan2Pi`'s rows are the quarter turns `±0`, `±1/4`, `±1/2`,
//!   `±3/4`, `±1`, all exact, where `atan2`'s corresponding rows were
//!   rounded `π` family irrationals carrying `INEXACT`.
//! * A signaling NaN gives a quiet NaN with `INVALID`; a quiet NaN
//!   propagates, in the fixed operand order for the binary case.
//!
//! # Exactness
//!
//! Every value listed above is exact and carries `Status::OK` (§7.5
//! forbids `INEXACT` on an exact result). The two non terminating
//! rationals in the family, `asinPi(±1/2) = ±1/6` and
//! `acosPi(±1/2) ∈ {1/3, 2/3}`, are correctly rounded and `INEXACT`
//! in every direction; ADR-0061's no ties theorem proves no operation
//! here has a nearest mode tie at any format.

use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `asinPi(self)`: the arcsine in revolutions,
    /// `asin(self)/π`, rounded by `rm`. Domain `[-1, +1]`; outside is
    /// NaN with `INVALID`. Range `[-1/2, +1/2]`.
    ///
    /// Correctly rounded at every rounding direction, at exact parity
    /// with the `Decimal128` parent. This module's header lists the
    /// §9.2.1 rows; the exact classification, the anchor derivations,
    /// and the error budget live on
    /// `ferrodec_transcend::inverse_trig_pi` and `ladder::ASINPI`.
    #[must_use]
    #[doc(alias = "asinPi")]
    #[doc(alias = "asinpi")]
    pub fn asin_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::asin_pi_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `acosPi(self)`: the arccosine in
    /// revolutions, `acos(self)/π`, rounded by `rm`. Domain
    /// `[-1, +1]`; outside is NaN with `INVALID`. Range `[0, 1]`.
    ///
    /// Correctly rounded at every rounding direction, at exact parity
    /// with the `Decimal128` parent; a tiny operand is decided by the
    /// ADR-0051 residual channel at the `1/2` anchor rather than by
    /// the ladder (`ladder::ACOSPI`).
    #[must_use]
    #[doc(alias = "acosPi")]
    #[doc(alias = "acospi")]
    pub fn acos_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::acos_pi_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atanPi(self)`: the arctangent in
    /// revolutions, `atan(self)/π`, rounded by `rm`. Range
    /// `[-1/2, +1/2]`, open on the finite operands.
    ///
    /// Correctly rounded at every rounding direction, at exact parity
    /// with the `Decimal128` parent; a large operand is decided by
    /// the ADR-0051 residual channel at the `±1/2` anchor
    /// (`ladder::ATANPI`).
    #[must_use]
    #[doc(alias = "atanPi")]
    #[doc(alias = "atanpi")]
    pub fn atan_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::atan_pi_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atan2Pi(self, x)`: the two argument
    /// arctangent in revolutions, `atan2(self, x)/π`, rounded by
    /// `rm`. Range `(-1, +1]`, quadrant per §9.2.1.
    ///
    /// Correctly rounded at every rounding direction, at exact parity
    /// with the `Decimal128` parent. The finite diagonals
    /// `|self| = |x|` are exact (`±1/4` for `x > 0`, `±3/4` for
    /// `x < 0`), and the two ADR-0051 residual channels cover an
    /// extreme ratio (`±1/2`) and a vanishing ratio against a
    /// negative abscissa (`±1`); see `ladder::ATAN2PI`.
    #[must_use]
    #[doc(alias = "atan2Pi")]
    #[doc(alias = "atan2pi")]
    pub fn atan2_pi(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::atan2_pi_kernel::<Decimal32>(self, x, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NE: RoundingMode = RoundingMode::NearestEven;

    fn parse(s: &str) -> Decimal32 {
        Decimal32::parse_str(s, NE).unwrap().0
    }

    /// Cohort-insensitive value equality (the IEEE `compare`).
    fn equal(a: Decimal32, b: Decimal32) -> bool {
        a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
    }

    #[test]
    fn quarter_turns_are_exact() {
        for (got, want) in [
            (parse("1").asin_pi(NE), "0.5"),
            (parse("-1").acos_pi(NE), "1"),
            (parse("1").atan_pi(NE), "0.25"),
            (Decimal32::INFINITY.atan_pi(NE), "0.5"),
        ] {
            let (r, st) = got;
            assert!(equal(r, parse(want)), "got {r}, want {want}");
            assert_eq!(st, Status::OK, "exact rows keep clean flags");
        }
    }

    #[test]
    fn one_sixth_is_inexact() {
        let (r, st) = parse("0.5").asin_pi(NE);
        assert!(equal(r, parse("0.1666667")), "got {r}");
        assert!(st.inexact());
    }

    #[test]
    fn atan2_pi_axis_row_is_exact() {
        let (r, st) = parse("0").atan2_pi(parse("-1"), NE);
        assert!(equal(r, parse("1")), "got {r}");
        assert_eq!(st, Status::OK);
    }
}
