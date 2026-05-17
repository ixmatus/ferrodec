//! Generic faithful-rounding oracle harness for the transcendental
//! ("recommended") operations, shared by every decimal sibling's
//! `tests/property_*.rs` suite.
//!
//! ## The faithful-rounding contract (ADR-0021, IEEE 754-2019 §9.2)
//!
//! IEEE 754-2019 §9.2 *recommends* but does not *require* correctly
//! rounded results for the transcendental ("recommended") operations.
//! The `ferrodec` family therefore declares the weaker **faithful
//! rounding** contract for every transcendental: the returned value is
//! one of the two representable values immediately adjacent to the exact
//! mathematical result (equivalently: no representable value lies
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
//! directed rounding modes additionally pin *which* of the two adjacent
//! values the spec mandates for that direction.
//!
//! ## Generic over the consumer's decimal type
//!
//! `ferrodec-test-support` must not depend on `ferrodec` /
//! `ferrodec-decimal64` / `ferrodec-decimal32` (those dev-depend on
//! *this* crate, so a reverse edge would be circular). The harness is
//! therefore generic over the [`FaithfulFormat`](crate::transcend_oracle::FaithfulFormat)
//! trait, which the
//! *consumer* implements for its concrete decimal type. The astro-float
//! oracle math, the `P_CMP = 256`-bit comparison precision, the
//! `1e-40` / `1e-300` dead-band, and the directed-mode side logic are
//! all format-independent and live here once.
//!
//! ## Two-tier usage (design note)
//!
//! The trait is shaped for two faithfulness routes:
//!
//! * **Direct tier** (Decimal128, decimal64): the consumer implements
//!   [`FaithfulFormat`](crate::transcend_oracle::FaithfulFormat) directly on its widest
//!   format and asserts the bracket against astro-float at that
//!   format's own boundary. The `next_up` / `next_down` / `sci`
//!   operations are the format's own.
//! * **Widen tier** (decimal32): the consumer implements
//!   [`FaithfulFormat`](crate::transcend_oracle::FaithfulFormat) on a *carrier* that
//!   losslessly widens decimal32
//!   to decimal64 (the `ferrodec-decimal32/tests/d64_crosscheck.rs`
//!   pattern), applying the faithful bracket at the decimal32 boundary
//!   while doing the structural neighbour walk in the wider format.
//!
//! Nothing here bakes in a Decimal128 specific (bit width, exponent
//! range, BID encoding): the only requirements are an exact scientific
//! string form, structural `next_up` / `next_down`, finiteness, and the
//! two status predicates. P1+ wires the sibling tiers; P0b only proves
//! the Decimal128 consumer stays byte-for-byte green.

use core::cmp::Ordering;

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};

/// The decimal-format capabilities the faithful-rounding harness needs.
///
/// Implemented by the *consumer* crate for its concrete decimal type so
/// that `ferrodec-test-support` carries no reverse dependency on the
/// decimal crates. Every method's semantics must match the format's own
/// IEEE behaviour exactly; the harness reasons about faithfulness purely
/// through this surface.
pub trait FaithfulFormat: Copy {
    /// The format's rounding-direction type (e.g. `RoundingMode`).
    type Rounding: Copy + core::fmt::Debug + 'static;

    /// The format's status-flag type (e.g. `Status`).
    type Status: Copy;

    /// The five IEEE 754-2019 rounding directions, asserted for every
    /// transcendental input so the directional faithful-rounding
    /// contract is exercised on each mode rather than only
    /// round-to-nearest-even. Order is irrelevant; the set must be the
    /// five spec directions.
    const MODES: &'static [Self::Rounding];

    /// Parse a decimal literal at round-half-even, panicking on invalid
    /// input. The shape every property test wants for hand-curated
    /// reference values.
    fn parse_nearest_even(s: &str) -> Self;

    /// Forced-scientific (`{:e}`, never `Display`) form, preserving the
    /// exact significand digits and exponent.
    fn sci(self) -> String;

    /// The next representable value above `self`.
    #[must_use]
    fn next_up(self) -> Self;

    /// The next representable value below `self`.
    #[must_use]
    fn next_down(self) -> Self;

    /// Whether `self` is finite (not infinity / NaN).
    fn is_finite(self) -> bool;

    /// Whether `st` raised the IEEE `INVALID` flag.
    fn raised_invalid(st: Self::Status) -> bool;

    /// Whether `st` raised the IEEE `INEXACT` flag.
    fn raised_inexact(st: Self::Status) -> bool;

    /// Classify a concrete rounding mode into the format-independent
    /// [`SpecRounding`] the faithful-side decision needs. The two
    /// to-nearest modes collapse to [`SpecRounding::Nearest`] (faithful,
    /// not correctly rounded, per ADR-0021); only the three directed
    /// modes pin a side. Lives on the trait (rather than a blanket
    /// `From` impl) so the consumer crate can supply it without an
    /// orphan-rule violation.
    fn spec_rounding(rm: Self::Rounding) -> SpecRounding;
}

/// The five IEEE 754-2019 rounding directions for the format `F`,
/// asserted for every transcendental input so the directional
/// faithful-rounding contract is exercised on each mode rather than only
/// round-to-nearest-even.
pub fn modes<F: FaithfulFormat>() -> &'static [F::Rounding] {
    F::MODES
}

/// Parse a decimal literal at round-half-even, panicking on invalid
/// input. The shape every property test wants for hand-curated
/// reference values.
pub fn parse<F: FaithfulFormat>(s: &str) -> F {
    F::parse_nearest_even(s)
}

/// Precision for the internal decimal → `BigFloat` conversions used to
/// compare `got` and its neighbours against the oracle. A `Decimal128`
/// carries ≤ 34 significant digits, so 256 bits (≈ 77 digits) holds
/// every converted value exactly; wider siblings stay well inside it.
const P_CMP: usize = 256;

/// Exact conversion of a finite decimal value to a `BigFloat`. Uses the
/// forced-scientific [`FaithfulFormat::sci`] form (never `Display`),
/// which preserves the exact significand digits and exponent.
fn to_bf<F: FaithfulFormat>(d: F, cc: &mut Consts) -> BigFloat {
    BigFloat::parse(&d.sci(), Radix::Dec, P_CMP, AfRm::None, cc)
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
/// value `oracle`: no representable value lies strictly between `got`
/// and the true result. Equivalent to "`got` is one of the two
/// representable values adjacent to the true value".
pub fn within_faithful_ulp<F: FaithfulFormat>(got: F, oracle: &BigFloat, cc: &mut Consts) -> bool {
    let up = to_bf(got.next_up(), cc);
    let down = to_bf(got.next_down(), cc);
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
/// * toward-positive — the representable value `≥ true`.
/// * toward-negative — the representable value `≤ true`.
/// * toward-zero — the adjacent value of smaller magnitude (toward
///   `−true` sign): floor for a positive result, ceil for a negative.
/// * the two to-nearest modes — either adjacent value (faithful, not
///   correctly rounded, per ADR-0021: §9.2 does not require the kernel
///   to resolve the tie the way an exact oracle would).
///
/// The directed-mode decision is keyed by [`SpecRounding`], a
/// format-independent classification the consumer derives from its
/// concrete rounding mode, so the directional logic here is identical
/// across siblings.
pub fn faithful_side_ok<F: FaithfulFormat>(
    got: F,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: F::Rounding,
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
    match F::spec_rounding(rm) {
        SpecRounding::Nearest => true,
        SpecRounding::TowardPositive => !is_floor,
        SpecRounding::TowardNegative => is_floor,
        SpecRounding::TowardZero => {
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

/// Format-independent classification of a rounding direction, as the
/// faithful-side decision needs it. The two to-nearest modes
/// (nearest-even, nearest-away) collapse to [`SpecRounding::Nearest`]
/// because §9.2 does not require a transcendental kernel to resolve the
/// tie the way an exact oracle would; only the three directed modes pin
/// a side. The consumer maps its concrete `RoundingMode` onto this via
/// `From` so the directional match here is shared verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecRounding {
    /// Either to-nearest mode (even or away): either adjacent value.
    Nearest,
    /// Round toward +∞: the representable value `≥ true`.
    TowardPositive,
    /// Round toward −∞: the representable value `≤ true`.
    TowardNegative,
    /// Round toward zero: the adjacent value of smaller magnitude.
    TowardZero,
}

/// Assert the full faithful-rounding contract for one transcendental
/// evaluation: `got` / `status` is what the kernel returned for rounding
/// mode `rm`, and `oracle` is the true value computed by the caller at
/// `≥ 256`-bit precision. Panics with a diagnostic naming the operation
/// if the value is not faithfully rounded, is on the wrong side for a
/// directed mode, signals `INVALID` for an in-domain input, or fails to
/// flag `INEXACT` when the result is provably inexact.
pub fn assert_faithful<F: FaithfulFormat>(
    got: F,
    status: F::Status,
    oracle: &BigFloat,
    cc: &mut Consts,
    rm: F::Rounding,
    ctx: &str,
) {
    assert!(
        got.is_finite(),
        "{ctx} rm={rm:?}: got non-finite {}",
        got.sci()
    );
    assert!(
        faithful_side_ok(got, oracle, cc, rm),
        "{ctx} rm={rm:?}: got {} not faithfully rounded for this mode",
        got.sci()
    );
    assert!(
        !F::raised_invalid(status),
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
            F::raised_inexact(status),
            "{ctx} rm={rm:?}: inexact result did not raise INEXACT"
        );
    }
}
