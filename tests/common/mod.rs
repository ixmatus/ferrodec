//! Shared helpers for the transcendental property-test files. Lives at
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
//! This module asserts that contract *exactly*. It does **not** use a
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

#![allow(dead_code)]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

/// The five IEEE 754-2019 rounding directions, asserted for every
/// transcendental input so the directional faithful-rounding contract
/// is exercised on each mode rather than only round-to-nearest-even.
pub const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

/// Precision for the internal `Decimal128 → BigFloat` conversions used
/// to compare `got` and its neighbours against the oracle. A
/// `Decimal128` carries ≤ 34 significant digits, so 256 bits (≈ 77
/// digits) holds every converted value exactly.
const P_CMP: usize = 256;

/// Parse a decimal literal at round-half-even, panicking on invalid
/// input. The shape every property test wants for hand-curated
/// reference values.
pub fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

/// Exact conversion of a finite `Decimal128` to a `BigFloat`. Uses the
/// forced-scientific `{:e}` form (never `Display`), which preserves the
/// exact significand digits and exponent.
fn to_bf(d: Decimal128, cc: &mut Consts) -> BigFloat {
    BigFloat::parse(&format!("{d:e}"), Radix::Dec, P_CMP, AfRm::None, cc)
}

/// Sign of `a - b` with a relative dead-band that swallows astro-float's
/// sub-ULP function-approximation error: values within `|b|·10^-40`
/// (or `10^-300` absolute when `b ≈ 0`) are reported `Equal`. The
/// dead-band is ~30 orders of magnitude below one `Decimal128` ULP, so
/// it never merges two distinct representable values; it only absorbs
/// the oracle's own noise so an exactly-correct result is not spuriously
/// judged "on the wrong side" of a true value it actually equals.
fn cmp_approx(a: &BigFloat, b: &BigFloat, cc: &mut Consts) -> Ordering {
    let diff = a.sub(b, P_CMP, AfRm::None);
    let e40 = BigFloat::parse("1e-40", Radix::Dec, P_CMP, AfRm::None, cc);
    let floor = BigFloat::parse("1e-300", Radix::Dec, P_CMP, AfRm::None, cc);
    let mut tol = b.mul(&e40, P_CMP, AfRm::None);
    if tol.abs_cmp(&floor).expect("finite tol") < 0 {
        tol = floor;
    }
    if diff.abs_cmp(&tol).expect("finite diff") <= 0 {
        Ordering::Equal
    } else if diff.is_negative() {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// `true` iff `got` is faithfully rounded with respect to the true
/// value `oracle`: no representable `Decimal128` lies strictly between
/// `got` and the true result. Equivalent to "`got` is one of the two
/// representable values adjacent to the true value".
pub fn within_faithful_ulp(got: Decimal128, oracle: &BigFloat, cc: &mut Consts) -> bool {
    let up = to_bf(got.next_up().0, cc);
    let down = to_bf(got.next_down().0, cc);
    // Not faithful iff a representable value sits strictly between
    // `got` and the true value: that happens exactly when `next_up`
    // is still strictly below the true value (so `next_up` itself lies
    // between `got` and `true`), or `next_down` is still strictly above
    // it.
    !(cmp_approx(&up, oracle, cc) == Ordering::Less
        || cmp_approx(&down, oracle, cc) == Ordering::Greater)
}

/// `true` iff `got` is on the spec-mandated side of the true value for
/// rounding mode `rm`:
///
/// * `TowardPositive` — the representable value `≥ true`.
/// * `TowardNegative` — the representable value `≤ true`.
/// * `TowardZero` — the adjacent value of smaller magnitude (toward
///   `−true` sign): floor for a positive result, ceil for a negative.
/// * the two to-nearest modes — either adjacent value (faithful, not
///   correctly rounded, per ADR-0021: §9.2 does not require the kernel
///   to resolve the tie the way an exact oracle would).
pub fn faithful_side_ok(
    got: Decimal128,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
) -> bool {
    if !within_faithful_ulp(got, oracle, cc) {
        return false;
    }
    let g = to_bf(got, cc);
    let side = cmp_approx(&g, oracle, cc);
    if side == Ordering::Equal {
        // `got` equals the true value (within the oracle dead-band):
        // exact, hence correct for every rounding direction.
        return true;
    }
    let is_floor = side == Ordering::Less; // got < true
    match rm {
        RoundingMode::NearestEven | RoundingMode::NearestAway => true,
        RoundingMode::TowardPositive => !is_floor,
        RoundingMode::TowardNegative => is_floor,
        RoundingMode::TowardZero => {
            // Smaller magnitude. The true value's sign equals `got`'s
            // here (they bracket each other within one ULP), so toward
            // zero is floor for a positive result and ceil for a
            // negative one.
            if oracle.is_negative() {
                !is_floor
            } else {
                is_floor
            }
        }
    }
}

/// Assert the full faithful-rounding contract for one transcendental
/// evaluation: `got` / `status` is what `ferrodec` returned for rounding
/// mode `rm`, and `oracle` is the true value computed by the caller at
/// `≥ 256`-bit precision. Panics with a diagnostic naming the operation
/// if the value is not faithfully rounded, is on the wrong side for a
/// directed mode, signals `INVALID` for an in-domain input, or fails to
/// flag `INEXACT` when the result is provably inexact.
pub fn assert_faithful(
    got: Decimal128,
    status: Status,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: RoundingMode,
    ctx: &str,
) {
    assert!(got.is_finite(), "{ctx} rm={rm:?}: got non-finite {got:?}");
    assert!(
        faithful_side_ok(got, oracle, cc, rm),
        "{ctx} rm={rm:?}: got {got:e} not faithfully rounded for this mode"
    );
    assert!(
        !status.invalid(),
        "{ctx} rm={rm:?}: in-domain input raised INVALID"
    );
    // `got` differs from the true value ⇒ the result is inexact ⇒
    // INEXACT is mandatory. When `got` equals the true value (within
    // the oracle dead-band) exactness cannot be proven from the oracle
    // alone, so INEXACT is not asserted.
    let g = to_bf(got, cc);
    let exact = cmp_approx(&g, oracle, cc) == Ordering::Equal;
    if !exact {
        assert!(
            status.inexact(),
            "{ctx} rm={rm:?}: inexact result did not raise INEXACT"
        );
    }
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
