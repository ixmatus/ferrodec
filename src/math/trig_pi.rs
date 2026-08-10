//! Delegating shim for the IEEE 754-2019 §9.2 forward pi-scaled trio in
//! `ferrodec-transcend` (ADR-0061 Track D D4). The public
//! `Decimal128::sin_pi`, `cos_pi`, and `tan_pi` wrappers stay here as
//! the byte-identical regression gate, the shape `hypot` carries.
//!
//! One function per block: the inverse four (`asin_pi` through
//! `atan2_pi`) append alongside these at integration.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `sinPi(self)`: the sine of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// The change of unit is the point. A revolutions operand reduces by
    /// `self mod 2` on its own decimal digits, so there is no `π`
    /// constant in the reduction, no Payne and Hanek window, and no
    /// truncation term: `sin_pi(0.5)` is exactly `1`, `sin_pi(2)` is
    /// exactly `+0`, and a quarter turn is a quarter turn at every
    /// magnitude the format reaches. The family ships under its own
    /// `trig-pi` feature, which does not imply `trig`, so a
    /// revolutions-based user pays for none of the radian reduction
    /// tables.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction. Tier 1 by
    /// construction plus the Tier 2 model (ADR-0059), **with no
    /// reduction caveat**: the reduction item is provably zero, so the
    /// budget prices only the `πδ` multiply and the Taylor series. That
    /// leaves it roughly three decimal orders tighter than `sin`, and
    /// the escalation rate near `10^-12` per call where the radian
    /// kernel's is three percent.
    ///
    /// The operation has **no nearest-mode tie at any format**: by
    /// Niven's theorem the rational values of `sinPi` are `{0, ±1/2, ±1}`,
    /// the `±1/2` rows need the abscissas `k ± 1/6` that no decimal
    /// format represents, and the survivors are grid points rather than
    /// midpoints. So `INEXACT` past the exact table below is correct in
    /// every mode, and the ladder's audit is vacuous for this operation
    /// by construction. ADR-0060's exact integer adjudicator is
    /// deliberately not wired: `sin(πp/q)` is algebraic of degree
    /// growing with `φ(2q)`, which at format denominators is past any
    /// fixed-width comparison. The derivations live on
    /// `ferrodec_transcend::sincospi` and `ladder::SINPI`.
    ///
    /// ## Exact results (§7.5, no `INEXACT`)
    ///
    /// | operand | result |
    /// |---|---|
    /// | integer `n` | `±0`, carrying the **operand's** sign |
    /// | half integer `n + 1/2` | `+1` for even `n`, `−1` for odd `n` |
    ///
    /// The zero rows follow §9.2.1's odd-function rule rather than the
    /// one-sided limit, so `sin_pi(-3)` is `−0`. Both tables are cohort
    /// insensitive: `2.50` and `2.5` are one operand.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `sinPi(±0)` is `±0`.
    /// * `sinPi(±∞)` is a quiet NaN with `INVALID`: the function is
    ///   periodic and has no limit at infinity.
    /// * NaN propagates; a signaling NaN raises `INVALID` and returns
    ///   the quieted payload.
    #[must_use]
    #[doc(alias = "sinPi")]
    #[doc(alias = "sinpi")]
    pub fn sin_pi(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincospi::sin_pi_kernel::<Decimal128>(self, rm)
    }
}

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `cosPi(self)`: the cosine of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// `cos_pi(0.5)` is exactly `+0` and `cos_pi(1)` is exactly `−1`,
    /// neither of which the radian kernel can deliver: the reduction is
    /// exact decimal arithmetic on the operand's own digits, so the
    /// quarter turns land where the geometry says they do.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded at every rounding direction, on the same Tier 1
    /// by construction plus Tier 2 model footing as
    /// [`sin_pi`](Self::sin_pi) and with the same absent reduction
    /// caveat and the same no-ties fact (the `±1/2` rows would need
    /// `k ± 1/3`, which no decimal format represents).
    ///
    /// One asymptotic family needs more than the ladder: `cos(πδ)` hugs
    /// `±1` quadratically, and near the integer zero the hug is
    /// unbounded, because there `δ` is the operand itself and reaches
    /// `10^-6176`. That neighborhood is delivered through an ADR-0051
    /// residual channel whose side is the theorem `cos(πδ) < 1` strictly
    /// off the exact set, with the per-format margin table derived on
    /// `ferrodec_transcend::sincospi`. Everything outside it the ladder
    /// decides unaided.
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
        ferrodec_transcend::sincospi::cos_pi_kernel::<Decimal128>(self, rm)
    }
}

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `tanPi(self)`: the tangent of `π · self`, with
    /// the operand measured in **revolutions** rather than radians.
    ///
    /// `tan_pi(0.25)` is exactly `1` and `tan_pi(0.5)` is exactly `+∞`.
    /// The quarter integers are the family the decimal formats keep
    /// where `1/6` and `1/3` deny the sine and cosine their `±1/2` rows:
    /// `0.25` and `0.75` and their translates are representable, so
    /// `tanPi` alone gains an exact `±1` table.
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
    /// neighborhood has magnitude at least `1/2`, so `|δ| ≥ 10^-34` and
    /// the pole value caps at `10^34/π ≈ 3.2·10^33` — 6111 decades
    /// inside this format's ceiling. The absence of the gate is the
    /// proof's consequence, recorded in ADR-0061 rather than left to a
    /// reader's inference.
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
    /// odd function, so `tan_pi(1.25)` is `+1` and `tan_pi(-0.25)` is
    /// `−1`. The pole row is `+∞` for even `n` and `−∞` for odd `n`,
    /// odd-reflected for a negative operand.
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
        ferrodec_transcend::sincospi::tan_pi_kernel::<Decimal128>(self, rm)
    }
}
