//! The ADR-0060 exact integer adjudicator: the rung 2 ambiguous-path
//! side decision for the algebraic §9.2 group (`rSqrt`, `hypot`,
//! `pown`, `rootn`, `compound`).
//!
//! ## What it decides
//!
//! Rung 2's guarded delivery locates, through
//! [`crate::ladder::BoundaryVerdict::Near`], the single candidate
//! boundary `b = C · 10^E` its working error cannot rule out. For the
//! five operations here the true value `y` satisfies a known integer
//! relation of bounded size — `y` itself, or its `d`-th power, is an
//! explicit rational of the operands — so "which side of `b`" is
//! decidable exactly in fixed-width integer arithmetic: never a
//! probability model, never a wider float. The engineering shape is
//! ADR-0060's own: replacing "the hash almost surely has no
//! collision, with a written probability" by a content compare on the
//! essentially-never-taken collision path.
//!
//! Each decider returns `Some(Side)` on its tabulated adjudicable
//! operand range and `None` outside it; on `None` the caller falls
//! back to the build's pre-adjudicator behavior, which the tier
//! language prices. The relations are transcribed from ADR-0060 and
//! the D3 classifier rustdoc (`crate::exact`), not rederived.
//!
//! ## Contract with the seam
//!
//! Every decider's `b` is the `Near` payload of a rung 2
//! [`crate::extended2::Extended2::candidate_boundary`] verdict for the
//! same operands: an exact rational within one working-precision
//! budget of the working value, hence within two budgets of the true
//! value — dozens of decimal orders inside half a format quantum. The
//! deciders never rely on that closeness for *correctness* (the
//! integer comparisons are exact at any distance); it is what makes
//! the aligned widths below small enough for `U1024`, and the decade
//! guard in [`cmp_aligned`] keeps every comparison total anyway.
//!
//! ## Completeness premise (why `Equal` panics)
//!
//! A comparison landing `Equal` means `y = b` exactly: the true value
//! IS a format grid point or nearest-mode midpoint. The input-side
//! exact and tie classifiers (`crate::exact`) deliver every such
//! input before any kernel runs, and that completeness is a stated
//! premise of every ADR-0060 floor (tripod leg 1). `Equal` here
//! therefore witnesses a classifier defect, and the adjudicator
//! panics loudly instead of guessing a side — the same posture as
//! `ladder_audit`, made unconditional.
//!
//! ## Envelope discipline
//!
//! Comparands fold into [`U1024`] (308 decimal digits). The widest
//! relations, at Decimal128 with midpoint boundaries (`C < 10^35`,
//! stripped format coefficients `a, c < 10^34`):
//!
//! | decider | widest comparand | digits |
//! |---|---|---|
//! | `rsqrt_side` | `c · C²` | ≤ 104 |
//! | `hypot_side` | `S < 2·10^170` | ≤ 171 |
//! | `powi_side`, `n = −6` | `C · a⁶` | ≤ 239 |
//! | `rootn_side`, `n = −6` | `a · C⁶` | ≤ 244 |
//! | `compound_side`, `|n|·w ≤ 196` | `C · N'^{|n|}` | ≤ 231 |
//!
//! plus the same-decade alignment shift of [`cmp_aligned`], which by
//! construction lands the shifted side exactly on the other's width:
//! every comparison stays under 245 digits, two decades inside the
//! envelope. The `U768` widths (`hypot`, `rootn`/`pown` to `|n| = 5`)
//! and the `U1024`-only rows (`rootn` and `pown` at `|n| = 6`) are
//! ADR-0060's width table, which is where the operand ranges of the
//! unconditional tier come from.

use crate::format::DecimalFormat;
use crate::ladder::Boundary;
use core::cmp::Ordering;
use ferrodec_multiword::{U1024, U256};

/// The true value's side of the candidate boundary, in magnitude:
/// every kernel here delivers through a positive working magnitude
/// (sign is reapplied by the kernel's own reflection), so the
/// deciders compare magnitudes and the delivery maps `Above` to the
/// growing side of the residual channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// `y < b` strictly.
    Below,
    /// `y > b` strictly.
    Above,
}

/// `sign(m1 · 10^e1 − m2 · 10^e2)` for nonzero comparands, total by
/// construction: `v_i ∈ [10^(hi_i − 1), 10^hi_i)` with
/// `hi_i = e_i + digits(m_i)`, so separated decades decide without
/// arithmetic, and the same-decade compare shifts one side by
/// `|e1 − e2| = |digits(m2) − digits(m1)|`, landing it exactly on the
/// other's width (≤ 308 digits, inside the envelope).
fn cmp_aligned(m1: U1024, e1: i64, m2: U1024, e2: i64) -> Ordering {
    debug_assert!(!m1.is_zero() && !m2.is_zero());
    let hi1 = e1 + i64::from(m1.decimal_digit_count());
    let hi2 = e2 + i64::from(m2.decimal_digit_count());
    if hi1 < hi2 {
        return Ordering::Less;
    }
    if hi1 > hi2 {
        return Ordering::Greater;
    }
    if e1 >= e2 {
        m1.mul_pow10((e1 - e2) as u32).cmp(m2)
    } else {
        m1.cmp(m2.mul_pow10((e2 - e1) as u32))
    }
}

/// Map the exact comparison (`y`-proportional side first) to the
/// delivered side; `Equal` is the completeness panic the module doc
/// derives.
fn side_of(ord: Ordering, op: &'static str) -> Side {
    match ord {
        Ordering::Greater => Side::Above,
        Ordering::Less => Side::Below,
        Ordering::Equal => panic!(
            "adjudicator ({op}): an exact boundary value reached the \
             kernel — the input-side exact and tie classification is \
             incomplete for this operation (ADR-0060 tripod leg 1)"
        ),
    }
}

/// `base^q` by a plain multiply fold, `None` on envelope overflow.
/// Linear rather than square-and-multiply on purpose: the fold runs
/// only behind a rung 2 ambiguity (~10^−66 of calls), so simplicity
/// is worth more than the saved multiplications, and the range gates
/// make the `None` unreachable anyway (kept so the fold is total).
fn pow_fold(base: U1024, q: u32) -> Option<U1024> {
    debug_assert!(q >= 1, "pow_fold is called with q ≥ 1 only");
    let mut acc = base;
    let mut i = 1;
    while i < q {
        acc = acc.checked_mul(base)?;
        i += 1;
    }
    Some(acc)
}

/// Decode a finite nonzero operand's magnitude into stripped
/// `(a, u)` parts with `a` in `u128`. `None` for the classes the
/// callers' special cases already dispatched (zero, non-finite, a
/// coefficient past `u128`) — defensive, never reachable from the
/// seam.
fn stripped_magnitude<F: DecimalFormat>(x: F) -> Option<(u128, i32)> {
    let (coef, exp, _) = x.to_extended_parts()?;
    if coef.is_zero() {
        return None;
    }
    let (c, e) = crate::exact::strip_trailing_zeros(coef, exp);
    if c.hi != 0 {
        return None;
    }
    Some((c.lo, e))
}

/// `rSqrt`: `y = 1/√x` with `x = c · 10^u > 0` stripped. ADR-0060's
/// one-compare relation, transcribed: with `b = C · 10^E`,
///
/// > `sign(y − b) = sign(y² − b²) = sign(1 − c·C² · 10^(u + 2E))`,
///
/// the first step because both sides are positive and squaring is
/// strictly monotone there, the second after multiplying through by
/// the positive `c · 10^u`. One `U1024` compare against 1
/// (`c·C² ≤ 10^104`, the ADR's `U384`-class width). No operand range
/// gate: the floor is uniform, so the whole domain adjudicates.
///
/// `Equal` would mean `1/√x` rational, which
/// [`crate::exact::rsqrt_exact_input`] classifies completely input
/// side (its bail proofs): the panic path, per the module doc.
pub(crate) fn rsqrt_side<F: DecimalFormat>(x: F, b: Boundary) -> Option<Side> {
    let (c, u) = stripped_magnitude(x)?;
    let rhs = U1024::from_u128(b.coef)
        .checked_mul(U1024::from_u128(b.coef))?
        .checked_mul(U1024::from_u128(c))?;
    let e2 = i64::from(u) + 2 * i64::from(b.exp);
    Some(side_of(
        cmp_aligned(U1024::from_u128(1), 0, rhs, e2),
        "rsqrt",
    ))
}

/// `hypot`: `y = √S · 10^q` on the kernel band's aligned integer
/// `S = A² + B²` (the same `S` the classifier
/// [`crate::exact::hypot_exact_or_tie`] tests for squareness — built
/// by the shared [`crate::exact::hypot_aligned_sum`], so the two
/// cannot drift). With `b = C · 10^E`:
///
/// > `sign(y − b) = sign(y² − b²) = sign(S · 10^2q − C² · 10^2E)`,
///
/// squaring monotone on positives again; the boundary kinds need no
/// separate formulas because `Boundary` carries a midpoint as its own
/// exact `(coef, exp)` pair. `S < 2·10^170` at Decimal128 (the ADR's
/// `U768` width row), `C² ≤ 10^70`. No operand range gate: the whole
/// band adjudicates. The `None` on the aligned-sum's defensive shift
/// bails is unreachable from the gated caller (its rustdoc carries
/// the band premise).
///
/// `Equal` would mean `S` a perfect square with `√S · 10^q` on a
/// boundary, which the classifier delivers input side: the panic
/// path.
pub(crate) fn hypot_side<F: DecimalFormat>(
    cw: U256,
    qw: i32,
    cz: U256,
    qz: i32,
    b: Boundary,
) -> Option<Side> {
    let (s, q) = crate::exact::hypot_aligned_sum::<F>(cw, qw, cz, qz)?;
    let lhs = U1024::from_u768(s);
    let rhs = U1024::from_u128(b.coef).checked_mul(U1024::from_u128(b.coef))?;
    Some(side_of(
        cmp_aligned(lhs, 2 * i64::from(q), rhs, 2 * i64::from(b.exp)),
        "hypot",
    ))
}

/// `pown` on the powering arm: `y = |x|^n` with `|x| = a · 10^u`
/// stripped, `2 ≤ |n| ≤ 6` (the adjudicable range; the tier's
/// operand table). With `b = C · 10^E` and `q = |n|`:
///
/// * `n > 0`: `sign(y − b) = sign(a^n · 10^(u·n) − C · 10^E)` — the
///   value is the explicit rational itself (Engine A).
/// * `n < 0`: `y = 10^(−u·q) / a^q`, so multiplying through by the
///   positive `a^q · 10^(u·q)` gives
///   `sign(y − b) = sign(1 − C·a^q · 10^(E + u·q))`.
///
/// Widths: `a^6 ≤ 10^204` (`U768` class), `C·a^6 ≤ 10^239` — the
/// `U1024`-only row of ADR-0060's table, which is exactly what put
/// `n = −6` inside the unconditional tier once `U1024` landed.
/// Outside `2 ≤ |n| ≤ 6`: `None` (the `exp(n·ln|x|)` arm's operands
/// carry the Tier 1/Tier 2 claims instead).
///
/// `Equal` would mean `x^n` terminating on a boundary, which
/// [`crate::exact::powi_exact_input`] classifies completely input
/// side: the panic path.
pub(crate) fn powi_side<F: DecimalFormat>(x: F, n: i32, b: Boundary) -> Option<Side> {
    let q = n.unsigned_abs();
    if !(2..=6).contains(&q) {
        return None;
    }
    let (a, u) = stripped_magnitude(x)?;
    let a_pow = pow_fold(U1024::from_u128(a), q)?;
    let ord = if n > 0 {
        cmp_aligned(
            a_pow,
            i64::from(u) * i64::from(q),
            U1024::from_u128(b.coef),
            i64::from(b.exp),
        )
    } else {
        let rhs = U1024::from_u128(b.coef).checked_mul(a_pow)?;
        let e2 = i64::from(b.exp) + i64::from(u) * i64::from(q);
        cmp_aligned(U1024::from_u128(1), 0, rhs, e2)
    };
    Some(side_of(ord, "powi"))
}

/// `rootn`: `y = |x|^(1/n)` with `|x| = a · 10^u` stripped,
/// `3 ≤ |n| ≤ 6` (the adjudicable range of THIS kernel: `|n| ≤ 2`
/// never reaches it — `n = ±1` are the identity and the reciprocal,
/// `n = 2` is the format's own square root, and `n = −2` delegates to
/// the `rsqrt` kernel, whose delivery adjudicates through
/// [`rsqrt_side`]; the tier language's `2 ≤ |n| ≤ 6` counts those
/// delegations). With `b = C · 10^E` and `q = |n|`, the `q`-th power
/// is strictly monotone on positives (Engine B's conjugate step needs
/// no explicit denominator for a *sign*):
///
/// * `n > 0`: `y^q = a · 10^u`, so
///   `sign(y − b) = sign(a · 10^u − C^q · 10^(q·E))`.
/// * `n < 0`: `y^q = 1 / (a · 10^u)`, so multiplying through by the
///   positive `a · 10^u` gives
///   `sign(y − b) = sign(1 − a·C^q · 10^(u + q·E))`.
///
/// Widths: `C^6 ≤ 10^210`, `a·C^6 ≤ 10^244` — the two `U1024`-only
/// rows of ADR-0060's table (`D(6)`), which is what put `|n| = 6`
/// inside the unconditional tier once `U1024` landed. Outside
/// `3 ≤ |n| ≤ 6`: `None`.
///
/// `Equal` would mean `x^(1/n)` rational on a boundary, which
/// [`crate::exact::rootn_exact_input`] classifies completely input
/// side: the panic path.
pub(crate) fn rootn_side<F: DecimalFormat>(x: F, n: i32, b: Boundary) -> Option<Side> {
    let q = n.unsigned_abs();
    if !(3..=6).contains(&q) {
        return None;
    }
    let (a, u) = stripped_magnitude(x)?;
    let c_pow = pow_fold(U1024::from_u128(b.coef), q)?;
    let ord = if n > 0 {
        cmp_aligned(
            U1024::from_u128(a),
            i64::from(u),
            c_pow,
            i64::from(q) * i64::from(b.exp),
        )
    } else {
        let rhs = c_pow.checked_mul(U1024::from_u128(a))?;
        let e2 = i64::from(u) + i64::from(q) * i64::from(b.exp);
        cmp_aligned(U1024::from_u128(1), 0, rhs, e2)
    };
    Some(side_of(ord, "rootn"))
}

/// `compound`: `y = (1 + x)^n` on the exact rational
/// `1 + x = N' · 10^s` (`N'` stripped), the D3 classifier's own
/// construction (`crate::exact::compound_exact_parts`: `N = 10^d ± c
/// · 10^e` over the stripped operand, here at `U768` width so the
/// full adjudicable range fits). Adjudicable range: `|n| · w ≤ 196`
/// with `w = digits(N')` — ADR-0060's tier row, covering every
/// realistic call. With `b = C · 10^E` and `q = |n|`:
///
/// * `n > 0`: `y = N'^n · 10^(s·n)` (Engine A, the value itself), so
///   `sign(y − b) = sign(N'^n · 10^(s·n) − C · 10^E)`.
/// * `n < 0`: `y = 10^(−s·q) / N'^q`, so
///   `sign(y − b) = sign(1 − C·N'^q · 10^(E + s·q))`.
///
/// Widths: `N'^q ≤ 10^196` by the range gate itself, `C·N'^q ≤
/// 10^231`. The build gates (`d ≤ 198`, `digits(c) + e ≤ 198`) lose
/// no in-range case: past either, `w ≥ 197 > 196 ≥ |n|·w` is already
/// out of range for every `n ≠ 0`.
///
/// Domain notes, mirroring the classifier: `x = 0` and `n = 0` are
/// special-cased before any kernel; a negative `x` with `u ≥ 0`
/// means `x ≤ −1`, dispatched as NaN/zero input side — all three
/// return a defensive `None`.
///
/// `Equal` would mean `(1+x)^n` terminating on a boundary, which
/// [`crate::exact::compound_exact_input`] classifies completely input
/// side (including both whole-range families): the panic path.
pub(crate) fn compound_side<F: DecimalFormat>(x: F, n: i32, b: Boundary) -> Option<Side> {
    use ferrodec_multiword::U768;
    if n == 0 {
        return None;
    }
    let (coef, exp, sign) = x.to_extended_parts()?;
    if coef.is_zero() || coef.hi != 0 {
        return None;
    }
    let (c, u) = crate::exact::strip_trailing_zeros(coef, exp);
    if sign && u >= 0 {
        return None; // x ≤ −1: dispatched input side
    }

    // The exact sum 1 + x = N / 10^d (or N · 10^0 with the integer
    // scale folded in on the u > 0 side), at U768 width.
    let d = if u < 0 { u.unsigned_abs() } else { 0 };
    let e = if u > 0 { u.unsigned_abs() } else { 0 };
    let c_digits = c.decimal_digit_count();
    if d > 198 || c_digits + e > 198 {
        return None; // w ≥ 197: out of the adjudicable range
    }
    if sign && c_digits > d {
        // `c ≥ 10^d` would mean `|x| ≥ 1` on the negative side, i.e.
        // `x ≤ −1`, dispatched input side; the guard keeps the
        // subtraction below from wrapping if that premise ever broke.
        return None;
    }
    let pow10_d = U768::from_u128(1).mul_pow10(d);
    let c768 = U768::from_u128(c.lo);
    let n_sum = if sign {
        pow10_d.sub(c768)
    } else {
        pow10_d.add(c768.mul_pow10(e))
    };
    // Strip: the sum's trailing zeros fold into the scale (the u = 0
    // integer case, e.g. 1 + 9 = 10, is the only source — the d > 0
    // and e > 0 shapes end in a nonzero digit by coprimality). The
    // helper is the classifier's own (`crate::exact`), shared so the
    // two canonicalisations cannot drift.
    let (n_stripped, t) = crate::exact::strip_trailing_zeros_u768(n_sum);
    let s = i64::from(t) - i64::from(d);
    let w = n_stripped.decimal_digit_count();

    let q = n.unsigned_abs();
    if u64::from(q) * u64::from(w) > 196 {
        return None; // the ADR-0060 tier row's range gate
    }

    let n_pow = pow_fold(U1024::from_u768(n_stripped), q)?;
    let ord = if n > 0 {
        cmp_aligned(
            n_pow,
            s * i64::from(q),
            U1024::from_u128(b.coef),
            i64::from(b.exp),
        )
    } else {
        let rhs = U1024::from_u128(b.coef).checked_mul(n_pow)?;
        let e2 = i64::from(b.exp) + s * i64::from(q);
        cmp_aligned(U1024::from_u128(1), 0, rhs, e2)
    };
    Some(side_of(ord, "compound"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ladder::BoundaryKind;
    use crate::mock_format::ValueFmt128;

    fn v(coef: u128, exp: i32) -> ValueFmt128 {
        ValueFmt128 {
            coef,
            exp,
            sign: false,
        }
    }

    fn grid(coef: u128, exp: i32) -> Boundary {
        Boundary {
            coef,
            exp,
            kind: BoundaryKind::Grid,
        }
    }

    fn midpoint(coef: u128, exp: i32) -> Boundary {
        Boundary {
            coef,
            exp,
            kind: BoundaryKind::Midpoint,
        }
    }

    #[test]
    fn cmp_aligned_decades_and_ties() {
        let one = U1024::from_u128(1);
        let m = U1024::from_u128(225);
        // Separated decades decide without a shift.
        assert_eq!(cmp_aligned(one, 5, m, 0), Ordering::Greater);
        assert_eq!(cmp_aligned(m, -10, one, 0), Ordering::Less);
        // Same decade, aligned exactly: 225·10^-2 vs 2250000·10^-6.
        assert_eq!(
            cmp_aligned(m, -2, U1024::from_u128(2_250_000), -6),
            Ordering::Equal
        );
        assert_eq!(
            cmp_aligned(m, -2, U1024::from_u128(2_250_001), -6),
            Ordering::Less
        );
        assert_eq!(
            cmp_aligned(m, -2, U1024::from_u128(2_249_999), -6),
            Ordering::Greater
        );
    }

    /// `rsqrt(2) = 0.70710678118…`: the true value sits strictly
    /// between the 7-digit grid points 0.7071067 and 0.7071068, and
    /// the decider must say which side of each — and of the midpoint —
    /// it is on. (The decider never reads `F::PRECISION`; the boundary
    /// itself carries the format, so a 7-digit boundary exercises the
    /// same code path every format uses.)
    #[test]
    fn rsqrt_side_brackets_root_half() {
        let x = v(2, 0);
        assert_eq!(rsqrt_side(x, grid(7_071_067, -7)), Some(Side::Above));
        assert_eq!(rsqrt_side(x, grid(7_071_068, -7)), Some(Side::Below));
        // Midpoint 0.70710675: 1/√2 = 0.707106781… sits above it.
        assert_eq!(rsqrt_side(x, midpoint(70_710_675, -8)), Some(Side::Above));
    }

    /// `rsqrt(4) = 0.5` exactly IS the boundary: the classifier owns
    /// that input, so reaching the decider with it must panic (the
    /// completeness posture, module doc).
    #[test]
    #[should_panic(expected = "adjudicator (rsqrt)")]
    fn rsqrt_side_panics_on_an_exact_boundary() {
        let _ = rsqrt_side(v(4, 0), grid(5_000_000, -7));
    }

    /// The planted `S = k² + 1` family at `k = 10^16`, Decimal128
    /// shape: `y = √(10^32 + 1) = 10^16 + 5·10^−17 − ε` sits between
    /// the grid points four and five ulps above `10^16`. In `y²` the
    /// three boundaries differ from `S` by `−0.2`, `+25·10^−34`, and
    /// `−1` scaled — the near-attaining sharpness the family was
    /// planted for, decided exactly.
    #[test]
    fn hypot_side_decides_the_k_squared_plus_one_family() {
        let k = U256::from_u128(10u128.pow(16));
        let one = U256::from_u128(1);
        let c0 = 10u128.pow(33);
        // Grid at 10^16 + 4·10^−17: y² − b² = +0.2·10^−16… > 0.
        assert_eq!(
            hypot_side::<ValueFmt128>(k, 0, one, 0, grid(c0 + 4, -17)),
            Some(Side::Above)
        );
        // Grid at 10^16 + 5·10^−17: y² − b² = −25·10^−34 < 0.
        assert_eq!(
            hypot_side::<ValueFmt128>(k, 0, one, 0, grid(c0 + 5, -17)),
            Some(Side::Below)
        );
        // The base grid point 10^16 itself.
        assert_eq!(
            hypot_side::<ValueFmt128>(k, 0, one, 0, grid(c0, -17)),
            Some(Side::Above)
        );
    }

    /// The planted `S = k² + k` family at `k = m²`, `m = 5·10^16`:
    /// `y = m·√(m² + 1)` hugs the midpoint `m² + ½` from below by
    /// `1/(8m²) ≈ 5·10^−35` relative — the closest constructed
    /// approach to the ADR-0060 floor, decided exactly.
    #[test]
    fn hypot_side_decides_the_k_squared_plus_k_family() {
        let m = 5 * 10u128.pow(16);
        let m_sq = m * m; // 2.5·10^33, a 34-digit grid point
        let cm = U256::from_u128(m);
        let cmsq = U256::from_u128(m_sq);
        assert_eq!(
            hypot_side::<ValueFmt128>(cmsq, 0, cm, 0, midpoint(10 * m_sq + 5, -1)),
            Some(Side::Below)
        );
        assert_eq!(
            hypot_side::<ValueFmt128>(cmsq, 0, cm, 0, grid(m_sq, 0)),
            Some(Side::Above)
        );
    }

    /// `1.234567² = 1.524155677489`: bracketed at the 7-digit grid,
    /// both signs of `n`, plus the range gate on `|n|`.
    #[test]
    fn powi_side_brackets_and_gates() {
        let x = v(1_234_567, -6);
        assert_eq!(powi_side(x, 2, grid(1_524_155, -6)), Some(Side::Above));
        assert_eq!(powi_side(x, 2, grid(1_524_156, -6)), Some(Side::Below));
        assert_eq!(powi_side(x, 2, midpoint(15_241_555, -7)), Some(Side::Above));
        // n = −2: 1/1.524155677489 = 0.656100958… (pinned by exact
        // cross-multiplication: 6561009 · 1524155677489 < 10^19 <
        // 6561010 · 1524155677489).
        assert_eq!(powi_side(x, -2, grid(6_561_009, -7)), Some(Side::Above));
        assert_eq!(powi_side(x, -2, grid(6_561_010, -7)), Some(Side::Below));
        // The adjudicable range is 2 ≤ |n| ≤ 6.
        assert_eq!(powi_side(x, 7, grid(1, 0)), None);
        assert_eq!(powi_side(x, -7, grid(1, 0)), None);
        assert_eq!(powi_side(x, 1, grid(1, 0)), None);
    }

    /// `1.5² = 2.25` exactly IS the boundary: classifier territory,
    /// so the decider panics.
    #[test]
    #[should_panic(expected = "adjudicator (powi)")]
    fn powi_side_panics_on_an_exact_boundary() {
        let _ = powi_side(v(15, -1), 2, grid(2_250_000, -6));
    }

    /// `2^(1/3) = 1.25992104989…` and `2^(−1/3) = 0.79370052598…`:
    /// bracketed at the 7-digit grid, plus the range gate (this
    /// kernel's own range is 3 ≤ |n| ≤ 6; `|n| ≤ 2` delegates before
    /// it).
    #[test]
    fn rootn_side_brackets_and_gates() {
        let x = v(2, 0);
        assert_eq!(rootn_side(x, 3, grid(1_259_921, -6)), Some(Side::Above));
        assert_eq!(rootn_side(x, 3, grid(1_259_922, -6)), Some(Side::Below));
        // Midpoint 0.79370055 sits above 2^(−1/3).
        assert_eq!(
            rootn_side(x, -3, midpoint(79_370_055, -8)),
            Some(Side::Below)
        );
        assert_eq!(rootn_side(x, -3, grid(7_937_005, -7)), Some(Side::Above));
        assert_eq!(rootn_side(x, 2, grid(1, 0)), None);
        assert_eq!(rootn_side(x, -2, grid(1, 0)), None);
        assert_eq!(rootn_side(x, 7, grid(1, 0)), None);
    }

    /// `compound(0.1, 9) = 1.1^9 = 2.357947691` exactly (11^9/10^9,
    /// ten digits): bracketed at the 7-digit grid on both signs of
    /// `n`, plus the `|n|·w ≤ 196` range gate at its exact edge
    /// (`w = 2` for `1 + 0.1 = 11/10`: `n = 98` adjudicates, `n = 99`
    /// does not).
    #[test]
    fn compound_side_brackets_and_gates() {
        let x = v(1, -1);
        assert_eq!(compound_side(x, 9, grid(2_357_947, -6)), Some(Side::Above));
        assert_eq!(compound_side(x, 9, grid(2_357_948, -6)), Some(Side::Below));
        // n = −9: 10^9/2357947691 = 0.42409761837…
        assert_eq!(compound_side(x, -9, grid(4_240_976, -7)), Some(Side::Above));
        assert_eq!(compound_side(x, -9, grid(4_240_977, -7)), Some(Side::Below));
        // The range gate at its edge: 98·2 = 196 in, 99·2 = 198 out.
        assert!(compound_side(x, 98, grid(1, 5)).is_some());
        assert_eq!(compound_side(x, 99, grid(1, 5)), None);
        assert_eq!(compound_side(x, 0, grid(1, 0)), None);
    }

    /// `compound(0.21, 2) = 1.21² = 1.4641` exactly on a 7-digit
    /// grid boundary spelling: classifier territory, so the decider
    /// panics.
    #[test]
    #[should_panic(expected = "adjudicator (compound)")]
    fn compound_side_panics_on_an_exact_boundary() {
        let _ = compound_side(v(21, -2), 2, grid(1_464_100, -6));
    }

    /// The negative operand side: `powi` and `rootn` adjudicate the
    /// working magnitude (the kernels reflect sign after delivery),
    /// so a negative `x` decides identically to its magnitude.
    #[test]
    fn deciders_read_the_magnitude() {
        let neg = ValueFmt128 {
            coef: 1_234_567,
            exp: -6,
            sign: true,
        };
        assert_eq!(powi_side(neg, 2, grid(1_524_155, -6)), Some(Side::Above));
        assert_eq!(rootn_side(neg, 3, grid(1_071_373, -6)), Some(Side::Above));
    }
}
