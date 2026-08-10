//! IEEE 754-2019 §9.2 forward pi-scaled trio for [`Decimal64`]:
//! `sinPi`, `cosPi`, `tanPi` (ADR-0061 Track D D4).
//!
//! Pure delegation onto the shared `ferrodec-transcend` kernel, which
//! resolves every §9.2.1 special value internally, classifies the exact
//! set from the operand's own digits, and runs the ADR-0059 escalation
//! ladder from this group's first release. The operand counts
//! revolutions, so the reduction is `self mod 2` in exact decimal
//! arithmetic: no `π` constant, no Payne and Hanek window, and no
//! reduction term in the error budget. The family ships under its own
//! `trig-pi` feature, which does not imply `trig`.
//!
//! One function per block: the inverse four append alongside these at
//! integration.

use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
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
        ferrodec_transcend::sincospi::sin_pi_kernel::<Decimal64>(self, rm)
    }
}

impl Decimal64 {
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
    /// `10^-398`. That neighborhood is delivered through an ADR-0051
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
        ferrodec_transcend::sincospi::cos_pi_kernel::<Decimal64>(self, rm)
    }
}

impl Decimal64 {
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
    /// neighborhood has magnitude at least `1/2`, so `|δ| ≥ 10^-16` and
    /// the pole value caps at `10^16/π ≈ 3.2·10^15` — 369 decades
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
        ferrodec_transcend::sincospi::tan_pi_kernel::<Decimal64>(self, rm)
    }
}
