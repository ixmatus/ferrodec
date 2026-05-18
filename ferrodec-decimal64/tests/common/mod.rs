//! Thin `Decimal64` adapter onto the shared, generic faithful-rounding
//! harness in `ferrodec_test_support::transcend_oracle`. Lives at
//! `tests/common/mod.rs` so each `tests/property_*.rs` file can pull it
//! in via `mod common;` without Cargo treating it as a separate
//! integration-test binary.
//!
//! A `#[allow(dead_code)]` blanket sits at the top because the helpers
//! here are split across consumers: a file that only checks `exp` does
//! not touch the identity helper, and vice versa. Without the blanket,
//! files that import one helper would warn for the others.
//!
//! ## The faithful-rounding contract (ADR-0021, IEEE 754-2019 §9.2)
//!
//! IEEE 754-2019 §9.2 *recommends* but does not *require* correctly
//! rounded results for the transcendental ("recommended") operations.
//! `ferrodec-decimal64` therefore declares the weaker **faithful
//! rounding** contract for `exp` / `ln`: the returned value is one of
//! the two representable `Decimal64` values immediately adjacent to the
//! exact mathematical result (equivalently: no representable value lies
//! strictly between the result and the true value).
//!
//! The harness asserts that contract *exactly*. It does **not** use a
//! symmetric `± k ULP` tolerance envelope: an envelope silently admits
//! a systematically half-ULP-biased kernel (every result rounded the
//! wrong way by just under a ULP still "passes"), which is precisely
//! the failure the faithfulness remediation exists to rule out.
//!
//! Instead the true value is computed by astro-float at a working
//! precision the caller fixes at `≥ 256` bits (`≈ 77` decimal digits,
//! function-approximation error `~10^-70`, far below one `Decimal64`
//! ULP of `~10^-16`). Faithfulness is then decided structurally using
//! the format's own `next_up` / `next_down`: `got` is faithful iff
//! `next_up(got)` does not fall on or below the true value and
//! `next_down(got)` does not fall on or above it (i.e. no representable
//! value is strictly between `got` and the true result). The three
//! directed `ferrodec-decimal64` rounding modes additionally pin
//! *which* of the two adjacent values the spec mandates for that
//! direction.
//!
//! The faithful machinery itself (`to_bf`, `cmp_approx`,
//! `within_faithful_ulp`, `faithful_side_ok`, `assert_faithful`, the
//! `P_CMP = 256` precision, the dead-band) lives once in
//! `ferrodec_test_support::transcend_oracle`, generic over the
//! `FaithfulFormat` trait. The [`D64`] newtype below is the
//! `Decimal64` implementation of that trait (a newtype because the
//! orphan rule forbids implementing a foreign trait for the foreign
//! `Decimal64` directly from this test crate). This is `Decimal64`
//! TIER-1: the oracle is computed directly in astro-float, the same
//! way `Decimal128` does it, because `decimal64` carries an
//! astro-float dev-dependency. Same-named, concrete
//! `Decimal64`-signature wrappers (`assert_faithful`, `parse`, `MODES`)
//! sit on top so every `tests/property_*.rs` keeps its
//! `use common::{...}` lines and call sites byte-unchanged.

#![allow(dead_code)]

use astro_float::{BigFloat, Consts};
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::{self, FaithfulFormat, SpecRounding};

/// Newtype carrier so the foreign [`FaithfulFormat`] trait can be
/// implemented for the foreign `Decimal64` from this test crate
/// without tripping the orphan rule. Every method forwards to the
/// format's own IEEE behaviour, so the bracket logic in
/// `transcend_oracle` reasons over the exact same values it did for
/// `Decimal128`.
#[derive(Clone, Copy, Debug)]
pub struct D64(pub Decimal64);

impl FaithfulFormat for D64 {
    type Rounding = RoundingMode;
    type Status = Status;

    /// The five IEEE 754-2019 rounding directions, asserted for every
    /// transcendental input so the directional faithful-rounding
    /// contract is exercised on each mode rather than only
    /// round-to-nearest-even. Same order as the `Decimal128` adapter.
    const MODES: &'static [RoundingMode] = &[
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardZero,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ];

    fn parse_nearest_even(s: &str) -> Self {
        D64(Decimal64::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0)
    }

    fn sci(self) -> String {
        let d = self.0;
        format!("{d:e}")
    }

    fn next_up(self) -> Self {
        D64(self.0.next_up().0)
    }

    fn next_down(self) -> Self {
        D64(self.0.next_down().0)
    }

    fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    fn raised_invalid(st: Status) -> bool {
        st.invalid()
    }

    fn raised_inexact(st: Status) -> bool {
        st.inexact()
    }

    /// Map `ferrodec-decimal64`'s concrete `RoundingMode` onto the
    /// harness's format-independent [`SpecRounding`] classification.
    /// The two to-nearest modes collapse to `Nearest` (faithful, not
    /// correctly rounded, per ADR-0021); only the three directed modes
    /// pin a side. Identical grouping to the `Decimal128` adapter.
    fn spec_rounding(rm: RoundingMode) -> SpecRounding {
        match rm {
            RoundingMode::NearestEven | RoundingMode::NearestAway => SpecRounding::Nearest,
            RoundingMode::TowardPositive => SpecRounding::TowardPositive,
            RoundingMode::TowardNegative => SpecRounding::TowardNegative,
            RoundingMode::TowardZero => SpecRounding::TowardZero,
        }
    }
}

/// The five IEEE 754-2019 rounding directions, asserted for every
/// transcendental input so the directional faithful-rounding contract
/// is exercised on each mode rather than only round-to-nearest-even.
/// Re-exported under the historical name and concrete element type so
/// every `tests/property_*.rs` keeps its `use common::MODES` line and
/// `for &rm in MODES` loop unchanged.
pub const MODES: &[RoundingMode] = <D64 as FaithfulFormat>::MODES;

/// Parse a decimal literal at round-half-even, panicking on invalid
/// input. The shape every property test wants for hand-curated
/// reference values. Concrete `Decimal64` signature so consumers stay
/// byte-unchanged.
pub fn parse(s: &str) -> Decimal64 {
    transcend_oracle::parse::<D64>(s).0
}

/// `true` iff `got` is faithfully rounded with respect to the true
/// value `oracle`: no representable `Decimal64` lies strictly between
/// `got` and the true result. Concrete-`Decimal64` wrapper around the
/// generic harness; semantics unchanged.
pub fn within_faithful_ulp(got: Decimal64, oracle: &BigFloat, cc: &mut Consts) -> bool {
    transcend_oracle::within_faithful_ulp(D64(got), oracle, cc)
}

/// `true` iff `got` is on the spec-mandated side of the true value for
/// rounding mode `rm`. Concrete-`Decimal64` wrapper around the generic
/// harness; semantics unchanged.
pub fn faithful_side_ok(
    got: Decimal64,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
) -> bool {
    transcend_oracle::faithful_side_ok(D64(got), oracle, cc, rm)
}

/// Assert the full faithful-rounding contract for one transcendental
/// evaluation. Concrete-`Decimal64` wrapper around the generic
/// harness; the panic conditions (non-finite, not faithfully rounded,
/// in-domain `INVALID`, inexact-without-`INEXACT`) are identical.
pub fn assert_faithful(
    got: Decimal64,
    status: Status,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
    ctx: &str,
) {
    transcend_oracle::assert_faithful(D64(got), status, oracle, cc, rm, ctx);
}

/// `true` iff `got` lies within `n_ulps` representable steps of `want`
/// (`|got − want| ≤ n_ulps · ulp(want)`). The structural band for
/// **metamorphic identity cross-checks** (ADR-0025): the caller derives
/// `n_ulps` per identity from the analytic condition number. Concrete-
/// `Decimal64` wrapper around the generic harness; semantics unchanged.
pub fn within_n_ulp_band(got: Decimal64, want: Decimal64, n_ulps: u32, cc: &mut Consts) -> bool {
    transcend_oracle::within_n_ulp_band(D64(got), D64(want), n_ulps, cc)
}
