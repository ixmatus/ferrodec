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

use astro_float::{Radix, RoundingMode as AfRm};

/// Re-export of astro-float's [`BigFloat`] and [`Consts`] so a consumer
/// crate can name the oracle's true-value and constants-cache types
/// (`oracle::exp(&exact, &mut cc)` returns a `BigFloat`; `Consts` seeds
/// it) without taking a direct `astro-float` dependency of its own. The
/// decimal32 manifest stays astro-float-free (Design-A constraint):
/// astro-float compiles only transitively inside this test-support
/// crate. decimal64 / Decimal128 keep their own `use astro_float::...`
/// and are unaffected by this re-export.
pub use astro_float::{BigFloat, Consts};

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

/// The wider of the two adjacent representable gaps at `v`
/// (`|bf(next_up(v)) − bf(v)|` versus `|bf(v) − bf(next_down(v))|`), as a
/// `BigFloat`. The max of the one-sided gaps is conservative across a
/// power-of-ten cohort boundary, where the up-gap and the down-gap
/// differ by a factor of ten; taking the larger never makes an
/// identity band falsely tight.
fn ulp_at<F: FaithfulFormat>(v: F, cc: &mut Consts) -> BigFloat {
    let c = to_bf(v, cc);
    let up = to_bf(v.next_up(), cc);
    let dn = to_bf(v.next_down(), cc);
    let gap_up = up.sub(&c, P_CMP, AfRm::None).abs();
    let gap_dn = c.sub(&dn, P_CMP, AfRm::None).abs();
    if gap_up.abs_cmp(&gap_dn).expect("finite gaps") >= 0 {
        gap_up
    } else {
        gap_dn
    }
}

/// `true` iff `got` lies within `n_ulps` representable steps of `want`,
/// i.e. `|got − want| ≤ n_ulps · ulp(want)`.
///
/// This is the structural band for **algebraic identity cross-checks**
/// (metamorphic tests), *not* the IEEE faithful-rounding contract
/// ([`within_faithful_ulp`]). A composed identity accumulates more than
/// one ULP by construction, and an ill-conditioned composition
/// accumulates a condition-number multiple of a ULP. The caller derives
/// `n_ulps` per identity from the analytic condition number evaluated at
/// the test point (see ADR-0025); this routine only enforces the band
/// the caller specifies.
///
/// `want` is an exact representable value (the identity's right-hand
/// side), so the comparison carries no oracle noise: `to_bf` is exact
/// and the `cmp_approx` dead-band is deliberately kept off this path.
///
/// O(1) in `n_ulps`: the gap is computed once and scaled, never walked
/// `n` times. A condition-amplified `n_ulps` can be `~10^5`, so walking
/// would be both slow and pointless.
pub fn within_n_ulp_band<F: FaithfulFormat>(got: F, want: F, n_ulps: u32, cc: &mut Consts) -> bool {
    if !got.is_finite() || !want.is_finite() {
        return false;
    }
    let g = to_bf(got, cc);
    let w = to_bf(want, cc);
    let diff = g.sub(&w, P_CMP, AfRm::None).abs();
    let ulp = ulp_at(want, cc);
    let n = BigFloat::parse(
        &n_ulps.max(1).to_string(),
        Radix::Dec,
        P_CMP,
        AfRm::None,
        cc,
    );
    let tol = ulp.mul(&n, P_CMP, AfRm::None);
    diff.abs_cmp(&tol).expect("finite diff/tol") <= 0
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

/// Exact high-precision oracle values for the exp-log family of
/// transcendentals, computed once here in astro-float so every decimal
/// sibling's `tests/property_*.rs` shares one definition.
///
/// Each builder parses the exact scientific string of an input (the
/// [`FaithfulFormat::sci`] form) at [`oracle::P`]-bit
/// precision, applies the
/// operation with `RoundingMode::None` (the recommendation in the
/// astro-float docs when a sub-ULP function-approximation error is
/// acceptable, since the comparison dead-band absorbs it), and returns
/// the resulting [`BigFloat`] for [`assert_faithful`].
///
/// Centralising these means the direct tier (decimal64, Decimal128) and
/// the widen tier (decimal32, which renders its input through a lossless
/// decimal64 carrier) feed the *same* exact 256-bit oracle, so
/// faithfulness soundness is uniform across siblings and astro-float
/// stays confined to this crate.
pub mod oracle {
    use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};

    /// Working precision for the astro-float oracle: 256 bits
    /// (`≈ 77` decimal digits), far above the widest sibling's faithful
    /// bracket (36 digits for `Decimal128`) and identical to the
    /// `P_CMP` comparison precision the harness converts against.
    pub const P: usize = 256;

    /// Parse the exact scientific string of an input into a `BigFloat`
    /// at oracle precision.
    fn arg(x_str: &str, cc: &mut Consts) -> BigFloat {
        BigFloat::parse(x_str, Radix::Dec, P, AfRm::None, cc)
    }

    /// `e^x` of the exact value `x_str`.
    pub fn exp(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).exp(P, AfRm::None, cc)
    }

    /// `2^x` of the exact value `x_str`. astro-float has no direct
    /// `exp2`; `2.pow(x)` computes it at full oracle precision.
    pub fn exp2(x_str: &str, cc: &mut Consts) -> BigFloat {
        let x = arg(x_str, cc);
        let two = BigFloat::from_word(2, P);
        two.pow(&x, P, AfRm::None, cc)
    }

    /// Natural logarithm of the exact value `x_str`.
    pub fn ln(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).ln(P, AfRm::None, cc)
    }

    /// Base-2 logarithm of the exact value `x_str`.
    pub fn log2(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).log2(P, AfRm::None, cc)
    }

    /// Base-10 logarithm of the exact value `x_str`.
    pub fn log10(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).log10(P, AfRm::None, cc)
    }

    /// Cube root of the exact value `x_str`. `cc` is consumed parsing
    /// the argument; astro-float's `cbrt` itself needs no constants
    /// cache, so all six builders still share one call shape across the
    /// property suites.
    pub fn cbrt(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).cbrt(P, AfRm::None)
    }

    /// `x_str` raised to the power `y_str`, both parsed at oracle
    /// precision. astro-float's `pow(&self, &n, ...)` computes the
    /// general `xʸ` at full precision; the `exp2` builder above uses
    /// the same primitive with a literal base `2`. Centralising the
    /// genuine two-argument `pow(x, y)` here lets the direct tier
    /// (Decimal128, decimal64) and the astro-float-free decimal32
    /// widen tier feed the same exact 256-bit oracle, the same reason
    /// the unary builders are shared.
    pub fn pow(x_str: &str, y_str: &str, cc: &mut Consts) -> BigFloat {
        let x = arg(x_str, cc);
        let y = arg(y_str, cc);
        x.pow(&y, P, AfRm::None, cc)
    }

    /// Sine, in radians, of the exact value `x_str`.
    pub fn sin(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).sin(P, AfRm::None, cc)
    }

    /// Cosine, in radians, of the exact value `x_str`.
    pub fn cos(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).cos(P, AfRm::None, cc)
    }

    /// Sine of the exact value `x_str` computed at a caller-chosen
    /// working precision `p_bits` instead of the fixed [`P`].
    ///
    /// Large-magnitude `sin` / `cos` need the 2/π argument reduction
    /// carried at a precision that absorbs the input's magnitude
    /// before the residual is extracted; the fixed 256-bit [`sin`]
    /// builder loses the residual once `|x|` exceeds `~10^70`. The
    /// `property_sincos_large` suites pass `p_bits` scaled to the
    /// input magnitude (the same `≈ 3.4` bits per decimal digit rule
    /// the direct-tier Decimal128 suite uses). The widened result is
    /// still in `[-1, 1]`, so the harness's 256-bit comparison
    /// (`P_CMP`) brackets it exactly. Keeping this in the shared
    /// builder set is what lets the astro-float-free decimal32 widen
    /// tier reach a magnitude-widened oracle without naming
    /// astro-float.
    pub fn sin_at(x_str: &str, p_bits: usize, cc: &mut Consts) -> BigFloat {
        let x = BigFloat::parse(x_str, Radix::Dec, p_bits, AfRm::None, cc);
        x.sin(p_bits, AfRm::None, cc)
    }

    /// Cosine of the exact value `x_str` at a caller-chosen working
    /// precision `p_bits`. The `cos` analogue of [`sin_at`]; see that
    /// builder for why a magnitude-scaled precision is required.
    pub fn cos_at(x_str: &str, p_bits: usize, cc: &mut Consts) -> BigFloat {
        let x = BigFloat::parse(x_str, Radix::Dec, p_bits, AfRm::None, cc);
        x.cos(p_bits, AfRm::None, cc)
    }

    /// Tangent, in radians, of the exact value `x_str`.
    pub fn tan(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).tan(P, AfRm::None, cc)
    }

    /// Inverse sine of the exact value `x_str` (domain `[-1, +1]`).
    pub fn asin(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).asin(P, AfRm::None, cc)
    }

    /// Inverse cosine of the exact value `x_str` (domain `[-1, +1]`).
    pub fn acos(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).acos(P, AfRm::None, cc)
    }

    /// Inverse tangent of the exact value `x_str`.
    pub fn atan(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).atan(P, AfRm::None, cc)
    }

    /// Two-argument arctangent `atan2(y, x)`, in radians, of the exact
    /// values `y_str` / `x_str`. astro-float has no native `atan2`;
    /// it is synthesized as `atan(y / x)` plus the quadrant shift
    /// (`±π` when `x < 0`, the sign taken from `y`), the exact
    /// construction the `Decimal128` `property_inverse_trig` suite's
    /// `check_atan2` used before this builder centralised it. The
    /// caller passes the sign bits of the parsed `y` / `x` (via the
    /// format's own `is_sign_negative`) so the quadrant decision is
    /// not a re-parse of the strings.
    pub fn atan2(
        y_str: &str,
        x_str: &str,
        y_is_neg: bool,
        x_is_neg: bool,
        cc: &mut Consts,
    ) -> BigFloat {
        let yv = arg(y_str, cc);
        let xv = arg(x_str, cc);
        let pi_bf = cc.pi(P, AfRm::None);
        let q = yv.div(&xv, P, AfRm::None);
        let mut oracle = q.atan(P, AfRm::None, cc);
        if x_is_neg {
            if y_is_neg {
                oracle = oracle.sub(&pi_bf, P, AfRm::None);
            } else {
                oracle = oracle.add(&pi_bf, P, AfRm::None);
            }
        }
        oracle
    }

    /// Hyperbolic sine of the exact value `x_str`.
    pub fn sinh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).sinh(P, AfRm::None, cc)
    }

    /// Hyperbolic cosine of the exact value `x_str`.
    pub fn cosh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).cosh(P, AfRm::None, cc)
    }

    /// Hyperbolic tangent of the exact value `x_str`.
    pub fn tanh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).tanh(P, AfRm::None, cc)
    }

    /// Inverse hyperbolic sine of the exact value `x_str` (defined for
    /// all real `x`).
    pub fn asinh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).asinh(P, AfRm::None, cc)
    }

    /// Inverse hyperbolic cosine of the exact value `x_str` (domain
    /// `[1, +∞)`).
    pub fn acosh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).acosh(P, AfRm::None, cc)
    }

    /// Inverse hyperbolic tangent of the exact value `x_str` (domain
    /// `(-1, +1)`).
    pub fn atanh(x_str: &str, cc: &mut Consts) -> BigFloat {
        arg(x_str, cc).atanh(P, AfRm::None, cc)
    }
}
