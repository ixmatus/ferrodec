//! Delegating shim for `compound` (IEEE 754-2019 §9.2; ADR-0059
//! Track D group D3). The kernel lives in `ferrodec-transcend`
//! (`compound::compound_kernel`), generic over the `DecimalFormat`
//! seam, so all three decimal siblings share one implementation.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `compound(self, n)`: `(1 + self)^n` for an
    /// integral `n`, evaluated so that a small `self` keeps its full
    /// significance instead of losing it to the cancellation
    /// `1 ⊕ self` would suffer at this format's precision.
    ///
    /// That is the whole point of the operation, and why it exists
    /// beside `pow`: computing `(1 + rate).pow(periods)` by adding one
    /// first throws away exactly the digits a small `rate` carries.
    /// This kernel builds the base at working precision instead.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded. Like the rest of the ADR-0059 Track D
    /// surface, `compound` runs on the ADR-0059 escalation ladder from
    /// its first release: rung 1 evaluates at 50 digits and delivers
    /// only when the operation's error budget clears every rounding
    /// boundary of the format, otherwise the identical body re-runs at
    /// rung 2's 110 digits, and under the `unbounded-ladder` feature at
    /// a dynamic rung that widens until the rounding is decided. The
    /// budget is itemized in `ferrodec-transcend`'s `ladder.rs`
    /// (`COMPOUND`), and the two premises it rests on are the ADR-0059
    /// Tier 1 conditions: the budget is sound and the exactness
    /// classification is complete.
    ///
    /// `compound` differs from every other operation in this family in
    /// one respect worth stating plainly: its value is **always
    /// rational**, because `1 + self` is an exact rational for every
    /// in-domain input and an integer power of a rational is rational.
    /// So exact results and nearest-mode ties are not rare corners here
    /// but the operation's ordinary business, and they are all decided
    /// from the inputs alone before any approximation runs
    /// (`(1.05)^3 = 1.157625` exactly, `(1 + 4)^49 = 5^49` on a
    /// nearest-mode midpoint). The derivation, the tie families, and
    /// the per-bail completeness proofs live on
    /// `ferrodec_transcend::exact::compound_exact_input`.
    ///
    /// Two families sit exactly on a format grid point, where no
    /// working precision can decide the directed modes, and both are
    /// answered input side: `1 + self = 10^k` (the nines patterns
    /// `9`, `99`, … and `−0.9`, `−0.99`, …), whose value `10^(k·n)`
    /// stays on the grid at any magnitude and carries the §7.4
    /// disposition past the exponent range; and the band where
    /// `|n · ln(1 + self)|` is tiny and the value hugs 1, delivered
    /// through the ADR-0051 residual channel on the strict side
    /// theorem that `(1 + self)^n` exceeds 1 exactly when `self` and
    /// `n` share a sign.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `compound(self, 0) = 1` for every `self ≥ −1`, **and for a
    ///   quiet NaN** — one of the few places a NaN does not propagate.
    /// * `compound(−1, n) = +∞` with `DIV_BY_ZERO` for `n < 0`, and
    ///   `+0` for `n > 0`.
    /// * `compound(±0, n) = 1`.
    /// * `compound(+∞, n) = +∞` for `n > 0`, `+0` for `n < 0`.
    /// * `compound(self, n)` is NaN with `INVALID` for `self < −1`
    ///   (`−∞` included), at every `n` — including `n = 0`, since the
    ///   rule above is conditioned on `self ≥ −1`.
    /// * A quiet NaN propagates for `n ≠ 0`; a signaling NaN raises
    ///   `INVALID` and returns the quieted payload at every `n`,
    ///   `n = 0` included.
    /// * The result is never negative: `1 + self > 0` on the domain.
    ///
    /// ## Preferred exponent (IEEE 754-2019 §9.2.2)
    ///
    /// `Q(compound(self, n))` is `floor(n × min(0, Q(self)))`, honoured
    /// on exact deliveries, subject to §6.3's rule that the quantum
    /// moves only as far as the coefficient allows.
    #[must_use]
    pub fn compound(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::compound::compound_kernel::<Decimal128>(self, n, rm)
    }
}
