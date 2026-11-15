//! Delegating shim for IEEE 754-2019 §9.2 `powr` on [`Decimal128`]
//! (ADR-0059 Track D D3). The kernel, the §9.2.1 table, and the
//! derivation of every row from `exp(y · ln x)` live in
//! `ferrodec_transcend::powr`; the behaviour gate is
//! `tests/transcend_exact_powr.rs`.
//!
//! `powr` sits beside `pow` rather than replacing it: the two operate
//! on the same kernel and disagree only on the special value table,
//! deliberately and per the standard.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `powr(self, exponent)`: `self` raised to the
    /// power `exponent`, **defined as `exp(exponent · ln(self))`**,
    /// rounded by `rm`.
    ///
    /// `powr` is the strict exponential-logarithm power, distinct from
    /// [`Decimal128::pow`] on four families of input where taking the
    /// definition literally means refusing an answer `pow` supplies by
    /// the integer power rule:
    ///
    /// * `powr(x, y)` for **any** `x < 0` is qNaN + `INVALID`, integer
    ///   exponents included, because `ln x` is undefined. `pow(−2, 3)`
    ///   is `−8`.
    /// * `powr(±0, ±0)`, `powr(+∞, ±0)`, and `powr(+1, ±∞)` are each
    ///   qNaN + `INVALID`, the three indeterminate forms of
    ///   `y · ln x`. `pow` answers `1` for all three.
    ///
    /// The rest of the table follows the same limits: `powr(x, ±0) = 1`
    /// for finite `x > 0`; `powr(±0, y)` is `+∞` with `DIV_BY_ZERO` for
    /// finite `y < 0`, `+∞` with no exception for `y = −∞`, and `+0`
    /// for `y > 0`; `powr(+1, y) = 1` for finite `y`; NaN propagates
    /// quietly for `x ≥ 0`, and a signaling NaN raises `INVALID`.
    ///
    /// Accuracy and exactness are `pow`'s exactly, on the shared
    /// kernel: correctly rounded on the ADR-0059 escalation ladder,
    /// with exact powers and `PRECISION + 1` ties classified from the
    /// inputs before any approximation runs. The claim carries `pow`'s
    /// tier and cannot be upgraded by the ADR-0060 Liouville mechanism
    /// that makes the rest of the algebraic §9.2 group unconditional;
    /// `ferrodec_transcend::powr`'s module doc states why.
    #[must_use]
    pub fn powr(self, exponent: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::powr::powr_kernel::<Decimal128>(self, exponent, rm)
    }

    /// Kani-only entry point returning the IEEE 754-2019 §9.2.1 `powr`
    /// special-value branch only, without invoking the working
    /// precision `exp(y · ln x)` pipeline.
    ///
    /// Mirrors [`Decimal128::pow_special_only_for_kani`] so a symbolic
    /// proof of the `powr` table stays inside CBMC's budget (ADR-0016).
    /// Returns `None` for the general-path inputs (`x` finite `> 0`,
    /// `y` finite non-zero). `rm` is accepted for convention parity
    /// with the sibling shims but ignored: the table does not depend on
    /// the rounding direction.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn powr_special_only_for_kani(
        self,
        exponent: Self,
        _rm: RoundingMode,
    ) -> Option<(Self, Status)> {
        ferrodec_transcend::powr::powr_special_cases::<Decimal128>(self, exponent)
    }
}
