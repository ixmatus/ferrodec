//! Thin `Decimal128` adapter onto the shared, generic faithful-rounding
//! harness in `ferrodec_test_support::transcend_oracle`. Lives at
//! `tests/common/mod.rs` so each `tests/property_*.rs` file can pull it
//! in via `mod common;` without Cargo treating it as a separate
//! integration-test binary.
//!
//! A `#[allow(dead_code)]` blanket sits at the top because the helpers
//! here are split across consumers: a file that only checks `exp` does
//! not touch the two-argument or identity helpers, and vice versa.
//! Without the blanket, files that import one helper would warn for the
//! others.
//!
//! ## The faithful-rounding contract (ADR-0021, IEEE 754-2019 §9.2)
//!
//! IEEE 754-2019 §9.2 *recommends* but does not *require* correctly
//! rounded results for the transcendental ("recommended") operations.
//! `ferrodec` therefore declares the weaker **faithful rounding**
//! contract for every transcendental: the returned value is one of the
//! two representable `Decimal128` values immediately adjacent to the
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
//! function-approximation error `~10^-70`, far below one `Decimal128`
//! ULP of `~10^-34`). Faithfulness is then decided structurally using
//! the format's own `next_up` / `next_down`: `got` is faithful iff
//! `next_up(got)` does not fall on or below the true value and
//! `next_down(got)` does not fall on or above it (i.e. no representable
//! value is strictly between `got` and the true result). The three
//! directed `ferrodec` rounding modes additionally pin *which* of the
//! two adjacent values the spec mandates for that direction.
//!
//! The faithful machinery itself (`to_bf`, `cmp_approx`,
//! `within_faithful_ulp`, `faithful_side_ok`, `assert_faithful`, the
//! `P_CMP = 256` precision, the `1e-40` / `1e-300` dead-band) now lives
//! once in `ferrodec_test_support::transcend_oracle`, generic over the
//! `FaithfulFormat` trait. The [`D128`] newtype below is the
//! `Decimal128` implementation of that trait (a newtype because the
//! orphan rule forbids implementing a foreign trait for the foreign
//! `Decimal128` directly from this test crate). Same-named, concrete
//! `Decimal128`-signature wrappers (`assert_faithful`, `parse`, `MODES`,
//! `within_faithful_ulp`, `faithful_side_ok`) sit on top so every
//! `tests/property_*.rs` keeps its `use common::{...}` lines and call
//! sites byte-unchanged. `within_rel_ulps` stays here too: it is a
//! `Decimal128`-arithmetic identity cross-check, not the faithful
//! contract, so it does not belong in the generic harness.

#![allow(dead_code)]

use core::cmp::Ordering;

use astro_float::{BigFloat, Consts};
use ferrodec::{Decimal128, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::{self, FaithfulFormat, SpecRounding};

/// Newtype carrier so the foreign [`FaithfulFormat`] trait can be
/// implemented for the foreign `Decimal128` from this test crate
/// without tripping the orphan rule. Every method forwards to the
/// format's own IEEE behaviour, so the bracket logic in
/// `transcend_oracle` reasons over the exact same values it did when
/// the machinery lived in this file.
#[derive(Clone, Copy, Debug)]
pub struct D128(pub Decimal128);

impl FaithfulFormat for D128 {
    type Rounding = RoundingMode;
    type Status = Status;

    /// The five IEEE 754-2019 rounding directions, asserted for every
    /// transcendental input so the directional faithful-rounding
    /// contract is exercised on each mode rather than only
    /// round-to-nearest-even.
    const MODES: &'static [RoundingMode] = &[
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardZero,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ];

    fn parse_nearest_even(s: &str) -> Self {
        D128(
            Decimal128::parse_str(s, RoundingMode::NearestEven)
                .unwrap()
                .0,
        )
    }

    fn sci(self) -> String {
        let d = self.0;
        format!("{d:e}")
    }

    fn next_up(self) -> Self {
        D128(self.0.next_up().0)
    }

    fn next_down(self) -> Self {
        D128(self.0.next_down().0)
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

    /// Map `ferrodec`'s concrete `RoundingMode` onto the harness's
    /// format-independent [`SpecRounding`] classification. The two
    /// to-nearest modes collapse to `Nearest` (faithful, not correctly
    /// rounded, per ADR-0021); only the three directed modes pin a
    /// side.
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
pub const MODES: &[RoundingMode] = <D128 as FaithfulFormat>::MODES;

/// Parse a decimal literal at round-half-even, panicking on invalid
/// input. The shape every property test wants for hand-curated
/// reference values. Concrete `Decimal128` signature so consumers stay
/// byte-unchanged.
pub fn parse(s: &str) -> Decimal128 {
    transcend_oracle::parse::<D128>(s).0
}

/// `true` iff `got` is faithfully rounded with respect to the true
/// value `oracle`: no representable `Decimal128` lies strictly between
/// `got` and the true result. Concrete-`Decimal128` wrapper around the
/// generic harness; semantics unchanged.
pub fn within_faithful_ulp(got: Decimal128, oracle: &BigFloat, cc: &mut Consts) -> bool {
    transcend_oracle::within_faithful_ulp(D128(got), oracle, cc)
}

/// `true` iff `got` is on the spec-mandated side of the true value for
/// rounding mode `rm`. Concrete-`Decimal128` wrapper around the generic
/// harness; semantics unchanged.
pub fn faithful_side_ok(
    got: Decimal128,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
) -> bool {
    transcend_oracle::faithful_side_ok(D128(got), oracle, cc, rm)
}

/// Assert the full faithful-rounding contract for one transcendental
/// evaluation. Concrete-`Decimal128` wrapper around the generic
/// harness; the panic conditions (non-finite, not faithfully rounded,
/// in-domain `INVALID`, inexact-without-`INEXACT`) are identical.
pub fn assert_faithful(
    got: Decimal128,
    status: Status,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
    ctx: &str,
) {
    transcend_oracle::assert_faithful(D128(got), status, oracle, cc, rm, ctx);
}

/// Relative-tolerance closeness for **algebraic identity cross-checks
/// only** (e.g. `sin²x + cos²x ≈ 1`), *not* the IEEE faithfulness
/// contract. An identity composed from several rounded operations
/// accumulates more than one ULP of error by construction, so pinning
/// it to the faithful contract would be wrong; a loose relative bound
/// is the right sanity check here and is deliberately kept under a name
/// that cannot be mistaken for the spec claim. `true` if `got` is
/// within `ulps · 10^-33` relative of `want`, or within `ulps · 10^-30`
/// absolute when `want` is zero.
///
/// Kept in this `Decimal128` adapter rather than the generic harness:
/// it does decimal arithmetic (`sub` / `div` / `partial_cmp`) on the
/// format itself, so it is an identity cross-check helper, not part of
/// the faithful contract the shared module owns.
pub fn within_rel_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-30"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(cmp, Some(Ordering::Less | Ordering::Equal));
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-33"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(cmp, Some(Ordering::Less | Ordering::Equal))
}
