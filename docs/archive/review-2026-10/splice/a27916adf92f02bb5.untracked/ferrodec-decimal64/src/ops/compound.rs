//! Delegating shim for `compound` (IEEE 754-2019 §9.2; ADR-0059
//! Track D group D3). The kernel lives in `ferrodec-transcend`
//! (`compound::compound_kernel`), generic over the `DecimalFormat`
//! seam, so all three decimal siblings share one implementation.

use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `compound(self, n)`: `(1 + self)^n` for an
    /// integral `n`, evaluated so that a small `self` keeps its full
    /// significance instead of losing it to the cancellation
    /// `1 ⊕ self` would suffer at this format's precision. Pure
    /// delegation onto the shared kernel, which resolves every §9.2.1
    /// special value internally and runs the ADR-0059 escalation
    /// ladder from this operation's first release; the derivation of
    /// its exactness classification, its two on-grid families, its
    /// ADR-0051 anchor arm, and its error budget live on
    /// `ferrodec_transcend::compound::compound_kernel`,
    /// `ferrodec_transcend::exact::compound_exact_input`, and
    /// `ladder::COMPOUND`.
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
    ///   (`−∞` included), at every `n` including `n = 0`.
    /// * A quiet NaN propagates for `n ≠ 0`; a signaling NaN raises
    ///   `INVALID` at every `n`. The result is never negative.
    ///
    /// ## Preferred exponent (IEEE 754-2019 §9.2.2)
    ///
    /// `Q(compound(self, n))` is `floor(n × min(0, Q(self)))`, honoured
    /// on exact deliveries subject to §6.3's coefficient limit.
    #[must_use]
    pub fn compound(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::compound::compound_kernel::<Decimal64>(self, n, rm)
    }
}
