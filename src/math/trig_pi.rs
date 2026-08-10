//! Delegating shim for the IEEE 754-2019 §9.2 pi scaled family in
//! `ferrodec-transcend` (ADR-0061 Track D D4). The public
//! `Decimal128` wrappers and their behaviour tests stay here as the
//! byte-identical regression gate.
//!
//! The file is structured one function per block, in the order the
//! standard lists them. The forward trio (`sin_pi`, `cos_pi`,
//! `tan_pi`) lands at integration from its own slice; the four
//! inverse wrappers below are complete on their own.
//!
//! These operations measure angles in **revolutions**, not radians:
//! `asin_pi(1) = 1/2` turn, `atan2_pi(1, 0) = 1/4` turn. The scaling
//! is what makes the special values exact, so a caller working in
//! revolutions (the family's dominant real use) gets a table of
//! clean quarter turns where the radian spelling could only offer
//! rounded multiples of `π`.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `asinPi(self)`: the arcsine in revolutions,
    /// `asin(self)/π`, rounded by `rm`. Domain `[-1, +1]`; outside is
    /// NaN with `INVALID`. Range `[-1/2, +1/2]`.
    ///
    /// ## Special values (§9.2.1)
    ///
    /// * `asinPi(±0) = ±0`, exact.
    /// * `asinPi(±1) = ±1/2`, exact, no exception. The radian
    ///   `asin(±1) = ±π/2` had to round an irrational.
    /// * `|x| > 1` and `±∞` give a quiet NaN with `INVALID`.
    /// * A signaling NaN gives a quiet NaN with `INVALID`; a quiet
    ///   NaN propagates.
    ///
    /// ## Exact values
    ///
    /// The whole exact set is `{±0 → ±0, ±1 → ±1/2}`. In particular
    /// `asinPi(±1/2) = ±1/6` is **not** exact: `1/6` is rational but
    /// non terminating in every decimal format, so the result is the
    /// correctly rounded neighbour and `INEXACT` is raised in every
    /// rounding direction.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction: Tier 1 by
    /// construction on the ADR-0059 ladder under `ladder::ASINPI`,
    /// with the Tier 2 model on the remainder. The exact set is
    /// decided from the input before any approximation runs, and
    /// ADR-0061's no ties theorem proves this family has no nearest
    /// mode tie at any format. Unlike `sin` and `cos`, no argument
    /// reduction caveat applies anywhere in the pi scaled family: its
    /// reduction is exact decimal arithmetic on the operand's own
    /// digits, so the dominant error item of the radian kernels does
    /// not exist here.
    #[must_use]
    #[doc(alias = "asinPi")]
    #[doc(alias = "asinpi")]
    pub fn asin_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::asin_pi_kernel::<Decimal128>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `acosPi(self)`: the arccosine in
    /// revolutions, `acos(self)/π`, rounded by `rm`. Domain
    /// `[-1, +1]`; outside is NaN with `INVALID`. Range `[0, 1]`.
    ///
    /// ## Special values (§9.2.1)
    ///
    /// * `acosPi(±0) = 1/2`, exact for both zero signs.
    /// * `acosPi(+1) = +0` and `acosPi(-1) = 1`, both exact.
    /// * `|x| > 1` and `±∞` give a quiet NaN with `INVALID`.
    /// * A signaling NaN gives a quiet NaN with `INVALID`; a quiet
    ///   NaN propagates.
    ///
    /// ## Exact values
    ///
    /// The exact set is `{±0 → 1/2, +1 → +0, -1 → 1}`.
    /// `acosPi(1/2) = 1/3` and `acosPi(-1/2) = 2/3` are rational but
    /// non terminating, so both round and raise `INEXACT` in every
    /// direction.
    ///
    /// A tiny operand is handled by an ADR-0051 residual channel
    /// rather than by the ladder: the value hugs `1/2` from below for
    /// `x > 0` and from above for `x < 0`, at a distance no finite
    /// working precision separates, and the side theorem decides the
    /// directed modes exactly.
    ///
    /// ## Accuracy
    ///
    /// As `asin_pi`, under `ladder::ACOSPI`.
    #[must_use]
    #[doc(alias = "acosPi")]
    #[doc(alias = "acospi")]
    pub fn acos_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::acos_pi_kernel::<Decimal128>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atanPi(self)`: the arctangent in
    /// revolutions, `atan(self)/π`, rounded by `rm`. Range
    /// `[-1/2, +1/2]`, open on the finite operands.
    ///
    /// ## Special values (§9.2.1)
    ///
    /// * `atanPi(±0) = ±0`, exact.
    /// * `atanPi(±∞) = ±1/2`, exact, no exception raised. The radian
    ///   `atan(±∞) = ±π/2` had to round an irrational and raise
    ///   `INEXACT`.
    /// * A signaling NaN gives a quiet NaN with `INVALID`; a quiet
    ///   NaN propagates.
    ///
    /// ## Exact values
    ///
    /// `atanPi(±1) = ±1/4` exactly: the quarter turn family the
    /// decimal formats keep, where `asinPi` and `acosPi` lost theirs
    /// to the non terminating `1/6` and `1/3`. With the zeros and the
    /// infinities that is the complete exact set.
    ///
    /// A large operand is handled by an ADR-0051 residual channel:
    /// the value hugs `±1/2` from inside, and `|atanPi(x)| < 1/2` for
    /// every finite `x` decides the directed modes.
    ///
    /// ## Accuracy
    ///
    /// As `asin_pi`, under `ladder::ATANPI`.
    #[must_use]
    #[doc(alias = "atanPi")]
    #[doc(alias = "atanpi")]
    pub fn atan_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::atan_pi_kernel::<Decimal128>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atan2Pi(self, x)`: the two argument
    /// arctangent in revolutions, `atan2(self, x)/π`, rounded by
    /// `rm`. Range `(-1, +1]`, quadrant per §9.2.1.
    ///
    /// ## Special values (§9.2.1), and the flag difference
    ///
    /// Every row of this table is an exact multiple of a quarter
    /// turn, delivered with clean flags, where `atan2`'s
    /// corresponding row was a rounded `π` family irrational carrying
    /// `INEXACT`:
    ///
    /// | operands | `atan2` | `atan2Pi` |
    /// |---|---|---|
    /// | `(±∞, +∞)` | `±π/4`, inexact | `±1/4`, exact |
    /// | `(±∞, -∞)` | `±3π/4`, inexact | `±3/4`, exact |
    /// | `(±∞, finite)` | `±π/2`, inexact | `±1/2`, exact |
    /// | `(±y, -∞)`, `(±0, x < 0)`, `(±0, -0)` | `±π`, inexact | `±1`, exact |
    /// | `(±y, +∞)`, `(±0, x > 0)`, `(±0, +0)` | `±0`, exact | `±0`, exact |
    /// | `(y ≠ 0, ±0)` | `±π/2`, inexact | `±1/2`, exact |
    ///
    /// A signaling NaN in either operand gives a quiet NaN with
    /// `INVALID`; a quiet NaN propagates, in the fixed operand order
    /// `[self, x]`.
    ///
    /// ## Exact values
    ///
    /// Beyond the table, the finite diagonals `|self| = |x|` are
    /// exact: `±1/4` for `x > 0` and `±3/4` for `x < 0`, signed by
    /// the ordinate, decided cohort insensitively from the operands.
    /// Every other finite pair has an irrational value.
    ///
    /// ## Accuracy
    ///
    /// As `asin_pi`, under `ladder::ATAN2PI`, with ADR-0051 residual
    /// channels at `±1/2` for an extreme ratio and at `±1` for a
    /// vanishing ratio against a negative abscissa.
    #[must_use]
    #[doc(alias = "atan2Pi")]
    #[doc(alias = "atan2pi")]
    pub fn atan2_pi(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig_pi::atan2_pi_kernel::<Decimal128>(self, x, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;

    const NE: RoundingMode = RoundingMode::NearestEven;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, NE).unwrap().0
    }

    /// The quarter turn table, exact and flagless, at the four
    /// entries a caller meets first.
    #[test]
    fn quarter_turns_are_exact() {
        let (r, st) = parse("1").asin_pi(NE);
        assert_eq!(alloc::format!("{r}"), "0.5");
        assert_eq!(st, Status::OK);
        let (r, st) = parse("-1").acos_pi(NE);
        assert_eq!(alloc::format!("{r}"), "1");
        assert_eq!(st, Status::OK);
        let (r, st) = parse("1").atan_pi(NE);
        assert_eq!(alloc::format!("{r}"), "0.25");
        assert_eq!(st, Status::OK);
        let (r, st) = Decimal128::INFINITY.atan_pi(NE);
        assert_eq!(alloc::format!("{r}"), "0.5");
        assert_eq!(st, Status::OK);
    }

    /// The non terminating rows: representable input, rational value,
    /// no exact decimal, so the correctly rounded `1/6` with
    /// `INEXACT`.
    #[test]
    fn one_sixth_is_inexact() {
        let (r, st) = parse("0.5").asin_pi(NE);
        assert_eq!(
            alloc::format!("{r}"),
            "0.1666666666666666666666666666666667"
        );
        assert!(st.inexact());
    }

    /// `atan2Pi`'s axis rows are exact where `atan2`'s were not: the
    /// behavioural difference the scaling introduces, asserted
    /// against the radian kernel itself.
    #[test]
    #[cfg(feature = "trig")]
    fn atan2_pi_axes_are_exact_where_atan2_was_inexact() {
        let (r, st) = parse("0").atan2_pi(parse("-1"), NE);
        assert_eq!(alloc::format!("{r}"), "1");
        assert_eq!(st, Status::OK);
        let (_, st_radian) = parse("0").atan2(parse("-1"), NE);
        assert!(st_radian.inexact());
    }
}
