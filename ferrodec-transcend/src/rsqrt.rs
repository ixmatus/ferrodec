//! `rsqrt(x)` — IEEE 754-2019 §9.2 `rSqrt`, the reciprocal square
//! root `1/√x` (ADR-0059 Track D group D3, under the ADR-0060 phase
//! gate).
//!
//! ## Why a Newton kernel and not `exp(−½·ln x)`
//!
//! The architecture is forced, not preferred. ADR-0060's Engine B
//! derivation gives `rSqrt` a *uniform* Liouville floor: for every
//! input not disposed of input side, the true value's relative
//! distance to every format grid point and every nearest mode midpoint
//! exceeds `4.9·10^-105`. A two rung build is unconditionally correctly
//! rounded exactly when the rung 2 budget clears that floor, which
//! needs `B₂ ≤ 10³`. The `exp(−½·ln x)` composition cannot: it inherits
//! the `|ln x| ≤ 14151` amplification that puts `ladder::CBRT` and
//! `ladder::POW` at the `1e8` scale, and a `1e8` budget on rung 2
//! resolves only to `~10^-101` — four decades short of the floor. The
//! direct Newton composition below prices at `ladder::RSQRT` = 400,
//! two and a half orders inside the ADR's ceiling.
//!
//! ## The kernel
//!
//! At the rung's working precision `E`:
//!
//! ```text
//! s = √x        (E::sqrt, Newton, seeded at the format's own sqrt)
//! y = 1/s       (E::recip, Newton, seeded at the format's own recip)
//! y ← y·(3 − x·y²)/2     (one division-free Newton polish)
//! ```
//!
//! **Order: `sqrt` first, then `recip`.** The two orders have the same
//! algebraic error story (each Newton step is self correcting, and
//! `sqrt` even halves whatever relative error it is handed), so the
//! tiebreak is the *intermediate range*, and there it is not close.
//! Both `E::sqrt` and `E::recip` seed themselves by rounding their
//! argument into the format `F` and calling `F`'s own kernel, so an
//! intermediate that leaves `F`'s normal range degrades or destroys the
//! seed. Taking `recip` first does exactly that: `recip` of the
//! smallest subnormal overflows the format (`1/10^-6176` at
//! `Decimal128` is `10^6176`, past `MAX`), so the seed would be `+∞`.
//! Taking `sqrt` first keeps every intermediate normal at every format:
//! `√x` spans `[10^etiny/2, 10^(emax+1)/2]` and its reciprocal the
//! mirrored band, and both sit strictly inside the format's normal
//! range by the table in "Result range" below.
//!
//! ## Why the composition is polished
//!
//! `E::sqrt` and `E::recip` seed at `F::PRECISION` digits and run a
//! *fixed* number of Newton steps on the fixed rungs (two at rung 1,
//! three at rung 2), a count calibrated for the 34-digit `Decimal128`
//! seed. Precision doubles per step, so the composition's guaranteed
//! correct digits are `F::PRECISION · 2^steps`, capped at the working
//! width:
//!
//! | format | rung 1 (50 digits) | rung 2 (110 digits) |
//! |---|---|---|
//! | `Decimal128` (34) | 136 → full | 272 → full |
//! | `Decimal64` (16) | 64 → full | 128 → full |
//! | `Decimal32` (7) | **28** | **56** |
//!
//! At `Decimal32` the bare composition therefore carries ~28 correct
//! digits on rung 1 and ~56 on rung 2 — 22 and 54 decades short of
//! their working widths, and so vastly outside a 400 unit budget. The
//! polish step repairs it without touching the shared Newton surface:
//! `y ← y·(3 − x·y²)/2` is the reciprocal square root iteration in its
//! division-free form (`y = (1+e)/√x` maps to `y = (1 − 1.5e² − …)/√x`),
//! it needs no seed and no division, so it is bounded by the working
//! width alone rather than by the seed chain. One step squares 28 into
//! 56 (past rung 1's 50) and 56 into 112 (past rung 2's 110), leaving
//! only its own five roundings: ≤ 5 units at every format and rung,
//! which is what `ladder::RSQRT`'s itemization charges.
//!
//! The step is unconditional rather than narrow-format-only: at
//! `Decimal64` and `Decimal128` the composition is already at the
//! working width, where the same step is a self correcting no-op that
//! costs its five roundings and buys uniformity of the budget argument
//! across the seam.
//!
//! ## Result range (no subnormal edge, at any format)
//!
//! For finite nonzero `x > 0` the result never reaches the subnormal
//! quantum region, so the ADR-0060 boundary analysis (which pins the
//! quantum at `10^(etiny−1)` in that region) never has to run its
//! subnormal branch here, and the kernel needs no underflow arm.
//! Checked per format against its own `etiny` and `emax`, at both ends
//! of the input range:
//!
//! | format | smallest `x` → largest result | largest `x` → smallest result | normal band |
//! |---|---|---|---|
//! | `Decimal128` | `10^-6176` → `10^3088` | `~10^6145` → `~3.16·10^-3073` | `[10^-6143, ~10^6145)` |
//! | `Decimal64` | `10^-398` → `10^199` | `~10^385` → `~3.16·10^-193` | `[10^-383, ~10^385)` |
//! | `Decimal32` | `10^-101` → `10^50` | `~10^97` → `~3.16·10^-49` | `[10^-95, ~10^97)` |
//!
//! Every result column sits strictly inside its format's normal band
//! with thousands of decades to spare on the wide formats and 46 on the
//! narrowest: `rSqrt` halves the exponent, and no format's exponent
//! range is more than twice as wide on one side as the other.
//!
//! ## No anchor arm
//!
//! Unlike `log10p1`, `exp10m1`, and `exp10`, this operation has no
//! grid-hugging family and therefore no ADR-0051 residual channel.
//! ADR-0060's ten-scaling argument is the reason: writing `u = 2v + r`
//! with `r ∈ {0,1}` gives `y = 10^-v · m^(-1/2)` with `m = a·10^r`, and
//! scaling by `10^v` maps every boundary near `y` to a boundary of the
//! same form near `y₀ = m^(-1/2)`. The input exponent drops out
//! entirely — powers of ten couple multiplicatively through the root —
//! so the floor is uniform over the whole exponent range instead of
//! degrading at its ends. That is precisely the asymptotic degradation
//! the anchor channel exists to catch, and it does not happen here.
//!
//! ## Accuracy
//!
//! Correctly rounded, and *unconditionally* so in the default two rung
//! build (ADR-0060's verdict for this operation): the input side
//! classification is complete (`exact::rsqrt_exact_input`), the
//! Liouville floor `4.9·10^-105` is proven, and rung 2's 400 unit
//! budget resolves to `4·10^-108`, clearing the floor by more than two
//! orders. The exact integer adjudicator ADR-0060 specifies for this
//! group — which removes the margin question entirely by deciding the
//! one candidate boundary in `U384` integer arithmetic — is forthcoming
//! and is not wired here.

use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Reciprocal square root `1/√self` (IEEE 754-2019 §9.2 `rSqrt`).
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exactness is decided from the input alone: stripped
/// `x = 2^A · 5^B · s · 10^0` is a rational reciprocal square root iff
/// `s = 1` and both `A` and `B` are even, and the value is then the
/// pure power of two or of five `2^-A/2 · 5^-B/2` folded to a decimal.
/// Both the completeness proof and the tie discussion (the powers of
/// five at width `PRECISION + 1` are real nearest mode midpoints) live
/// on `exact::rsqrt_exact_input`. Past the classifier the result is
/// irrational or a non-terminating rational (`rsqrt(9) = 1/3`), so the
/// unconditional `INEXACT` is correct in every mode.
pub fn rsqrt_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| rsqrt_kernel_body::<F, _>(ex, x, rm))
}

/// The IEEE 754-2019 §9.2.1 dispositions for `rSqrt`, transcribed from
/// the standard; `None` means finite positive nonzero, the kernel's
/// domain.
///
/// * "rSqrt(+∞) is +0 with no exception."
/// * "rSqrt(±0) is ±∞ and signals the divideByZero exception." The
///   sign is preserved: `rSqrt(−0)` is `−∞`, the one place in this
///   operation where a negative operand does not raise `INVALID`.
/// * Every other negative operand — finite and `−∞` alike — is a
///   domain error: `1/√x` has no real value for `x < 0`, so the answer
///   is a quiet NaN with `INVALID`, the same disposition `ln` gives its
///   negative domain (`ln::ln_special_cases`).
/// * NaN propagates per the crate convention: a signaling NaN raises
///   `INVALID` and returns the quieted payload, a quiet NaN passes
///   through untouched.
pub(crate) fn rsqrt_special_cases<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { sign } => Some(if sign {
            (F::NAN, Status::INVALID)
        } else {
            (F::ZERO, Status::OK)
        }),
        Class::Zero { sign, .. } => Some(if sign {
            (F::NEG_INFINITY, Status::DIV_BY_ZERO)
        } else {
            (F::INFINITY, Status::DIV_BY_ZERO)
        }),
        Class::Finite { sign, .. } if sign => Some((F::NAN, Status::INVALID)),
        Class::Finite { .. } => None,
    }
}

/// Generic body of [`rsqrt_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn rsqrt_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = rsqrt_special_cases(x) {
        return Some(early);
    }

    // Input-side exactness and ties (ADR-0059 M7): a rational
    // `1/√x` — necessarily a pure power of two or of five over a power
    // of ten — is delivered here, every rounding direction, before any
    // approximation runs. The ties are real (`rsqrt(2^98) = 5^49·10^-49`
    // is a 35-digit midpoint at `Decimal128`), and no approximation
    // kernel can resolve a value that *is* a rounding boundary.
    if let Some(exact) = crate::exact::rsqrt_exact_input::<F>(x, rm) {
        return Some(exact);
    }

    let x_ext = ex.from_format(x);
    // sqrt first, then reciprocal: both Newton surfaces seed through
    // the format, and only this order keeps every intermediate inside
    // the format's normal range (module doc, "The kernel").
    let y = rsqrt_polish(ex, x_ext, x_ext.sqrt::<F>().recip::<F>());

    // `round_guarded` raises INEXACT unconditionally, and here that is
    // correct: the exact and tie cases returned above, and past the
    // classifier `1/√x` is irrational or a non-terminating rational
    // (`exact::rsqrt_exact_input`), never on a boundary.
    ladder::round_guarded::<F, E>(y, rm, &ladder::RSQRT)
}

/// One division-free Newton step of the reciprocal square root:
/// `y ← y·(3 − x·y²)/2`, for `x > 0` and a `y` already within a few
/// percent of `1/√x`.
///
/// Writing `y = (1 + e)/√x` gives `x·y² = (1 + e)²` and
/// `y·(3 − (1+e)²)/2 = (1 − 1.5e² − 0.5e³)/√x`: the relative error is
/// squared, and the step needs neither a seed nor a division, so its
/// accuracy is bounded by the working width alone rather than by the
/// seed chain that limits `ExtNum::sqrt` and `ExtNum::recip` at the
/// narrow formats. The five roundings it costs (square, multiply,
/// subtract, multiply, halve) are the whole of what
/// `ladder::RSQRT` has left to charge; the module doc's "Why the
/// composition is polished" carries the per-format derivation.
pub(crate) fn rsqrt_polish<E: ExtNum>(ex: E, x: E, y: E) -> E {
    y.mul(ex.from_i32(3).sub(x.mul(y.square()))).div_u32(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extended::Extended;
    use crate::extended2::Extended2;
    use crate::mock_format::ValueFmt128;

    /// `1/√2` to 130 correct digits, the reference the polish step is
    /// measured against. Computed offline at 200 digits by Python's
    /// `decimal` module (`Decimal(1) / Decimal(2).sqrt()`) and truncated
    /// — an independent arbitrary-precision evaluation, never this
    /// kernel's own output.
    const INV_SQRT2: &str = "0.7071067811865475244008443621048490392848359376884740365883398689953662392310535194251937671638207863675069231154561485124624180";

    /// Feed the polish step a `y` deliberately truncated to `digits`
    /// correct digits — the width the shared Newton surface leaves the
    /// composition at, per the module doc's table — and require the
    /// result to reproduce `x·y² = 1` to within `1e-(prec−2)`, i.e. ten
    /// working ulps of 1. This is the executable form of the budget's
    /// central claim, and it needs no format seeds (the mock's are
    /// `unreachable!`), which is exactly the property that makes the
    /// step immune to the seed chain in the first place.
    fn assert_polish_reaches_the_working_width<E: ExtNum>(ex: E, digits: usize) {
        let x = ex.from_i32(2);
        // 1/√2 = 0.707…, so the significant digits start after "0.".
        let truncated = &INV_SQRT2[..2 + digits];
        let y = rsqrt_polish(ex, x, ex.parse_str(truncated));
        let residual = x.mul(y.square()).sub(ex.one()).abs();
        let prec = i32::try_from(ex.precision()).expect("working precision fits i32");
        let bound = ex.from_parts_u128(1, -(prec - 2), false);
        assert!(
            residual.cmp(bound) != core::cmp::Ordering::Greater,
            "polish from {digits} digits at precision {prec}: residual \
             {residual:?} exceeds ten working ulps ({bound:?})"
        );
    }

    /// Rung 1 (50 digits): the composition's narrowest guaranteed width
    /// is `Decimal32`'s 28 digits (a 7-digit seed doubled twice), and
    /// one polish step must carry that to the full working width.
    #[test]
    fn polish_lifts_the_rung1_narrow_format_width() {
        for digits in [28, 34, 50] {
            assert_polish_reaches_the_working_width(Extended::ZERO, digits);
        }
    }

    /// Rung 2 (110 digits): the narrowest guaranteed width is
    /// `Decimal32`'s 56 digits (7 doubled three times).
    #[test]
    fn polish_lifts_the_rung2_narrow_format_width() {
        for digits in [56, 64, 110] {
            assert_polish_reaches_the_working_width(Extended2::ZERO, digits);
        }
    }

    /// The step is self-correcting, so applying it twice changes
    /// nothing beyond its own rounding: the second application must
    /// stay inside the same ten-ulp band. This is what licenses running
    /// it unconditionally at the wide formats, where the composition
    /// already sits at the working width.
    #[test]
    fn polish_is_idempotent_at_the_working_width() {
        let ex = Extended::ZERO;
        let x = ex.from_i32(2);
        // Parsed at the rung's own width: `parse_str` takes literals up
        // to the working precision, and 50 digits is already past what
        // the composition guarantees at any format.
        let at_width = &INV_SQRT2[..2 + ex.precision() as usize];
        let once = rsqrt_polish(ex, x, ex.parse_str(at_width));
        let twice = rsqrt_polish(ex, x, once);
        let residual = x.mul(twice.square()).sub(ex.one()).abs();
        let bound = ex.from_parts_u128(1, -48, false);
        assert!(
            residual.cmp(bound) != core::cmp::Ordering::Greater,
            "second polish drifted: residual {residual:?}"
        );
    }

    /// The §9.2.1 zero row on the mock format: `rSqrt(±0)` is `±∞` and
    /// signals `divideByZero`, sign preserved. The mock names no NaN or
    /// infinity encodings of its own, so the value assertion is on the
    /// sign only; the full per-format table lives in
    /// `tests/transcend_exact_rsqrt.rs` and the sibling mirrors.
    #[test]
    fn zero_signals_divide_by_zero_with_the_sign_preserved() {
        for sign in [false, true] {
            let (r, st) = rsqrt_special_cases::<ValueFmt128>(ValueFmt128 {
                coef: 0,
                exp: 0,
                sign,
            })
            .expect("zero is a special case");
            assert_eq!(r.sign, sign, "rsqrt(±0) preserves the sign");
            assert!(st.div_by_zero(), "rsqrt(±0) signals divideByZero");
            assert!(!st.inexact(), "rsqrt(±0) is not inexact");
        }
    }

    /// A finite negative operand is a domain error, and a positive
    /// finite one falls through to the kernel.
    #[test]
    fn negative_finite_is_invalid_and_positive_finite_falls_through() {
        let (_, st) = rsqrt_special_cases::<ValueFmt128>(ValueFmt128 {
            coef: 4,
            exp: 0,
            sign: true,
        })
        .expect("a negative finite operand is a domain error");
        assert!(st.invalid(), "rsqrt(negative) raises INVALID");
        assert!(
            rsqrt_special_cases::<ValueFmt128>(ValueFmt128 {
                coef: 4,
                exp: 0,
                sign: false,
            })
            .is_none(),
            "a positive finite operand is the kernel's domain"
        );
    }
}
