//! Delegating shim for the IEEE 754-2019 §9.2 pi scaled family in
//! `ferrodec-transcend` (ADR-0061 Track D D4). The public `Decimal32`
//! wrappers and their behaviour tests stay here as the
//! byte-identical regression gate, the shape `hypot` carries.
//!
//! One function per block, in the order the standard lists them:
//! the forward trio (`sin_pi`, `cos_pi`, `tan_pi`), then the four
//! inverse wrappers (`asin_pi` through `atan2_pi`).
//!
//! These operations measure angles in **revolutions**, not radians:
//! `sin_pi(0.5) = 1` exactly, `atan2_pi(1, 0) = 1/4` turn. The
//! scaling is what makes the special values exact, so a caller
//! working in revolutions (the family's dominant real use) gets a
//! table of clean quarter and half turns where the radian spelling
//! could only offer rounded multiples of `π`.

use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `sinPi(self)`: the sine of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// `sin_pi(0.5)` is exactly `1` and `sin_pi(2)` is exactly `+0`, at
    /// every magnitude the format reaches: the reduction is exact
    /// decimal arithmetic on the operand's own digits.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction. Tier 1 by
    /// construction plus the Tier 2 model (ADR-0059), **with no
    /// reduction caveat**: the reduction item is provably zero, so the
    /// budget prices only the `πδ` multiply and the Taylor series, and
    /// the escalation rate lands near `10^-12` per call where the radian
    /// kernel's is three percent.
    ///
    /// The operation has **no nearest-mode tie at any format**: by
    /// Niven's theorem the rational values are `{0, ±1/2, ±1}`, the
    /// `±1/2` rows need the abscissas `k ± 1/6` that no decimal format
    /// represents, and the survivors are grid points rather than
    /// midpoints. `INEXACT` past the exact table is therefore correct in
    /// every mode. ADR-0060's adjudicator route is closed for this
    /// family and deliberately not wired. The derivations live on
    /// `ferrodec_transcend::sincospi` and `ladder::SINPI`.
    ///
    /// ## Exact results (§7.5, no `INEXACT`)
    ///
    /// | operand | result |
    /// |---|---|
    /// | integer `n` | `±0`, carrying the **operand's** sign |
    /// | half integer `n + 1/2` | `+1` for even `n`, `−1` for odd `n` |
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `sinPi(±0)` is `±0`.
    /// * `sinPi(±∞)` is a quiet NaN with `INVALID`.
    /// * NaN propagates; a signaling NaN raises `INVALID` and returns
    ///   the quieted payload.
    #[must_use]
    #[doc(alias = "sinPi")]
    #[doc(alias = "sinpi")]
    pub fn sin_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincospi::sin_pi_kernel::<Decimal32>(self, rm)
    }
}

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `cosPi(self)`: the cosine of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// `cos_pi(0.5)` is exactly `+0` and `cos_pi(1)` is exactly `−1`.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction, on the same Tier 1
    /// by construction plus Tier 2 model footing as
    /// [`sin_pi`](Self::sin_pi), with the same absent reduction caveat
    /// and the same no-ties fact (the `±1/2` rows would need `k ± 1/3`).
    ///
    /// One asymptotic family needs more than the ladder: `cos(πδ)` hugs
    /// `±1` quadratically, and near the integer zero the hug is
    /// unbounded, because there `δ` is the operand itself and reaches
    /// `10^-101`. That neighborhood is delivered through an ADR-0051
    /// residual channel whose side is the theorem `cos(πδ) < 1` strictly
    /// off the exact set.
    ///
    /// ## Exact results (§7.5, no `INEXACT`)
    ///
    /// | operand | result |
    /// |---|---|
    /// | integer `n` | `+1` for even `n`, `−1` for odd `n` |
    /// | half integer `n + 1/2` | `+0`, **always** |
    ///
    /// The half-integer sign is §9.2.1's rule that keeps the function
    /// even: `cos_pi(-0.5)` is `+0`, not `−0`.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `cosPi(±0)` is `1`.
    /// * `cosPi(±∞)` is a quiet NaN with `INVALID`.
    /// * NaN propagates; a signaling NaN raises `INVALID` and returns
    ///   the quieted payload.
    /// * The operand's sign never reaches the result: `cosPi` is even.
    #[must_use]
    #[doc(alias = "cosPi")]
    #[doc(alias = "cospi")]
    pub fn cos_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincospi::cos_pi_kernel::<Decimal32>(self, rm)
    }
}

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `tanPi(self)`: the tangent of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// `tan_pi(0.25)` is exactly `1` and `tan_pi(0.5)` is exactly `+∞`.
    /// The quarter integers are the family the decimal formats keep
    /// where `1/6` and `1/3` deny the sine and cosine their `±1/2` rows.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction, Tier 1 by
    /// construction plus the Tier 2 model, no reduction caveat, no
    /// nearest-mode tie at any format.
    ///
    /// **The poles cannot overflow, and the kernel carries no overflow
    /// gate by design.** Representing `n + 1/2 + δ` forces `δ` to be a
    /// nonzero multiple of the operand's own quantum, and a pole
    /// neighborhood has magnitude at least `1/2`, so `|δ| ≥ 10^-7` and
    /// the pole value caps at `10^7/π ≈ 3.2·10^6` — 90 decades
    /// inside this format's ceiling. The absence of the gate is the
    /// proof's consequence, recorded in ADR-0061.
    ///
    /// ## Exact results (§7.5, no `INEXACT`)
    ///
    /// | operand | result |
    /// |---|---|
    /// | integer `n` | zero, signed `(−1)^n · sign(self)` |
    /// | quarter integer `n + 1/4` | `+1` |
    /// | quarter integer `n + 3/4` | `−1` |
    /// | half integer `n + 1/2` | `±∞` with `DIV_BY_ZERO` |
    ///
    /// The quarter-integer rows have period 1 and reflect through the
    /// odd function. The pole row is `+∞` for even `n` and `−∞` for odd
    /// `n`, odd-reflected for a negative operand.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `tanPi(±0)` is `±0`.
    /// * `tanPi(±∞)` is a quiet NaN with `INVALID`.
    /// * NaN propagates; a signaling NaN raises `INVALID` and returns
    ///   the quieted payload.
    #[must_use]
    #[doc(alias = "tanPi")]
    #[doc(alias = "tanpi")]
    pub fn tan_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincospi::tan_pi_kernel::<Decimal32>(self, rm)
    }
}

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
