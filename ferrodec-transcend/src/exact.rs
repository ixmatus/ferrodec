//! Exact-result detection for `cbrt`, `pow`, `powi`, `rootn`,
//! `compound`, `rsqrt`, `hypot`, `exp2`, `log2`, `log10`, `log2p1`,
//! and `log10p1` (IEEE 754-2019 §7.5).
//!
//! ## Why
//!
//! The §9.2 kernels evaluate at 50-digit [`Extended`] precision and round
//! once at the format boundary. `exp_from_extended` (`exp.rs`) raises
//! `INEXACT` unconditionally, because the extended-precision approximation
//! of a transcendental value almost never lands exactly and its own
//! round-step inexact bit reflects "rounded 50 digits down to the format
//! width", not "the true result differs from the delivered one". For
//! `exp`, `ln`, and the trig and hyperbolic families that is correct:
//! their values at non-special representable inputs are irrational
//! (Lindemann for the base-`e` family at rational arguments), and the
//! special inputs short-circuit. Fifteen functions are the exceptions:
//! `exp2(n)` with `2^n` in reach of the width gate, `log2(2^k)`,
//! `log10(10^k)` (fd-aqs.8), `log2p1(2^k − 1)`, `log10p1(10^k − 1)`,
//! `exp2m1(n)`'s exact-and-tie family, `exp10(n)`'s whole-range
//! integer family, and `exp10m1(n)`'s all-nines family (ADR-0059
//! Track D), a
//! perfect cube under `cbrt`, an
//! exact rational power under `pow` (the decimal Lauter–Lefèvre
//! criterion), that criterion at `y = 1/n` under `rootn` and — the
//! widest family of the fifteen, since an integer exponent makes
//! *every* result rational — collapsed at `b = 1` under `powi`, the
//! terminating reciprocal square roots under `rsqrt`, the scaled
//! Pythagorean family under `hypot`, and the exact rational
//! `(1 + x)^n` under `compound` (ADR-0059 Track D group D3). Every
//! one is decided from the *input alone* and
//! short-circuits before the kernel, which both delivers the exact
//! value at every rounding direction and keeps the flags honest —
//! §7.5 forbids `INEXACT` on exact results.
//!
//! ADR-0047 instead proved `cbrt` / `pow` exactness *post-hoc* from
//! the rounded result. That proof was circular — it could only
//! recognise an exact value the kernel had already delivered exactly,
//! leaning on the very correct-rounding claim under repair in
//! ADR-0059 — and it failed in production on the directed modes
//! (`cbrt(0.027)` and `pow(4, 0.5)` at `TowardZero` shipped one-ulp-
//! low values with spurious `INEXACT`). The retired predicates
//! survive under `#[cfg(test)]` as independent witnesses that the
//! input-side decisions are cross-checked against.
//!
//! Classification widened to ties (ADR-0059 M7): a nearest-mode tie is
//! exactly "expressible at `PRECISION + 1` digits with final digit 5",
//! a value the approximation kernel can never resolve (the true result
//! IS a rounding boundary, so the kernel's error picks an arbitrary
//! side). The width gates therefore admit `PRECISION + 1` digits and
//! [`pack_value`] hands the exact coefficient to the format rounder,
//! whose own tie rule, directed-mode sides, and `INEXACT` accounting
//! are correct by construction on an exact input.
//!
//! ## Soundness (the invariant the whole module is built around)
//!
//! Each classifier carries a two-sided obligation. Value soundness: a
//! `Some` must be the infinitely precise result's exact coefficient
//! and exponent — [`pack_value`] then makes every mode's answer and
//! flag correct by construction, so a wrong `Some` would ship a wrong
//! value, and the width and range gates are chosen so that cannot
//! happen. Completeness: every `None` must be *provably* neither
//! exact nor a nearest-mode tie, because the kernel raises `INEXACT`
//! unconditionally past the classifier and the escalation ladder
//! (ADR-0059) assumes every remaining input sits a finite distance
//! from its rounding boundary. Each bail site documents its proof.
//!
//! `no_std`, alloc-free: fixed-width [`U256`] / `U384` integer math only.

use crate::extended::u256_mul_u256;
use crate::format::DecimalFormat;
use core::cmp::Ordering;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::u768::u384_mul_u384_to_u768;
use ferrodec_multiword::{U256, U384, U768};

#[cfg(test)]
use crate::extended::Extended;

/// Remove trailing decimal zeros from `(coef, exp)`, raising `exp` by one
/// for each zero stripped. Canonicalises a cohort so two values that are
/// numerically equal but stored at different quanta compare equal after
/// stripping. Bounded by the U256 digit cap (≤ 78 iterations).
pub(crate) fn strip_trailing_zeros(mut coef: U256, mut exp: i32) -> (U256, i32) {
    if coef.is_zero() {
        return (coef, exp);
    }
    loop {
        let (q, r) = coef.div_rem10();
        if r != 0 {
            break;
        }
        coef = q;
        exp += 1;
    }
    (coef, exp)
}

/// Exact `a × b` as a [`U256`], or `None` when the product would exceed
/// `U256`'s ~78-digit envelope. The combined-digit guard keeps the
/// intermediate `U384` product inside its ~115-digit capacity (so the
/// `u256_mul_u256` precondition holds), and the top-limb check then
/// rejects any product that does not fit `U256`.
fn checked_mul_u256(a: U256, b: U256) -> Option<U256> {
    if a.decimal_digit_count() + b.decimal_digit_count() > 115 {
        return None;
    }
    let prod = u256_mul_u256(a, b);
    if prod.hi != 0 {
        return None;
    }
    Some(U256 {
        lo: prod.lo,
        hi: prod.mid,
    })
}

/// `c³` as a [`U256`], or `None` if `c` is too wide to be an exact cube
/// root of a representable value. A perfect cube root of a value with at
/// most 34 significant digits has at most 12 significant digits (since
/// `(10^12)³ = 10^36` already exceeds the widest format coefficient), so
/// a wider `c` cannot be exact and bails. The retained range keeps `c³`
/// below ~36 digits, comfortably inside `U256`.
///
/// Test-only since ADR-0059 M7: the production path decides exactness
/// input-side ([`cbrt_exact_input`]); this survives as the independent
/// witness the unit tests cross-check that decision against.
#[cfg(test)]
fn cube_u256(c: U256) -> Option<U256> {
    if c.decimal_digit_count() > 12 {
        return None;
    }
    let c2 = checked_mul_u256(c, c)?;
    checked_mul_u256(c2, c)
}

/// Exact value equality of `c1 · 10^e1` and `c2 · 10^e2`. Builds two
/// [`Extended`] magnitudes directly (no rounding) and compares with
/// [`Extended::cmp`], which aligns the coefficients in `U384` before
/// comparing. Callers must keep both coefficients inside `U256`'s
/// envelope so the alignment stays inside `U384`. Test-only since
/// ADR-0059 M7 (a helper of the retired post-hoc witnesses).
#[cfg(test)]
fn value_eq(c1: U256, e1: i32, c2: U256, e2: i32) -> bool {
    let a = Extended {
        coef: c1,
        exp: e1,
        sign: false,
    };
    let b = Extended {
        coef: c2,
        exp: e2,
        sign: false,
    };
    a.cmp(b) == Ordering::Equal
}

/// `true` when `result` is the exact cube root of `x` (a perfect cube).
/// Compares magnitudes only (`cbrt` is odd; signs match by
/// construction).
///
/// Test-only since ADR-0059 M7. This was the ADR-0047 post-hoc proof,
/// and it was circular: it could only recognise an exact root the
/// kernel had *already delivered exactly*, which leans on the very
/// correct-rounding claim being proved. The failure was live, not
/// hypothetical: at `TowardZero` / `TowardNegative` the kernel's
/// 50-digit error landed `cbrt(0.027)` on `0.2999…9`, the cube-back
/// check saw a non-cube, and the wrong value shipped with a spurious
/// `INEXACT`. The production path now decides exactness from the input
/// alone ([`cbrt_exact_input`]); this predicate survives as the
/// independent test witness (delivered root cubes back to the input).
#[cfg(test)]
fn cube_is_exact<F: DecimalFormat>(result: F, x: F) -> bool {
    let (Some((cr, er, _)), Some((cx, ex, _))) =
        (result.to_extended_parts(), x.to_extended_parts())
    else {
        return false; // NaN / Inf: not an exact finite result.
    };
    if cr.is_zero() || cx.is_zero() {
        return false;
    }
    let (cr, er) = strip_trailing_zeros(cr, er);
    let (cx, ex) = strip_trailing_zeros(cx, ex);
    let Some(cr3) = cube_u256(cr) else {
        return false;
    };
    let Some(exp3) = er.checked_mul(3) else {
        return false;
    };
    value_eq(cr3, exp3, cx, ex)
}

/// Count the factor `p` (2 or 5) in `n`. Caller guarantees `n >= 1` (a
/// zero `n` would loop forever, since `0 % p == 0`).
fn factor_count(mut n: u128, p: u128) -> u32 {
    let mut c = 0;
    while n % p == 0 {
        n /= p;
        c += 1;
    }
    c
}

/// Reduce `|y| = cy · 10^ey` to a lowest-terms rational `a / b`, where
/// `cy >= 1` is already stripped of trailing zeros. Returns `None` when
/// the numerator or denominator overflows `u128`; [`pow_exact_input`]'s
/// bail proofs show no exact or tie case is lost there (its `|x| = 1`
/// pre-check is the anchor).
///
/// For `ey >= 0` the value is the integer `cy · 10^ey` over 1. For
/// `ey < 0` write `y = cy / 10^d` (`d = -ey`) and cancel the common
/// factors of 2 and 5: a stripped `cy` shares no factor of 10, so it has
/// at most one of the two, and `gcd(cy, 10^d) = 2^min(v2,d) · 5^min(v5,d)`.
fn reduce_rational(cy: u128, ey: i32) -> Option<(u128, u128)> {
    if ey >= 0 {
        let scale = 10u128.checked_pow(u32::try_from(ey).ok()?)?;
        let a = cy.checked_mul(scale)?;
        return Some((a, 1));
    }
    let d = (-ey) as u32;
    let v2 = factor_count(cy, 2).min(d);
    let v5 = factor_count(cy, 5).min(d);
    let a = cy / 2u128.pow(v2) / 5u128.pow(v5);
    let pow2 = 2u128.checked_pow(d - v2)?;
    let pow5 = 5u128.checked_pow(d - v5)?;
    let b = pow2.checked_mul(pow5)?;
    Some((a, b))
}

/// `base^exp` as a [`U256`], or `None` once an intermediate product would
/// exceed `U256`. Square-and-multiply; each multiply is bounds-checked, so
/// a base above 1 bails as soon as the running power outgrows the
/// envelope. `base = 1` stays at 1 for any `exp` (the power-of-ten cases).
fn int_pow_u256(base: U256, exp: u32) -> Option<U256> {
    let mut result = U256::from_u128(1);
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = checked_mul_u256(result, b)?;
        }
        e >>= 1;
        if e > 0 {
            b = checked_mul_u256(b, b)?;
        }
    }
    Some(result)
}

/// `true` when `prod · 10^exp` is exactly the value 1. Strips trailing
/// zeros from the product, then requires the residue to be 1 and the
/// accumulated exponent to cancel to zero. Test-only since ADR-0059 M7
/// (a helper of the retired post-hoc witnesses).
#[cfg(test)]
fn value_is_one(prod: U384, exp: i32) -> bool {
    if prod.is_zero() {
        return false;
    }
    let mut p = prod;
    let mut zeros: i64 = 0;
    loop {
        let (q, r) = p.div_rem10();
        if r != 0 {
            break;
        }
        p = q;
        zeros += 1;
    }
    p == U384::from_u128(1) && i64::from(exp) + zeros == 0
}

/// `true` when `result` is the exact value of `x^y` (an exact integer or
/// rational power). Writes `|y| = a / b` in lowest terms and checks the
/// relation in exact fixed-width integer arithmetic on the canonical
/// coefficients: for `y > 0`, `|result|^b == |x|^a`; for `y < 0`,
/// `|result|^b · |x|^a == 1` (since `result = x^{-a/b}`). Magnitudes
/// only; the kernel owns the odd-integer sign.
///
/// Test-only since ADR-0059 M7. This was the ADR-0047 post-hoc proof,
/// and it was circular: it could only recognise an exact power the
/// kernel had *already delivered exactly*, which leans on the very
/// correct-rounding claim being proved. The failure was live: at
/// `TowardZero` / `TowardNegative` the kernel's 50-digit error landed
/// `pow(4, 0.5)` on `1.999…9`, the power-back check saw a non-power,
/// and the wrong value shipped with a spurious `INEXACT`. The
/// production path now decides exactness from the input alone
/// ([`pow_exact_input`]); this predicate survives as the independent
/// test witness (delivered power raised back reproduces the input).
#[cfg(test)]
fn power_is_exact<F: DecimalFormat>(result: F, x: F, y: F) -> bool {
    let (Some((cr, er, _)), Some((cx, ex, _)), Some((cy, ey, y_neg))) = (
        result.to_extended_parts(),
        x.to_extended_parts(),
        y.to_extended_parts(),
    ) else {
        return false; // NaN / Inf: not an exact finite result.
    };
    if cr.is_zero() || cx.is_zero() || cy.is_zero() {
        return false;
    }
    let (cr, er) = strip_trailing_zeros(cr, er);
    let (cx, ex) = strip_trailing_zeros(cx, ex);
    let (cy, ey) = strip_trailing_zeros(cy, ey);
    // A representable exponent coefficient fits u128; bail defensively.
    if cy.hi != 0 {
        return false;
    }
    let Some((a, b)) = reduce_rational(cy.lo, ey) else {
        return false;
    };
    let Ok(a32) = u32::try_from(a) else {
        return false;
    };
    let Ok(b32) = u32::try_from(b) else {
        return false;
    };
    let Some(p) = int_pow_u256(cr, b32) else {
        return false;
    };
    let Some(q) = int_pow_u256(cx, a32) else {
        return false;
    };
    // |result|^b carries exponent er·b; |x|^a carries ex·a.
    let Ok(ep) = i32::try_from(i64::from(er) * i64::from(b32)) else {
        return false;
    };
    let Ok(eq) = i32::try_from(i64::from(ex) * i64::from(a32)) else {
        return false;
    };
    if y_neg {
        // result = x^{-a/b}, so result^b · x^a must equal 1. Keep the
        // product inside U384.
        if p.decimal_digit_count() + q.decimal_digit_count() > 115 {
            return false;
        }
        let Some(total) = ep.checked_add(eq) else {
            return false;
        };
        value_is_one(u256_mul_u256(p, q), total)
    } else {
        // result = x^{a/b}, so result^b == x^a.
        value_eq(p, ep, q, eq)
    }
}

// ----------------------------------------------------------------------------
// Input-side classification (fd-aqs.8 for `exp2` / `log2` / `log10`;
// ADR-0059 M7 for `cbrt` / `pow` and the tie widening).
//
// Every boundary case is detected from the *input alone*, before any
// approximation runs: `exp2(n)` for integer `n` with `2^n` inside the
// width gate, `log10(x)` iff `x = 10^k`, `log2(x)` iff `x = 2^k`,
// `cbrt(x)` iff `x` is a perfect cube, `pow(x, y)` iff `x^y` is an
// exact rational of bounded width (the decimal Lauter–Lefèvre
// criterion below), `powi(x, n)` iff the always-rational `x^n` is
// narrow enough (that criterion at `b = 1`, where it stops being a
// rationality test and becomes a width test).
// Delivery through [`pack_value`] then yields the
// exact value and clean `OK` for representable results, and the
// correctly resolved rounding — tie rule included — for
// `PRECISION + 1`-digit results, in one move.
//
// Two-sided obligation: a classifier must never claim a value it
// cannot prove (value soundness — a wrong `Some` would deliver a
// wrong result), and every `None` must be provably neither exact nor
// a tie (completeness — the kernel's unconditional `INEXACT` and the
// escalation ladder's "not a boundary" assumption both lean on it).
// Each bail site carries its proof.

/// Deliver an exactly known value `coef · 10^exp` with `sign` through
/// the format rounder. Caller guarantees `coef` is the value's exact,
/// complete coefficient (no digits beyond it, so `pre_sticky = false`)
/// of at most `F::PRECISION + 1` significant digits. The rounder then
/// decides everything §7 asks for: a coefficient within the format
/// precision packs exactly (`OK`, moved toward the §6.3 preferred
/// quantum 0), while a `PRECISION + 1`-digit coefficient rounds with
/// its final digit as the round digit and an empty sticky — resolving
/// a nearest-mode tie (final digit 5, nothing behind it) by the mode's
/// own tie rule, landing every directed mode on the correct side, and
/// raising `INEXACT` exactly when a nonzero digit drops (ADR-0059 M7).
fn pack_value<F: DecimalFormat>(coef: U256, exp: i32, sign: bool, rm: RoundingMode) -> (F, Status) {
    F::round_and_pack_finite(coef, exp, 0, sign, false, rm, Status::OK)
}

/// Decode `x` as a small signed integer, or `None` if it is not an
/// integer or its magnitude exceeds `limit`. Caller has filtered the
/// non-finite and zero classes.
fn as_small_int<F: DecimalFormat>(x: F, limit: u128) -> Option<(u128, bool)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    let (c, e) = strip_trailing_zeros(coef, exp);
    // After stripping, a residual negative exponent means fractional
    // digits remain: not an integer.
    if e < 0 || c.hi != 0 {
        return None;
    }
    // `n = c · 10^e`; anything past `limit` cannot be exact, so coarse
    // bails are fine. The stripped-exponent bail admits every integer
    // up to `10^6 − 1`, which [`exp10_integer`]'s five-digit decode
    // window needs (ADR-0059 Track D). Behaviour for the earlier
    // callers is unchanged: each passes `limit ≤ 200`, and any `n` the
    // wider exponent admits is at least `10^4`, so the `n > limit`
    // check below rejects it exactly as the narrower bail did. The
    // decode set of every caller is therefore the same set as before.
    if e > 5 || c.lo > limit {
        return None;
    }
    let n = c.lo.checked_mul(10u128.checked_pow(e as u32)?)?;
    if n > limit {
        return None;
    }
    Some((n, sign))
}

/// The exact or tie value of `exp2(x)` when `x` is an integer `n`
/// with `2^n` expressible in at most `F::PRECISION + 1` significant
/// digits; `None` routes to the kernel.
///
/// ## Classification completeness (integer `x` is the whole story)
///
/// If `2^x` is exactly representable, or sits exactly on a nearest
/// mode midpoint of adjacent representable values (a tie), that value
/// `v` is a terminating decimal, hence rational. A representable `x`
/// is rational, `x = a/b` in lowest terms, and `2^a = v^b`; unique
/// factorization forces every prime factor of `v` to be 2, so
/// `v = 2^k` and `a = k·b`, hence `b = 1`: `x` is an integer (the
/// standard rational-power argument; Niven, *Irrational Numbers* —
/// docs/references/niven-irrational-numbers.md). A midpoint's
/// stripped coefficient has at most
/// `PRECISION + 1` digits and ends in 5, so the width gate below
/// admits every tie; a wider `2^n` is neither representable nor a
/// midpoint, and the kernel's unconditional `INEXACT` stays correct
/// in every mode.
///
/// The ties are real, not hypothetical: `5^n` always ends in 5, so
/// `exp2(-n)` with `5^n` exactly `PRECISION + 1` digits wide IS a
/// nearest-mode midpoint — `exp2(-49)` and `exp2(-50)` at
/// `Decimal128`, `exp2(-23)` and `exp2(-24)` at `Decimal64`,
/// `exp2(-11)` at `Decimal32`. The approximation kernel cannot
/// resolve a value that is itself a rounding boundary (its error
/// lands on an arbitrary side of the midpoint; before this
/// classification, `exp2(-49)` misrounded at `NearestAway` and
/// `exp2(-50)` at `NearestEven`). Delivering the exact coefficient
/// through the format rounder is the mechanism that is correct by
/// construction (ADR-0059, tripod leg 1).
pub(crate) fn exp2_exact_or_tie<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (n, neg) = as_small_int(x, 127)?;
    let n32 = n as u32;
    if neg {
        // `2^{-n} = 5^n · 10^{-n}`. `5^55` is the last power of five
        // inside `u128`, and `5^56` has 40 digits, past every format's
        // `PRECISION + 1` (≤ 35) — so this bail loses no tie.
        if n32 > 55 {
            return None;
        }
        let p = 5u128.pow(n32);
        if U256::from_u128(p).decimal_digit_count() > F::PRECISION + 1 {
            return None;
        }
        Some(pack_value(U256::from_u128(p), -(n as i32), false, rm))
    } else {
        // `2^n` for `n ≤ 127` fits `u128`; `2^128` has 39 digits,
        // past every format's `PRECISION + 1`, so the `as_small_int`
        // limit above loses no exact or tie case either.
        let p = 2u128.checked_pow(n32)?;
        if U256::from_u128(p).decimal_digit_count() > F::PRECISION + 1 {
            return None;
        }
        Some(pack_value(U256::from_u128(p), 0, false, rm))
    }
}

/// The exact `log10(x)` when `x = 10^k`; `None` routes to the kernel.
/// Caller has filtered non-finite, zero, and negative inputs.
pub(crate) fn log10_exact<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (coef, exp, _) = x.to_extended_parts()?;
    let (c, e) = strip_trailing_zeros(coef, exp);
    // A power of ten strips to coefficient exactly 1; `k` is the
    // remaining exponent. The format exponent range keeps `|k|` well
    // inside five digits, always representable.
    if c.hi != 0 || c.lo != 1 {
        return None;
    }
    Some(pack_value(
        U256::from_u128(u128::from(e.unsigned_abs())),
        0,
        e < 0,
        rm,
    ))
}

/// The exact `log2(x)` when `x = 2^k`; `None` routes to the kernel.
/// Caller has filtered non-finite, zero, and negative inputs.
pub(crate) fn log2_exact<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (coef, exp, _) = x.to_extended_parts()?;
    let (c, e) = strip_trailing_zeros(coef, exp);
    if c.hi != 0 {
        return None;
    }
    let k: i32 = if e >= 0 {
        // Integer `x = c · 10^e` must be a power of two. A
        // representable power of two is at most `2^112` (34 digits),
        // so `e ≤ 38` keeps the multiply inside `u128` with room.
        if e > 38 {
            return None;
        }
        let n = c.lo.checked_mul(10u128.checked_pow(e as u32)?)?;
        if n == 0 || (n & (n - 1)) != 0 {
            return None;
        }
        i32::try_from(n.trailing_zeros()).ok()?
    } else {
        // `x = c · 10^e` with no trailing zeros: `x = 2^e` iff
        // `c = 5^{-e}` exactly (then `x = 5^{-e} · 10^e = 2^e`).
        let m = e.unsigned_abs();
        if m > 55 {
            return None;
        }
        if c.lo != 5u128.pow(m) {
            return None;
        }
        e
    };
    Some(pack_value(
        U256::from_u128(u128::from(k.unsigned_abs())),
        0,
        k < 0,
        rm,
    ))
}

/// The exact `log2p1(x) = log2(1 + x)` when `1 + x` is a power of two;
/// `None` routes to the kernel. The caller
/// (`crate::ln::log2p1_kernel_body`) has already run
/// `logp1_special_cases`, so every `x` reaching here is finite,
/// nonzero, and strictly above `−1`; zeros, infinities, NaNs, and the
/// domain errors never arrive.
///
/// ## Rational values are integers (ADR-0059 Track D)
///
/// Let `log2(1+x) = a/b` in lowest terms with `x` representable, so
/// `1+x` is rational and `(1+x)^b = 2^a`. Write `1+x = p/q` in lowest
/// terms: `p^b = 2^a q^b` (taking `a ≥ 0`; the `a < 0` case mirrors
/// with `p` and `q` swapped). Any prime dividing `q` divides `p^b`,
/// contradicting `gcd(p,q) = 1`, so `q = 1` and `p^b = 2^a`; unique
/// factorization forces `p = 2^j` with `jb = a`, and `gcd(a,b) = 1`
/// then forces `b = 1`. Every rational value of `log2p1` at a
/// representable input is therefore an INTEGER `k`, with `1 + x = 2^k`.
///
/// ## The exact set
///
/// * `k ≥ 1`: `x = 2^k − 1`, an odd integer (stripped exponent 0).
///   Representable iff `2^k − 1` carries at most `PRECISION` digits:
///   `k ≤ 112` at `Decimal128`, `k ≤ 53` at `Decimal64`, `k ≤ 23` at
///   `Decimal32`.
/// * `k = 0`: `x = 0`, delivered sign preserved by
///   `logp1_special_cases` before this classifier runs, so the
///   classifier never sees it.
/// * `k = −m ≤ −1`: `x = 2^−m − 1 = −(10^m − 5^m)·10^−m`. The
///   coefficient `10^m − 5^m = 5^m(2^m − 1)` is odd, so that IS its
///   stripped form, and it carries exactly `m` digits (it lies in
///   `[10^m/2, 10^m)`). Representable iff `m ≤ PRECISION`.
///
/// ## No ties
///
/// A tie value is rational, hence an integer by the argument above. A
/// nearest mode midpoint has a stripped coefficient of exactly
/// `PRECISION + 1` digits ending in 5, so an integer midpoint carries
/// magnitude at least `10^PRECISION ≥ 10^7` (subnormal range midpoints
/// are far below 1 and cannot be integers either), while
/// `|log2p1(x)| ≤ log2(10^6146) < 21,000` at the widest format. No tie
/// exists, so the kernel's unconditional `INEXACT` on everything this
/// classifier declines is correct in every mode.
///
/// ## Bail site completeness
///
/// Every `None` below is provably neither exact nor a tie; each site
/// carries its proof.
pub(crate) fn log2p1_exact<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    let (c, e) = strip_trailing_zeros(coef, exp);
    // A format coefficient fits u128 (≤ 34 digits); bail defensively.
    if c.hi != 0 {
        return None;
    }
    let k: i32 = if sign {
        // `−1 < x < 0` in the caller's domain, so `0 < 1 + x < 1` and
        // any exact `k` is `−m ≤ −1`.
        if e >= 0 {
            // Unreachable in domain: a stripped `c ≥ 1` with `e ≥ 0`
            // gives `|x| ≥ 1`, which the special case handler already
            // sent to `−∞` or NaN. Bailing loses no exact case.
            return None;
        }
        let m = e.unsigned_abs();
        // `x = 2^−m − 1` has stripped coefficient `10^m − 5^m`, a
        // number of exactly `m` digits; a representable `x` carries at
        // most `PRECISION` significant digits, so `m > PRECISION`
        // admits no `k`.
        if m > F::PRECISION {
            return None;
        }
        // `m ≤ PRECISION ≤ 34` for every IEEE decimal format keeps
        // `10^m` well inside `u128`; the checked forms are defensive.
        let pow10 = 10u128.checked_pow(m)?;
        let pow5 = 5u128.checked_pow(m)?;
        // Stripped forms are unique, so a different coefficient at this
        // exponent names a different value: not `2^−m`.
        if c.lo != pow10 - pow5 {
            return None;
        }
        -(m as i32)
    } else {
        // `x > 0`, so `1 + x > 1` and any exact `k` is `≥ 1`.
        if e > 0 {
            // `x = c·10^e` with `e ≥ 1` is `≡ 0 (mod 10)`, so
            // `1 + x ≡ 1 (mod 10)`, while `2^k ≡ 2, 4, 8, 6 (mod 10)`
            // for every `k ≥ 1`: no `k`.
            return None;
        }
        if e < 0 {
            // A stripped coefficient at `e < 0` leaves a nonzero
            // fractional digit, so `1 + x` is a non integer above 1
            // while `2^k` is an integer for every `k ≥ 1`: no `k`.
            return None;
        }
        // `e = 0`: `x = c` and `1 + x = c + 1`, no overflow (a format
        // coefficient stays below `10^34`). Stripped forms are unique,
        // so `c + 1` failing the power of two test means `1 + x` is no
        // `2^k`.
        let n = c.lo + 1;
        if !n.is_power_of_two() {
            return None;
        }
        i32::try_from(n.trailing_zeros()).ok()?
    };
    // `k` spans `[−34, 112]` at `Decimal128` and narrows at the
    // siblings: a small integer, exactly representable in every format,
    // so `pack_value` delivers status `OK` identically in every
    // rounding mode (IEEE 754-2019 §7.5 forbids `INEXACT` here).
    Some(pack_value(
        U256::from_u128(u128::from(k.unsigned_abs())),
        0,
        k < 0,
        rm,
    ))
}

/// The exact or tie value of `exp2m1(x) = 2^x − 1` when `x` is an
/// integer `n` whose value is expressible in at most
/// `F::PRECISION + 1` significant digits; `None` routes to the
/// kernel. The caller (`crate::exp::exp2m1_kernel_body`) has already
/// run `expm1_special_cases`, so every `x` arriving here is finite
/// and nonzero.
///
/// ## Rational values force integer inputs (ADR-0059 Track D)
///
/// Suppose `2^x − 1 = r` with `r` rational and `x` representable.
/// Then `2^x = 1 + r` is rational too, and [`exp2_exact_or_tie`]'s
/// argument applies verbatim: a representable `x` is rational,
/// `x = a/b` in lowest terms, `2^a = (1 + r)^b`, and unique
/// factorization forces every prime factor of `1 + r` to be 2, hence
/// `1 + r = 2^k` with `a = kb` and so `b = 1` (Niven, *Irrational
/// Numbers* — docs/references/niven-irrational-numbers.md). Every
/// exact result and every nearest mode tie of `exp2m1` therefore
/// sits at an integer `x = n`, with value `2^n − 1`.
///
/// ## The exact set
///
/// * `n ≥ 1`: `2^n − 1` is odd, so that IS its stripped coefficient,
///   at exponent 0. Representable iff it carries at most
///   `F::PRECISION` digits: `n ≤ 112` at `Decimal128`, `n ≤ 53` at
///   `Decimal64`, `n ≤ 23` at `Decimal32`.
/// * `n = 0`: `x = ±0`, delivered `±0` sign preserved by
///   `expm1_special_cases` before this classifier runs, so the
///   classifier never sees it.
/// * `n = −m ≤ −1`: `2^−m − 1 = −(10^m − 5^m)·10^−m`. The
///   coefficient `10^m − 5^m = 5^m(2^m − 1)` is odd, so that is its
///   stripped form, and it carries exactly `m` digits (`5^m ≤ 10^m/2`
///   puts it in `[5·10^(m−1), 10^m)`). Representable iff
///   `m ≤ F::PRECISION`.
///
/// ## The ties, one per side per format
///
/// A nearest mode midpoint's stripped coefficient carries exactly
/// `PRECISION + 1` digits and ends in 5. Both sides reach that shape,
/// which is what separates `exp2m1` from the tie free `log2p1`:
///
/// * Positive side: `2^n mod 10` cycles `2, 4, 8, 6` over
///   `n ≡ 1, 2, 3, 0 (mod 4)`, so `2^n − 1` ends in 5 exactly when
///   `4 | n`. The `n` whose `2^n − 1` carries exactly
///   `PRECISION + 1` digits span a window three to four wide, holding
///   exactly one multiple of four: `n = 116` at `Decimal128`,
///   `n = 56` at `Decimal64`, `n = 24` at `Decimal32`.
/// * Negative side: `10^m` ends in 0 and `5^m` in 5, so `10^m − 5^m`
///   always ends in 5 and `m = PRECISION + 1` is a tie at every
///   format: `n = −35` at `Decimal128`, `n = −17` at `Decimal64`,
///   `n = −8` at `Decimal32`.
///
/// [`pack_value`] hands each midpoint's exact coefficient to the
/// format rounder, whose own tie rule then resolves it. No
/// approximation kernel can do that: the true value IS the rounding
/// boundary, so the kernel's error picks an arbitrary side (ADR-0059,
/// tripod leg 1). The six tie inputs are deliberately absent from the
/// sampled corpus — a certified ball around an exact midpoint never
/// becomes decisive — so `tests/transcend_exact_exp2m1.rs` and its
/// siblings carry the literal deliveries as their only witnesses.
///
/// ## Bail site completeness
///
/// Every `None` below is provably neither exact nor a tie; the proofs
/// sit at their sites. The recurring fact: both closed forms are odd,
/// so neither carries a trailing zero, and a stripped coefficient
/// wider than `PRECISION + 1` digits is neither representable nor a
/// midpoint.
pub(crate) fn exp2m1_exact_or_tie<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    // A non integer `x` is ruled out by the derivation above, and a
    // magnitude past 130 cannot be exact or a tie either: `2^131 − 1`
    // carries 40 digits and `10^131 − 5^131` carries 131, both odd,
    // both far past every format's `PRECISION + 1 ≤ 35`, and both
    // grow with `|n|`.
    let (n, neg) = as_small_int(x, 130)?;
    let n32 = n as u32;
    if neg {
        let m = n32;
        // `10^m − 5^m` carries exactly `m` digits and is odd, so an
        // `m` past `PRECISION + 1` is neither representable nor a
        // midpoint. The bail doubles as the `u128` guard: `m ≤ 35`
        // keeps `10^m` well inside the envelope.
        if m > F::PRECISION + 1 {
            return None;
        }
        let pow10 = 10u128.checked_pow(m)?;
        let pow5 = 5u128.checked_pow(m)?;
        Some(pack_value(
            U256::from_u128(pow10 - pow5),
            -(m as i32),
            true,
            rm,
        ))
    } else {
        // `2^n` fits `u128` for `n ≤ 127`; `2^128 − 1` carries 39
        // digits, past every format's `PRECISION + 1`, and the value
        // only grows with `n`, so the overflow bail loses nothing.
        let p = 2u128.checked_pow(n32)?;
        let coef = U256::from_u128(p - 1);
        // Odd, hence stripped: more than `PRECISION + 1` digits is
        // neither representable nor a midpoint.
        if coef.decimal_digit_count() > F::PRECISION + 1 {
            return None;
        }
        Some(pack_value(coef, 0, false, rm))
    }
}

/// The value of `exp10m1(x) = 10^x − 1` at an INTEGER argument,
/// delivered through the format rounder; `None` routes to the kernel.
/// Caller (`crate::exp::exp10m1_kernel_body`) has already run
/// `exp::expm1_special_cases`, so every `x` reaching here is finite
/// and nonzero.
///
/// Unlike the other classifiers in this module, this one is not only
/// about §7.5 exactness: it must decide EVERY integer the format can
/// name, because the integers past the exact family are a
/// *constructible* misround class, not a model residual. Both halves
/// are proved below.
///
/// ## Rational values sit at integer arguments (ADR-0059 Track D)
///
/// Let `10^x − 1 = r` with `r` rational and `x` representable. Then
/// `1 + r = 10^x` is rational; write `1 + r = p/q` in lowest terms and
/// `x = a/b` in lowest terms, so `(p/q)^b = 10^a` (the `a < 0` case
/// mirrors with `p` and `q` swapped). Any prime dividing `q` divides
/// `p^b`, contradicting `gcd(p, q) = 1`, so `1 + r` is an integer
/// power form; unique factorization (`10 = 2·5`) then makes the 2
/// exponent and the 5 exponent of `1 + r` each equal `a/b`, forcing
/// `b | a` and hence `b = 1`. Every exact value of `exp10m1` at a
/// representable input therefore sits at an integer `x = n`, with
/// value `10^n − 1`: the `|n|` nines pattern (`9`, `99`, `999`, …
/// above zero, `−0.9`, `−0.99`, … below it).
///
/// ## No ties, anywhere
///
/// A nearest mode midpoint's stripped coefficient ends in 5. The
/// value `10^n − 1` is all nines at every `n`, so its stripped
/// coefficient ends in 9: no integer argument lands on a tie, and no
/// non integer argument can (its value is irrational by the argument
/// above). The kernel's unconditional `INEXACT` past this classifier
/// is therefore correct in every mode.
///
/// ## The delivery, by `n` (positive side)
///
/// * `1 ≤ n ≤ PRECISION`: the `n` nines integer is representable.
///   [`pack_value`] delivers it exactly at every rounding direction
///   with no `INEXACT` (IEEE 754-2019 §7.5).
/// * `n = PRECISION + 1`: the `PRECISION + 1` nines value is the
///   whole truth about the number — there is nothing below its last
///   digit — so the empty sticky form ([`pack_value`], `pre_sticky =
///   false`) is exact knowledge, and the rounder resolves it with the
///   final nine as the round digit: `NearestEven` / `NearestAway` /
///   `TowardPositive` deliver `10^n`, `TowardZero` / `TowardNegative`
///   the `PRECISION` nines neighbor, all `INEXACT`.
/// * `n ≥ PRECISION + 2`, all the way past `emax`: the all nines
///   proxy. The rounder receives the `PRECISION + 1` digit all nines
///   coefficient at exponent `n − (PRECISION + 1)` with
///   `pre_sticky = true`.
///
///   Soundness is *total digit knowledge*, not a margin. The true
///   value's decimal expansion is exactly `n` nines, so aligning it at
///   the format's drop position gives: kept digits = the top
///   `PRECISION` nines, round digit = a nine, sticky = the OR over
///   positions `PRECISION + 2 … n`, every one of them a nine and so
///   nonzero. The proxy hands the rounder that identical triple, so
///   every mode's verdict and every flag is the true value's — the
///   §7.4 overflow disposition for `n > emax` included, since it
///   comes out of the same rounder call with no special case. Stated
///   as an interval: the proxy plus its sticky denotes the open
///   interval `(10^n − 10^(n−PRECISION−1), 10^n)`, and the true value
///   `10^n − 1` lies inside it for every `n ≥ PRECISION + 2` (the
///   gap `10^(n−PRECISION−1) − 1` is positive and strictly below the
///   interval's width).
///
/// Negative side (`n = −m`), the mirror:
///
/// * `1 ≤ m ≤ PRECISION`: `−(10^m − 1)·10^−m`, the `m` nines
///   fraction, exactly representable.
/// * `m = PRECISION + 1`: the same empty sticky delivery at exponent
///   `−(PRECISION + 1)`, sign set.
/// * `m ≥ PRECISION + 2`: the negative all nines proxy, coefficient
///   and exponent frozen at the `PRECISION + 1` nines and
///   `−(PRECISION + 1)`. Its denoted magnitude interval is
///   `(1 − 10^−(PRECISION+1), 1)`, and the true magnitude `1 − 10^−m`
///   lies inside it for every `m ≥ PRECISION + 2` (it is at least
///   `1 − 10^−(PRECISION+2)`). Both the interval and the true value
///   sit strictly between the adjacent representable magnitudes
///   `1 − 10^−PRECISION` and `1`, and above their midpoint
///   `1 − 5·10^−(PRECISION+1)`, so every mode agrees: the nearest
///   modes and `TowardNegative` deliver `−1`, `TowardZero` and
///   `TowardPositive` the `PRECISION` nines neighbor, all `INEXACT`.
///   The digit identity of the positive side holds here too (kept
///   `PRECISION` nines, round digit nine, sticky over the remaining
///   `m − PRECISION − 1` nines).
///
/// ## Why the kernel cannot be left to decide the big ones
///
/// Once `n` passes the working width, `10^n ⊖ 1` keeps every digit of
/// `10^n`: the working value lands exactly ON the format grid point
/// `1·10^n`, a distance no fixed rung grows. Rung 1 absorbs the `1`
/// from `n = 50` and rung 2 from `n = 110`, so past roughly 107 —
/// where the surviving residual falls inside rung 2's own budget — a
/// default build decides the three directed modes by the sign of its
/// own noise, across thousands of inputs per format: a constructible
/// family rather than the `10^-36` model residual (the D1 `log10p1`
/// integer-anchor lesson, inverse direction). The gap integer between
/// the overflow gate and the true overflow boundary (`n = 6145` at
/// `Decimal128`, `385` at `Decimal64`, `97` at `Decimal32`) is worse
/// still: the absorbed proxy `10^n` is past `MAX`, so it raises
/// `OVERFLOW` in the directed modes where the true value
/// `10^n − 1`, rounding to `MAX` under an unbounded exponent range,
/// must not (§7.4); and an `unbounded-ladder` build escalates until
/// the rung width passes `n` digits before the subtraction means
/// anything at all.
///
/// ## Bail site completeness
///
/// Every `None` below is covered by the caller's gates, not by
/// silence (the `u32` conversion is defensive only: the decode limit
/// on the line above already bounds `n` by 99,999):
///
/// * A non integer `x` has an irrational `10^x − 1` (the derivation
///   above), so it is neither exact nor a tie, and its true value is
///   off grid: the ladder is the right decider for it.
/// * An integer past the decode limit (`|n| > 99,999`) reaches the
///   caller's `expm1_gates` with `|u| = |n|·ln 10 > 230,258`, which
///   clears both gates at every format: the overflow gate on the
///   positive side (the widest threshold is `Decimal128`'s 14,150)
///   and the `−1` band on the negative side (threshold 120). Both
///   gated deliveries are sound there — `10^n − 1` for `n > 99,999`
///   is past `MAX` by tens of thousands of decades, and
///   `10^−m − 1` for `m > 99,999` sits inside `(−1, −1 + 10^−99999)`,
///   far inside the `−1` band's proven window.
pub(crate) fn exp10m1_integer<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    // The decode limit is the gates' floor, not a precision bound: see
    // the bail-site proof above.
    let (n, neg) = as_small_int(x, 99_999)?;
    // `n ≥ 1`: the caller filtered the zero class, and `x` is finite.
    let n32 = u32::try_from(n).ok()?;
    // `PRECISION + 1 ≤ 35`, so `10^(PRECISION+1) − 1` (the widest
    // coefficient this classifier ever packs) stays inside `u128`'s
    // ~3.4·10^38 envelope with three decades to spare.
    let p1 = F::PRECISION + 1;
    if n32 <= F::PRECISION {
        // The exact family: the `n` nines integer above zero, the `n`
        // nines fraction below it. Status `OK` in every direction.
        let coef = 10u128.pow(n32) - 1;
        let exp = if neg { -(n32 as i32) } else { 0 };
        return Some(pack_value(U256::from_u128(coef), exp, neg, rm));
    }
    // `n ≥ PRECISION + 1`: the `PRECISION + 1` nines coefficient, at
    // the exponent that places its last digit where the true value's
    // `(PRECISION + 1)`-th nine sits. The two documented regimes
    // differ only in the sticky bit — at `n = PRECISION + 1` nothing
    // lies below that digit (the [`pack_value`] form), beyond it the
    // dropped positions are all nines and so all nonzero.
    let nines_p1 = 10u128.pow(p1) - 1;
    let exp = if neg { -(p1 as i32) } else { (n32 - p1) as i32 };
    Some(F::round_and_pack_finite(
        U256::from_u128(nines_p1),
        exp,
        0,
        neg,
        n32 > p1,
        rm,
        Status::OK,
    ))
}

/// The exact `exp10(x) = 10^x` when `x` is an integer `n`, delivered
/// as the coefficient 1 at exponent `n` through the format rounder;
/// `None` routes to the kernel. The caller
/// (`crate::exp::exp10_kernel_body`) has already disposed of the
/// special classes and of `±0`, so every `x` reaching here is finite
/// and nonzero.
///
/// ## Rational values force integer inputs (ADR-0059 Track D)
///
/// Suppose `10^x = v` with `v` rational and `x` representable, so
/// `x = a/b` in lowest terms and `v^b = 10^a`. Unique factorization
/// through `10 = 2·5` makes the 2-exponent and the 5-exponent of `v`
/// each equal `a/b`; both are integers, so `b | a` and `gcd(a, b) = 1`
/// forces `b = 1`. Every exact case is therefore an integer `n` whose
/// value `10^n` has coefficient 1, exactly representable for every `n`
/// in `[etiny, emax]`: `[−6176, 6144]` at `Decimal128`, `[−398, 384]`
/// at `Decimal64`, `[−101, 96]` at `Decimal32`.
///
/// ## No ties
///
/// A nearest-mode midpoint's stripped coefficient ends in 5; a power
/// of ten's stripped coefficient is 1. No representable input's
/// `10^x` is a midpoint, so the kernel's unconditional `INEXACT` past
/// this classifier is correct in every rounding direction.
///
/// ## Every integer, in range or not (the ladder's stake)
///
/// This classifier deliberately decides *every* decoded integer rather
/// than only the representable ones, and all three regimes are load
/// bearing (the `log10p1` integer-anchor lesson of D1, run in the
/// inverse direction):
///
/// * `etiny ≤ n ≤ emax`: `10^n` is a format grid point, and a grid
///   point is a rounding boundary no finite rung separates itself
///   from. Delivered here, [`pack_value`] packs it exactly with status
///   `OK` in every mode (IEEE 754-2019 §7.5 forbids `INEXACT` on an
///   exact result).
/// * `n > emax`: the working value would land exactly ON the grid
///   point `1·10^n` past the range. The escalation predicate widens at
///   the value's own exponent and never consults `emax`, so a guarded
///   delivery would escalate at every rung, panicking the
///   `ladder_audit` lane and widening without terminating under
///   `unbounded-ladder`. The `exp` overflow gate does not cover the
///   whole family: it fires at `|n·ln 10| >` the format's
///   `exp_overflow_limit`, which leaves exactly one integer per format
///   in the gap (`n = 6145` at `Decimal128`, whose `6145·ln 10 ≈
///   14149.4` stays inside the 14150 limit; `n = 385` at `Decimal64`
///   against 887; `n = 97` at `Decimal32` against 224), and one
///   further decade trips it. Delivered here, `pack_value` applies the
///   §7.4 overflow disposition per direction (`+∞` at the nearest
///   modes and toward `+∞`, the largest finite toward zero and `−∞`)
///   with `OVERFLOW | INEXACT`.
/// * `n < etiny`: the true value `10^n ≤ 10^(etiny−1)` is a tenth of
///   the smallest subnormal, hence far below the half of it that
///   decides the nearest modes. `pack_value` hands the rounder the
///   exact coefficient 1 form, which is the true value itself with no
///   residue, so every mode's verdict is the true value's by
///   construction: `+0` at the nearest modes, toward zero and toward
///   `−∞`, the smallest subnormal toward `+∞`, with
///   `UNDERFLOW | INEXACT`.
///
/// ## Beyond the decode limit
///
/// Every integer `as_small_int` declines has `|n| > 99,999`, at each
/// of its bail sites: a stripped exponent past 5 puts `n ≥ 10^6`, a
/// stripped coefficient past the limit puts `n` past it too (the
/// exponent only raises the magnitude), and the closing magnitude
/// check is the limit itself. Those all clear the `exp` gates before a
/// working value near a grid point can form: `|n·ln 10| > 99,999 ·
/// 2.3025 > 230,000`, past both the overflow limits (14,150 / 887 /
/// 224) and the underflow limits (14,221 / 918 / 235) of all three
/// formats, so the kernel's saturation proxy answers them, unguarded
/// and correct by its own margin argument.
pub(crate) fn exp10_integer<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (n, neg) = as_small_int(x, 99_999)?;
    // `n ≤ 99,999` fits `i32` with four orders to spare; the sign of
    // the input becomes the sign of the exponent (`10^−n`), never of
    // the value, which is positive for every finite `x`.
    let exp = if neg { -(n as i32) } else { n as i32 };
    Some(pack_value(U256::from_u128(1), exp, false, rm))
}

/// `true` when the `u128` `n ≥ 1` is a power of ten. Decided from the
/// decimal digit count alone: `n` has `d` digits iff
/// `10^(d−1) ≤ n < 10^d`, so the only power of ten with `d` digits is
/// `10^(d−1)`, and equality against it is the whole test. No floats,
/// no logarithms; `d ≤ 39` for every `u128`, and every caller keeps
/// `n ≤ 10^34`, so the exponentiation cannot overflow.
fn is_power_of_ten(n: u128) -> bool {
    let d = U256::from_u128(n).decimal_digit_count();
    match 10u128.checked_pow(d - 1) {
        Some(p) => n == p,
        None => false,
    }
}

/// The exact `log10p1(x) = log10(1 + x)` when `1 + x` is a power of
/// ten; `None` routes to the kernel. Caller has run
/// `ln::logp1_special_cases` first, so `x` here is finite, nonzero,
/// and strictly above `−1`.
///
/// ## Rational values are integers (ADR-0059 Track D)
///
/// `log10(1 + x) = a/b` in lowest terms forces `(1 + x)^b = 10^a`.
/// Write `1 + x = p/q` in lowest terms: any prime dividing `q` would
/// divide `p^b` (mirrored for `a < 0`), contradicting
/// `gcd(p, q) = 1`, so `1 + x` is an integer power form, and unique
/// factorization (`10 = 2·5`) makes the 2 exponent and the 5 exponent
/// of `1 + x` each equal `a/b`, forcing `b | a` and hence `b = 1`.
/// Every rational value of `log10p1` at a representable input is
/// therefore an integer `k` with `1 + x = 10^k`, and the exact set is
/// the nines patterns:
///
/// * `k ≥ 1`: `x = 10^k − 1`, the `k` nines integer (`9`, `99`, `999`,
///   …). It ends in 9, so its stripped exponent is 0 and its stripped
///   coefficient has exactly `k` digits: representable iff
///   `k ≤ F::PRECISION`.
/// * `k = 0`: `x = 0`, disposed of by the caller's special cases; this
///   classifier never sees it.
/// * `k = −m ≤ −1`: `x = 10^−m − 1 = −(10^m − 1)·10^−m`, the `m` nines
///   fraction (`−0.9`, `−0.99`, …). Its stripped coefficient
///   `10^m − 1` has exactly `m` digits and ends in 9: representable
///   iff `m ≤ F::PRECISION`.
///
/// So `k ∈ [−F::PRECISION, F::PRECISION]`, which every format packs
/// exactly. Note the asymmetry with [`log10_exact`], whose exact
/// family spans the format's whole exponent range (`±6176` at
/// `Decimal128`): here the *input* must carry the nines, so the
/// binding constraint is the format's digit width, not its exponent
/// range.
///
/// ## No ties
///
/// A tie value is rational, hence one of the integers above; but an
/// integer nearest-mode midpoint needs a `PRECISION + 1`-digit
/// stripped coefficient ending in 5, so magnitude at least `10^7`,
/// while `|log10p1(x)| ≤ 6146` at the widest format. No tie exists,
/// and the kernel's unconditional `INEXACT` past this classifier is
/// correct in every mode.
///
/// ## Bail-site completeness proofs
///
/// Each `None` below is provably neither exact nor a tie; the proofs
/// are carried at their sites.
pub(crate) fn log10p1_exact<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if coef.is_zero() {
        return None; // zero short-circuits at the kernel
    }
    let (c, e) = strip_trailing_zeros(coef, exp);
    // A format coefficient fits u128 (≤ 34 digits); bail defensively.
    // Such a `c` is outside every format's input set, so no exact or
    // tie case is lost.
    if c.hi != 0 {
        return None;
    }
    let k: i32 = if sign {
        // `x < 0` and the caller's domain gives `x > −1`, so
        // `0 < |x| < 1`; with `c ≥ 1` that forces `e < 0` (an `e ≥ 0`
        // would make `|x| = c · 10^e ≥ 1`). A nonnegative `e` here is
        // therefore outside this classifier's domain, not a missed
        // case.
        if e >= 0 {
            return None;
        }
        let m = e.unsigned_abs();
        // `10^m − 1` has exactly `m` digits, so a format coefficient
        // (≤ F::PRECISION digits) can never equal it once
        // `m > F::PRECISION`: no exact case is lost, and the bail
        // keeps `10^m` inside `u128` (`m ≤ 34`).
        if m > F::PRECISION {
            return None;
        }
        // Stripped forms are unique, so `1 + x = 10^−m` holds iff the
        // stripped coefficient is exactly the `m` nines; any other
        // `c` leaves `1 + x` strictly between two powers of ten (it
        // lies in `(0, 1)`, which rules out `k ≥ 0` outright).
        if c.lo != 10u128.pow(m) - 1 {
            return None;
        }
        -(m as i32)
    } else {
        // `x > 0` gives `1 + x > 1`, so only `k ≥ 1` can occur, and
        // `10^k` is then an integer divisible by 10.
        if e > 0 {
            // `x = c · 10^e` is an integer divisible by 10, so
            // `1 + x ≡ 1 (mod 10)` while `10^k ≡ 0 (mod 10)` for
            // every `k ≥ 1`: no `k`.
            return None;
        }
        if e < 0 {
            // A stripped `c` shares no factor of ten, so `e < 0`
            // leaves a nonzero last fractional digit: `1 + x` is a
            // non-integer above 1, while `10^k` is an integer for
            // `k ≥ 1` and at most 1 for `k ≤ 0`: no `k`.
            return None;
        }
        // `e = 0`: `x` is the integer `c`, and `1 + x = c + 1` must be
        // a power of ten. `c ≤ 10^34 − 1`, so the sum stays in `u128`.
        let n = c.lo + 1;
        if !is_power_of_ten(n) {
            // Unique stripped forms again: `c + 1` not a power of ten
            // means `1 + x ≠ 10^k` for every `k`.
            return None;
        }
        // `n ≥ 2` (`c ≥ 1`), so `n = 10^k` with `k ≥ 1`; the digit
        // count gives `k` directly.
        i32::try_from(U256::from_u128(n).decimal_digit_count() - 1).ok()?
    };
    Some(pack_value(
        U256::from_u128(u128::from(k.unsigned_abs())),
        0,
        k < 0,
        rm,
    ))
}

/// The integer-anchor exponent of `log10p1` for a power-of-ten input:
/// `Some(n)` iff `x = 10^n` exactly with `n ≥ 36`; `None` routes to
/// the kernel. Caller has disposed of the special classes and the
/// domain (`x` finite, nonzero, `> −1`).
///
/// ## Why this family needs the ADR-0051 residual channel
///
/// `log10p1(10^n) = n + 10^−n/ln 10`: strictly above the representable
/// integer `n` by `δ < 10^−36` (for `n ≥ 36`), while the nearest
/// rounding boundary above `n` — the midpoint toward `next_up(n)` —
/// sits at least `5·10^−31` away in the widest format (a 4 digit `n`
/// at 34 digit precision) and further in the narrower ones. The true
/// value and the residual channel's denoted interval therefore lie
/// strictly between the same adjacent boundaries, so they round
/// identically in every mode: `TowardPositive` to `next_up(n)`, the
/// other four to `n`, always `INEXACT`.
///
/// The kernel cannot decide this family on its own: its wide band
/// forms `t = 1 ⊕ x`, and once `n` passes the working width the `1`
/// is absorbed, landing the working value exactly ON the grid point
/// `n` — a distance no fixed rung can grow (the sinh/cosh saturation
/// lesson in a new costume; found by the D1 review's `ladder_audit`
/// lane). Base ten is what makes it bite: `logp1`'s absorbed anchor
/// `n·ln 10` is irrational, and `log2p1`'s representable `2^k`
/// inputs stop at `k = 112`, whose separation `2^−112/ln 2` rung 1
/// resolves; only `log10p1` keeps a representable on-grid anchor
/// across the whole exponent range.
///
/// Below the threshold the kernel provably decides: for
/// `2 ≤ n ≤ 35`, `t = 1 + 10^n` is exact at every rung width and the
/// separation `δ ≥ 4.3·10^−37` clears the predicate at rung 2 at
/// worst (`n ≤ 49` is exact at rung 1's 50 digits already). `n ≥ 36`
/// overlaps that band deliberately: both deliveries are proven, and
/// the classifier keeps the whole exposed family (`n` past the rung
/// widths) plus a margin on one uniform proof.
pub(crate) fn log10p1_power_of_ten_exponent<F: DecimalFormat>(x: F) -> Option<i32> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if sign || coef.is_zero() {
        return None;
    }
    let (c, e) = strip_trailing_zeros(coef, exp);
    if c.hi != 0 || c.lo != 1 || e < 36 {
        return None;
    }
    Some(e)
}

// ----------------------------------------------------------------------------
// Input-side exactness for `cbrt` (ADR-0059 M7, replacing the ADR-0047
// post-hoc proof — see `cube_is_exact` for why that proof was circular).

/// Integer cube root witness: `Some(t)` iff `t³ == c` exactly, for
/// `c ≥ 1`. Binary search with overflow-checked cubing, so the routine
/// is total over all of `u128` even though format coefficients stay
/// within 34 digits (root ≤ 12 digits).
fn cbrt_u128(c: u128) -> Option<u128> {
    let mut lo: u128 = 1;
    let mut hi: u128 = 6_981_463_658_332; // ⌈u128::MAX^(1/3)⌉
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        match mid.checked_mul(mid).and_then(|sq| sq.checked_mul(mid)) {
            Some(cube) if cube == c => return Some(mid),
            Some(cube) if cube < c => lo = mid + 1,
            _ => hi = mid - 1,
        }
    }
    None
}

/// The exact `cbrt(x)` decided from the input alone, for a positive
/// finite nonzero `x`; `None` routes to the kernel. The caller works
/// on `|x|` and re-applies the sign (`cbrt` is odd).
///
/// ## Exactness is decidable, and decided completely
///
/// Write `|x|` in stripped form `c · 10^e` (`c` free of trailing
/// zeros); stripped forms are unique. If `cbrt(x)` is representable it
/// is some stripped `t · 10^u`, and `t³` is itself free of trailing
/// zeros (`10 | t³` forces `2 | t` and `5 | t`), so cubing produces
/// exactly the stripped form of `|x|`: `c = t³` and `e = 3u`. Both
/// conditions are decidable: `3 | e` plus an integer cube-root check.
/// When either fails, `cbrt(x)` is irrational — a rational root `n/d`
/// in lowest terms of the terminating decimal `x` forces
/// `d³ | 10^k`, so `d = 2^i·5^j` and the root itself terminates,
/// which is the caught case — and the kernel's unconditional
/// `INEXACT` is then correct.
///
/// ## No ties
///
/// A nearest-mode midpoint `h` of the result grid has a stripped
/// coefficient ending in 5. In the normal range that coefficient has
/// exactly `PRECISION + 1` digits, so `x = h³` would need at least
/// `3·PRECISION + 1` stripped digits: no representable `x` has them.
/// In the subnormal range (quantum pinned at `etiny`) the midpoint is
/// `h = c_h · 10^(etiny − 1)` with `c_h ≤ 10^(PRECISION + 1)`, so
/// `x = c_h³ · 10^(3·etiny − 3) < 10^(3·PRECISION + 3·etiny)`, and
/// `3·PRECISION + 3·etiny < etiny` for every IEEE decimal format
/// (`etiny < −3·PRECISION/2` holds with orders of magnitude to
/// spare), putting `x` below every representable magnitude. `cbrt`
/// therefore has no ties: the exact case above is the whole boundary
/// story, and a delivered root (at most `⌈PRECISION/3⌉` digits)
/// packs exactly, status `OK`, identically in every rounding mode.
pub(crate) fn cbrt_exact_input<F: DecimalFormat>(
    abs_x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef, exp, _) = abs_x.to_extended_parts()?;
    if coef.is_zero() {
        return None; // zero short-circuits at the kernel
    }
    let (c, e) = strip_trailing_zeros(coef, exp);
    if e % 3 != 0 {
        return None;
    }
    // A format coefficient fits u128 (≤ 34 digits); bail defensively.
    if c.hi != 0 {
        return None;
    }
    let t = cbrt_u128(c.lo)?;
    Some(pack_value(U256::from_u128(t), e / 3, false, rm))
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `pow` (ADR-0059 M7,
// replacing the ADR-0047 post-hoc proof — see `power_is_exact` for why
// that proof was circular).

/// Integer `b`-th root witness: `Some(s)` iff `s^b == t` exactly, for
/// `t ≥ 2` and `b ≥ 2` (the caller short-circuits `t = 1`). Binary
/// search with overflow-checked powering, total over all of `u128`.
fn nth_root_u128(t: u128, b: u32) -> Option<u128> {
    if b >= 128 {
        // `s ≥ 2` gives `s^b ≥ 2^128 > t` for every `u128` t, and
        // `s = 1` was short-circuited: no root exists.
        return None;
    }
    let mut lo: u128 = 2;
    // `s ≤ 2^(128/b)` keeps `s^b` within (checked) reach.
    let mut hi: u128 = 1u128 << (128 / b).min(127);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        match mid.checked_pow(b) {
            Some(p) if p == t => return Some(mid),
            Some(p) if p < t => lo = mid + 1,
            _ => hi = mid - 1,
        }
    }
    None
}

/// The exact or tie value of `pow(x, y)` decided from the inputs
/// alone, for positive finite nonzero `|x| ≠ 1`-or-`= 1` and finite
/// nonzero `y`; `None` routes to the kernel. The caller works on `|x|`
/// under the negation-reflected rounding mode and re-applies the
/// odd-integer sign (the fd-aqs.5 rule), exactly as the kernel does.
///
/// ## The criterion (decimal analog of Lauter–Lefèvre 2009)
///
/// Factor `|x| = 2^α · 5^β · t` with `gcd(t, 10) = 1` (from the
/// stripped `c · 10^e`: `α = v₂(c) + e`, `β = v₅(c) + e`, both
/// possibly negative) and write `|y| = a/b` in lowest terms. Then
/// `x^(a/b)` is rational iff `x` is a `b`-th power in `ℚ`, i.e. iff
///
/// > `b | α`, `b | β`, and `t = s^b` for an integer `s`,
///
/// (prime-exponent divisibility under unique factorization; `gcd(a,b)
/// = 1` transfers the divisibility from `a·v_p(x)` to `v_p(x)`), and
/// then `x^(a/b) = s^a · 2^(αa/b) · 5^(βa/b)` exactly. Failing the
/// criterion, `x^y` is irrational: neither exact nor a tie (ties
/// terminate), so the kernel's unconditional `INEXACT` is correct.
/// All three conditions are decided in bounded integer arithmetic —
/// no factoring: `v₂` / `v₅` by trial division, `t = s^b` by
/// [`nth_root_u128`]. The criterion is re-derived here for base 10;
/// Lauter and Lefèvre's binary64 analysis is the shape precedent
/// (docs/references/lauter-lefevre-pow-boundary.md, "derivation over
/// analogy").
///
/// For `y < 0` the value is `s^(-a) · 2^(-αa/b) · 5^(-βa/b)`; the
/// negative powers of 2 and 5 fold into the decimal exponent, but
/// `s^(-a)` with `s ≥ 2` is a non-terminating rational — not
/// representable and not a tie — so exactness additionally requires
/// `s = 1`.
///
/// The rational value is assembled as `coef · 10^w`: with
/// `u = αa/b`, `v = βa/b`, `w = min(u, v)`, the coefficient
/// `s^a · 2^(u−w) · 5^(v−w)` is already stripped (at most one of the
/// 2- and 5-exponents is nonzero, and `s` is coprime to 10), so the
/// `PRECISION + 1` digit gate and [`pack_value`] finish the job: an
/// exact representable value packs `OK`, a `PRECISION + 1`-digit
/// value — including the real ties, e.g. `pow(5, 49)` and
/// `pow(2, -49)` whose true value's 35-digit coefficient ends in 5 —
/// rounds with the correct tie rule, directed sides, and flags, and
/// an out-of-range exponent over/underflows through the rounder with
/// exactly the §7.4 disposition.
///
/// ## Bail-site completeness proofs
///
/// Every `None` below is provably neither exact nor a tie. The
/// recurring facts: an exact-or-tie value has a stripped coefficient
/// of at most `PRECISION + 1 ≤ 35` digits and a decimal exponent
/// within the format's few-thousand range; `|α|, |β| ≤ v₂/₅(c) + |e|
/// < 7000` for every format.
///
/// * `|x| = 1` is answered (`1` for every finite `y`) before `y` is
///   even reduced, so no later bail needs to cover it.
/// * `reduce_rational` overflow / `a > u32::MAX`: an exact-or-tie
///   case with huge `a` needs `s = 1` (else `s^a` is astronomically
///   wide). With `α = β = 0` that means `|x| = 1`, already handled;
///   with `α ≠ β` the coefficient carries `2^(|α−β|a/b)` or the
///   5-analog, forcing `a ≤ 120·b`; with `α = β ≠ 0` (`x = 10^α`)
///   the exponent `αa/b` must stay in range, forcing `a < 10^7·b`.
///   Either way `a` fits `u32` whenever `b` does.
/// * `b > u32::MAX`: `b | α` and `b | β` with `(α, β) ≠ (0, 0)`
///   bounds `b < 7000`; `α = β = 0` makes `x = t = s^b`, and
///   `t ≤ 10^34` with `s ≥ 2` bounds `b ≤ 112` (`s = 1` is
///   `|x| = 1` again).
/// * Width bails (`du`/`dv` gates, `int_pow_u256`, the final digit
///   gate): the coefficient is stripped, so more than
///   `PRECISION + 1` digits is neither representable nor a midpoint.
/// * `|w| > EXACT_EXPONENT_WINDOW` (99,999): the value
///   `coef · 10^w` has already passed the digit gate
///   (`1 ≤ coef < 10^35`), so `|y·ln|x|| = |ln(x^y)| ≥
///   99,999·ln 10 − 81 > 230,000`, past both `exp` gates (overflow
///   limits 14,150 / 887 / 224; underflow limits 14,221 / 918 / 235)
///   of all three formats: the kernel's saturation proxy answers it
///   unguarded, correct by its own margin argument. The window is a
///   *soundness* bound on what the format rounder may be handed —
///   `round_and_pack_finite`'s `qmin − unbiased_exp` is `i32`
///   arithmetic, and an exponent near `i32::MAX` wraps it: before
///   this gate, `pow(10, 2147483647)` panicked in debug builds and
///   would have shipped a wrong §7.4 disposition in release (fd-clc,
///   found by the D3 powi lane; [`EXACT_EXPONENT_WINDOW`] carries the
///   shared derivation).
pub(crate) fn pow_exact_input<F: DecimalFormat>(
    abs_x: F,
    y: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef_x, exp_x, _) = abs_x.to_extended_parts()?;
    let (coef_y, exp_y, y_neg) = y.to_extended_parts()?;
    if coef_x.is_zero() || coef_y.is_zero() {
        return None; // zeros short-circuit at the kernel
    }
    let (cx, ex) = strip_trailing_zeros(coef_x, exp_x);
    let (cy, ey) = strip_trailing_zeros(coef_y, exp_y);
    // Format coefficients fit u128 (≤ 34 digits); bail defensively.
    if cx.hi != 0 || cy.hi != 0 {
        return None;
    }
    // |x| = 1: x^y is exactly 1 for every finite y, including y too
    // wide for the rational reduction below (pow(-1, 1E+40) reaches
    // here; +1 is answered by the kernel's rule 2). This check is
    // load-bearing for the huge-a/huge-b bail proofs above.
    if cx.lo == 1 && ex == 0 {
        return Some(pack_value(U256::from_u128(1), 0, false, rm));
    }
    let (a, b) = reduce_rational(cy.lo, ey)?;
    let a32 = u32::try_from(a).ok()?;
    let b32 = u32::try_from(b).ok()?;
    // Factor |x| = 2^α · 5^β · t on the stripped parts.
    let v2 = factor_count(cx.lo, 2);
    let v5 = factor_count(cx.lo, 5);
    let t = cx.lo / 2u128.pow(v2) / 5u128.pow(v5);
    let alpha = i64::from(v2) + i64::from(ex);
    let beta = i64::from(v5) + i64::from(ex);
    // The b-th power criterion; failing it, x^y is irrational.
    let b64 = i64::from(b32);
    if alpha % b64 != 0 || beta % b64 != 0 {
        return None;
    }
    let s = if t == 1 {
        1u128
    } else if b32 == 1 {
        t
    } else {
        nth_root_u128(t, b32)?
    };
    if y_neg && s != 1 {
        // 1 / s^a is a non-terminating rational: not representable,
        // not a tie.
        return None;
    }
    // Exponents of 2 and 5 in the result. |α/b| < 7000 and a < 2^32
    // keep the products well inside i64.
    let mut u = (alpha / b64) * i64::from(a32);
    let mut v = (beta / b64) * i64::from(a32);
    if y_neg {
        u = -u;
        v = -v;
    }
    let w = u.min(v);
    let (du, dv) = (u - w, v - w); // at least one is zero
                                   // 2^128 has 39 digits and 5^56 has 40, both past every format's
                                   // PRECISION + 1 (≤ 35): wider is neither exact nor a tie.
    if du > 127 || dv > 55 {
        return None;
    }
    let s_pow_a = if y_neg {
        U256::from_u128(1)
    } else {
        int_pow_u256(U256::from_u128(s), a32)?
    };
    let pow2 = int_pow_u256(U256::from_u128(2), du as u32)?;
    let pow5 = int_pow_u256(U256::from_u128(5), dv as u32)?;
    let coef = checked_mul_u256(checked_mul_u256(s_pow_a, pow2)?, pow5)?;
    if coef.decimal_digit_count() > F::PRECISION + 1 {
        return None;
    }
    if w.unsigned_abs() > EXACT_EXPONENT_WINDOW {
        return None;
    }
    // Inside the window `w` fits `i32` with four orders to spare.
    let exp = i32::try_from(w).ok()?;
    Some(pack_value(coef, exp, false, rm))
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `rsqrt` (IEEE 754-2019
// §9.2 `rSqrt`; ADR-0059 Track D group D3, under ADR-0060's phase
// gate — the classification's completeness is a *stated premise* of
// every Liouville floor that ADR names, so this is tripod leg 1 in its
// load-bearing role rather than a §7.5 flag nicety).

/// The exact or tie value of `rsqrt(x) = 1/√x` decided from the input
/// alone; `None` routes to the kernel. The caller
/// (`crate::rsqrt::rsqrt_kernel_body`) has already run
/// `rsqrt::rsqrt_special_cases`, so every `x` reaching here is finite,
/// nonzero, and positive.
///
/// ## The criterion (ADR-0060's rSqrt derivation, transcribed)
///
/// Write `x = a · 10^u` in stripped form (`a` free of trailing zeros,
/// so `a` shares at most one of the factors 2 and 5) and factor
/// `a = 2^v₂ · 5^v₅ · s` with `gcd(s, 10) = 1`.
///
/// * **`s ≠ 1` is neither exact nor a tie.** Write `x = 2^A · 5^B · s`
///   with `A = v₂ + u`, `B = v₅ + u`. If `s` is not a perfect square,
///   `√s` is irrational (unique factorization), hence so is `1/√x`:
///   not exact, and not a tie (a tie value is rational). If `s = q²`
///   for an integer `q > 1`, then `gcd(q, 10) = 1` and
///   `1/√x = 2^(−A/2) · 5^(−B/2) / q` — rational when `A` and `B` are
///   even (`rsqrt(9) = 1/3` is the smallest case) — but its lowest
///   terms denominator carries the factor `q` coprime to ten, so its
///   decimal expansion does not terminate. Exact values and nearest
///   mode midpoints both terminate, so this case reaches no boundary
///   either: bail.
/// * **`s = 1`, and `A` or `B` odd, is irrational.** `x = 2^A · 5^B`
///   gives `1/√x = 2^(−A/2) · 5^(−B/2)`; an odd `A` leaves a factor
///   `1/√2`, an odd `B` a factor `1/√5`, and both odd a factor
///   `1/√10`. Each is irrational (unique factorization again), so the
///   product is, so no exact value and no tie.
/// * **`s = 1`, `A = 2i`, `B = 2j`.** The value is exactly
///   `2^−i · 5^−j`. Folding to a decimal: for `i ≥ j` it is
///   `5^(i−j) · 10^−i`, for `i < j` it is `2^(j−i) · 10^−j`. A stripped
///   coefficient shares at most one of the two factors, so `v₂` and
///   `v₅` are never both positive and exactly one branch carries a
///   coefficient above 1 — `i ≥ j` iff `v₅ = 0`, with `i − j = v₂/2`,
///   and `i < j` with `j − i = v₅/2`.
///
/// ## The width gate is honest
///
/// The delivered coefficient is a pure power of five or a pure power of
/// two, hence coprime to the other factor of ten, hence *already
/// stripped*. A stripped coefficient wider than `F::PRECISION + 1`
/// digits is neither representable nor a nearest mode midpoint, so
/// bailing there loses nothing and the kernel's unconditional `INEXACT`
/// stays correct.
///
/// ## The ties are real
///
/// Powers of five always end in 5, so a `5^d` of exactly
/// `PRECISION + 1` digits IS a nearest mode midpoint, and each format
/// admits one: `rsqrt(2^98) = 5^49 · 10^−49` at `Decimal128`
/// (`5^49` is 35 digits), `rsqrt(2^48) = 5^24 · 10^−24` at `Decimal64`
/// (17 digits), `rsqrt(2^22) = 5^11 · 10^−11` at `Decimal32`
/// (8 digits). Each input is representable — `2^98` is 30 digits,
/// `2^48` is 15, `2^22` is 7 — so the family is reachable, not
/// hypothetical. [`pack_value`] hands the exact coefficient to the
/// format rounder, whose own tie rule resolves it; no approximation
/// kernel can, because the true value IS the boundary (ADR-0059,
/// tripod leg 1). Powers of two end in 2, 4, 6, or 8, so the
/// `i < j` branch contributes no ties; its `PRECISION + 1`-digit
/// deliveries are still exact knowledge and round correctly through the
/// same call.
///
/// ## Bail site completeness
///
/// Every `None` below is provably neither exact nor a tie; the proofs
/// sit at their sites. Zero and the non-finite classes never arrive
/// (the caller's special cases run first), and the negative domain is a
/// NaN there rather than a classification question.
pub(crate) fn rsqrt_exact_input<F: DecimalFormat>(x: F, rm: RoundingMode) -> Option<(F, Status)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if sign || coef.is_zero() {
        // Outside this classifier's domain: the caller's §9.2.1
        // dispositions answered both classes before it ran.
        return None;
    }
    let (c, e) = strip_trailing_zeros(coef, exp);
    // A format coefficient fits u128 (≤ 34 digits); such a `c` is
    // outside every format's input set, so bailing loses nothing.
    if c.hi != 0 {
        return None;
    }
    let (coef_out, exp_out) = rsqrt_exact_parts(c.lo, e)?;
    let coef_out = U256::from_u128(coef_out);
    // The coefficient is a pure power of two or of five, hence already
    // stripped: wider than `PRECISION + 1` digits is neither
    // representable nor a midpoint.
    if coef_out.decimal_digit_count() > F::PRECISION + 1 {
        return None;
    }
    Some(pack_value(coef_out, exp_out, false, rm))
}

/// The exact `(coefficient, exponent)` of `1/√(c · 10^e)` for a
/// stripped positive `(c, e)`, or `None` when the value is irrational.
/// Format independent: the caller owns the `PRECISION + 1` width gate
/// and the delivery. Split out from [`rsqrt_exact_input`] so the number
/// theory can be exercised directly, on a mock format whose rounder is
/// `unreachable!`.
fn rsqrt_exact_parts(c: u128, e: i32) -> Option<(u128, i32)> {
    // `c < 10^34 < 2^113` bounds `v₂ ≤ 112`, and `5^49 > 10^34` bounds
    // `v₅ ≤ 48`, so both powers stay well inside `u128`.
    let v2 = factor_count(c, 2);
    let v5 = factor_count(c, 5);
    if c / 2u128.pow(v2) / 5u128.pow(v5) != 1 {
        // `s ≠ 1`: neither exact nor a tie — irrational when `s` is
        // not a perfect square, else rational with a non-terminating
        // `1/q` factor (derivation above). Ties terminate, so neither
        // shape reaches a boundary.
        return None;
    }
    // `|e| < 7000` at every format and `v₂ ≤ 112`, so both sums stay
    // far inside `i64`.
    let a = i64::from(v2) + i64::from(e);
    let b = i64::from(v5) + i64::from(e);
    if a % 2 != 0 || b % 2 != 0 {
        // An odd exponent of 2 or of 5 leaves a `√2`, `√5`, or `√10`
        // factor in the value: irrational, so neither exact nor a tie.
        return None;
    }
    // Even, so the truncating division is exact on both signs.
    let (i, j) = (a / 2, b / 2);
    let (coef_out, exp_out) = if i >= j {
        // `v₅ = 0` (a stripped coefficient carries at most one of the
        // two factors), so `i − j = v₂/2 ≤ 56`.
        let d = u32::try_from(i - j).ok()?;
        // `5^56` carries 40 digits, past every format's
        // `PRECISION + 1 ≤ 35`, and `5^d` grows with `d`: the bail
        // loses no exact or tie case, and it doubles as the `u128`
        // guard (`5^55` is the last power of five inside the envelope).
        if d > 55 {
            return None;
        }
        (5u128.pow(d), -i)
    } else {
        // `v₂ = 0`, so `j − i = v₅/2 ≤ 24`; the guard is defensive
        // (`2^128` overflows `u128`, and 39 digits is past every
        // format's `PRECISION + 1` besides).
        let d = u32::try_from(j - i).ok()?;
        if d > 127 {
            return None;
        }
        (2u128.pow(d), -j)
    };
    // `|i|, |j| ≤ (112 + 7000)/2 < 3600`, so the exponent fits `i32`
    // with orders to spare; the conversion is defensive.
    Some((coef_out, i32::try_from(exp_out).ok()?))
}

/// `true` when `result` is the exact reciprocal square root of `x`:
/// squaring it and multiplying by `x` reproduces exactly 1, checked in
/// fixed-width integer arithmetic on the canonical coefficients.
///
/// The independent witness [`rsqrt_exact_input`]'s number theory is
/// cross-checked against, in the shape of [`cube_is_exact`] and
/// [`power_is_exact`] — and test-only for the same reason those are:
/// a post-hoc check can only recognise a value the kernel already
/// delivered exactly, which is circular as a production predicate and
/// fine as a second proof of the same boundary fact.
#[cfg(test)]
fn rsqrt_is_exact<F: DecimalFormat>(result: F, x: F) -> bool {
    let (Some((cr, er, _)), Some((cx, ex, _))) =
        (result.to_extended_parts(), x.to_extended_parts())
    else {
        return false; // NaN / Inf: not an exact finite result.
    };
    if cr.is_zero() || cx.is_zero() {
        return false;
    }
    let (cr, er) = strip_trailing_zeros(cr, er);
    let (cx, ex) = strip_trailing_zeros(cx, ex);
    let Some(sq) = checked_mul_u256(cr, cr) else {
        return false;
    };
    // Keep the closing product inside U384.
    if sq.decimal_digit_count() + cx.decimal_digit_count() > 115 {
        return false;
    }
    let Some(total) = er.checked_mul(2).and_then(|d| d.checked_add(ex)) else {
        return false;
    };
    value_is_one(u256_mul_u256(sq, cx), total)
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `powi` (IEEE 754-2019
// §9.2 `pown`; ADR-0059 Track D group D3, the ADR-0060 premise).

/// The widest decimal exponent [`powi_exact_input`] hands the format
/// rounder, matching [`exp10_integer`]'s decode window so the two
/// whole-range classifiers exercise the same envelope.
///
/// The window is a *soundness* bound, not a convenience one:
/// `round_and_pack_finite` computes `qmin − unbiased_exp` in `i32`,
/// which wraps once the exponent approaches `i32::MAX` (in a debug
/// build it panics; in a release build it would ship a wrong
/// disposition). Every classifier that can produce an exponent from
/// an operand-scaled product — this one and [`pow_exact_input`],
/// whose unwindowed `w = α·a` reached `i32::MAX` at
/// `pow(10, 2147483647)` and wrapped the rounder (fd-clc) — owes the
/// rounder an exponent inside its tested envelope. The bail proofs on
/// both classifiers show declining past the window loses no boundary
/// case.
const EXACT_EXPONENT_WINDOW: u64 = 99_999;

/// The exact or tie value of `powi(x, n) = x^n` decided from the input
/// alone, for a positive finite nonzero `abs_x` and any `n`; `None`
/// routes to the kernel. The caller works on `|x|` under the
/// negation-reflected rounding mode and re-applies the odd-`n` sign
/// (the fd-aqs.5 rule), exactly as [`pow_exact_input`]'s caller does.
///
/// ## The criterion, with the denominator gone
///
/// This is [`pow_exact_input`]'s decimal Lauter–Lefèvre criterion at
/// `|y| = a/b` with `(a, b) = (|n|, 1)`, which is not a special case
/// so much as a collapse: `b = 1` divides every integer, so the
/// `b | α` and `b | β` tests are vacuous, and `t = s^b` holds with
/// `s = t` for every `t`, so the `nth_root_u128` search is gone too.
/// **Every** representable `|x|` is a first power of itself, so
/// `x^n` is rational for every `n` and the classification question is
/// no longer "is it rational" but only "is the rational narrow
/// enough to be a format value or a midpoint". That is the whole
/// difference between this classifier and `pow`'s, and it is why the
/// derivation is re-done here rather than shared: sharing the body
/// would carry three tests whose answers are constants.
///
/// Factor the stripped `|x| = c · 10^e` as `2^α · 5^β · s` with
/// `α = v₂(c) + e`, `β = v₅(c) + e` (both possibly negative) and `s`
/// coprime to 10. Then
///
/// > `x^n = s^n · 2^(αn) · 5^(βn)`,
///
/// assembled as `coef · 10^w` with `u = αn`, `v = βn`,
/// `w = min(u, v)` and `coef = s^|n| · 2^(u−w) · 5^(v−w)` for `n > 0`
/// (`coef = 2^(u−w) · 5^(v−w)` for `n < 0`, where exactness needs
/// `s = 1`). At most one of `u−w`, `v−w` is nonzero and `s` is
/// coprime to 10, so `coef` is already stripped and its digit count
/// is the value's: the `PRECISION + 1` gate then admits exactly the
/// representable values and the nearest-mode midpoints, and
/// [`pack_value`] resolves the mode, the tie rule, and the flags by
/// construction.
///
/// ## The whole-range power-of-ten family (the D2 lesson, inverted)
///
/// `exp10_integer` had to classify `10^n` at *every* integer `n`,
/// representable exponent or not, because such a value sits exactly
/// ON a grid point at its own exponent and no rung of the ladder can
/// move it off. The same hazard arrives here through the input rather
/// than the output: for `x = 10^j` exactly (stripped coefficient 1),
/// `x^n = 10^(j·n)` is on the grid at any magnitude. The general path
/// above already covers it — `s = 1`, `α = β = j`, `coef = 1`,
/// `w = j·n` — provided the product is formed in `i64`, which it is;
/// an `i32` product would wrap and ship a wrong exponent, the one
/// arithmetic mistake in this routine that would not announce itself.
/// Inside `i32` the value is delivered through [`pack_value`], whose
/// §7.4 disposition is correct by construction at any exponent,
/// over/underflow and the directed-mode largest-finite cases
/// included.
///
/// ## Bail-site completeness proofs
///
/// Every `None` below is provably neither exact nor a nearest-mode
/// tie. The recurring facts: an exact-or-tie value has a *stripped*
/// coefficient of at most `PRECISION + 1 ≤ 35` digits (in the
/// subnormal range a midpoint is `(2k+1)·10^(etiny−1)` with
/// `k < 10^PRECISION`, so the bound holds there too), and
/// `|α|, |β| ≤ v₂/₅(c) + |e| < 6300` for every format.
///
/// * `n < 0` with `s ≠ 1`: `x^n = s^−|n| · 2^(−α|n|) · 5^(−β|n|)`.
///   The powers of 2 and 5 fold into a decimal exponent, but `s ≥ 2`
///   coprime to 10 makes `s^−|n|` a fraction whose lowest-terms
///   denominator is `s^|n| ∤ 10^k` for every `k`: a non-terminating
///   decimal, hence neither representable nor a midpoint (both
///   terminate).
/// * `du > 127` / `dv > 55`: `2^128` has 39 digits and `5^56` has 40,
///   both past every format's `PRECISION + 1`, and `coef` is a
///   multiple of that power. Wider than the gate means wider than a
///   midpoint.
/// * [`int_pow_u256`] / [`checked_mul_u256`] overflow: `coef` exceeds
///   `U256`'s ~78 digit envelope, twice the widest gate.
/// * The closing digit gate: the same fact stated directly.
/// * `|w| > EXACT_EXPONENT_WINDOW` (99,999): this bail is reached only
///   *after* the digit gate, so the declined value is exactly
///   `coef · 10^w` with `1 ≤ coef < 10^35`. Its logarithm is
///   therefore `|n·ln|x|| = |ln(x^n)| ≥ 99,999·ln 10 − 81 > 230,000`,
///   past both `exp` gates (the overflow limits 14,150 / 887 / 224
///   and the underflow limits 14,221 / 918 / 235) of all three
///   formats, so the kernel's saturation proxy answers it unguarded
///   and correct by its own margin argument — the same 230,000 the
///   [`exp10_integer`] beyond-the-decode-limit proof turns on, since
///   both windows are the same size. The `|n| ≤ 6` powering arm,
///   which has no gate to fall back on, never reaches this bail: its
///   results carry `|w| ≤ 6·6211 = 37,266`, comfortably inside the
///   window, so every on-grid value it could otherwise be handed is
///   disposed of here instead.
pub(crate) fn powi_exact_input<F: DecimalFormat>(
    abs_x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef, exp) = powi_exact_parts::<F>(abs_x, n)?;
    Some(pack_value(coef, exp, false, rm))
}

/// The exact `(coefficient, decimal exponent)` of `|x|^n` when it is
/// expressible in at most `F::PRECISION + 1` stripped digits at an
/// `i32` exponent; `None` otherwise. Split out of
/// [`powi_exact_input`] so the number theory is testable without a
/// value-carrying format rounder (the mock format's
/// `round_and_pack_finite` is `unreachable!`).
fn powi_exact_parts<F: DecimalFormat>(abs_x: F, n: i32) -> Option<(U256, i32)> {
    let (coef_x, exp_x, _) = abs_x.to_extended_parts()?;
    if coef_x.is_zero() {
        return None; // zeros are disposed of by the special cases
    }
    let (cx, ex) = strip_trailing_zeros(coef_x, exp_x);
    // A format coefficient fits u128 (≤ 34 digits); bail defensively.
    // Such a `c` is outside every format's input set, so no exact or
    // tie case is lost.
    if cx.hi != 0 {
        return None;
    }
    let neg = n < 0;
    // `i32::MIN.unsigned_abs()` is `2^31`, which `u32` holds exactly:
    // no negation overflow anywhere below.
    let a = n.unsigned_abs();
    // |x| = 2^α · 5^β · s on the stripped parts. A stripped `c` shares
    // no factor of ten, so at most one of v₂, v₅ is nonzero.
    let v2 = factor_count(cx.lo, 2);
    let v5 = factor_count(cx.lo, 5);
    let s = cx.lo / 2u128.pow(v2) / 5u128.pow(v5);
    let alpha = i64::from(v2) + i64::from(ex);
    let beta = i64::from(v5) + i64::from(ex);
    if neg && s != 1 {
        return None; // 1/s^|n| is a non-terminating rational
    }
    // Exponents of 2 and 5 in the result. |α|, |β| < 6300 and
    // `a ≤ 2^31` keep the products inside i64 with ten orders to
    // spare; an i32 product would wrap on the power-of-ten family.
    let mut u = alpha * i64::from(a);
    let mut v = beta * i64::from(a);
    if neg {
        u = -u;
        v = -v;
    }
    let w = u.min(v);
    let (du, dv) = (u - w, v - w); // at least one is zero
                                   // 2^128 has 39 digits and 5^56 has 40, both past every format's
                                   // PRECISION + 1 (≤ 35): wider is neither exact nor a tie.
    if du > 127 || dv > 55 {
        return None;
    }
    let s_pow_n = if neg {
        U256::from_u128(1)
    } else {
        int_pow_u256(U256::from_u128(s), a)?
    };
    let pow2 = int_pow_u256(U256::from_u128(2), du as u32)?;
    let pow5 = int_pow_u256(U256::from_u128(5), dv as u32)?;
    let coef = checked_mul_u256(checked_mul_u256(s_pow_n, pow2)?, pow5)?;
    if coef.decimal_digit_count() > F::PRECISION + 1 {
        return None;
    }
    if w.unsigned_abs() > EXACT_EXPONENT_WINDOW {
        return None;
    }
    Some((coef, w as i32))
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `rootn` (IEEE 754-2019
// §9.2 `rootn`, ADR-0059 Track D D3). Self-contained rather than a
// call into [`pow_exact_input`]: the exponent `1/n` is not a format
// datum, and the `a = 1` numerator collapses most of `pow`'s exponent
// arithmetic, so the shared shape is a derivation, not a call.

/// The exact or tie value of `rootn(x, n) = x^(1/n)` decided from the
/// input alone, for a positive finite nonzero `abs_x` and an `n` with
/// `|n| ≥ 2`; `None` routes to the kernel. The caller works on `|x|`
/// under the negation-reflected rounding mode and re-applies the sign
/// for a negative base with odd `n` (the fd-aqs.5 rule), exactly as
/// `pow` and `cbrt` do.
///
/// ## The criterion (`pow`'s, at `y = 1/n`)
///
/// Factor `|x| = 2^α · 5^β · t` with `gcd(t, 10) = 1` (from the
/// stripped `c · 10^e`: `α = v₂(c) + e`, `β = v₅(c) + e`, both
/// possibly negative) and write `b = |n|`. Then `|x|^(1/b)` is
/// rational iff `|x|` is a `b`-th power in `ℚ`, i.e. iff
///
/// > `b | α`, `b | β`, and `t = s^b` for an integer `s`,
///
/// and then `|x|^(1/b) = s · 2^(α/b) · 5^(β/b)` exactly. The
/// criterion is necessary as well as sufficient: if `y` is rational
/// with `y^b = |x|`, then `b · v_p(y) = v_p(|x|)` for every prime `p`
/// under unique factorization, so `b` divides every prime exponent of
/// `|x|` and `s = ∏ p^(v_p(t)/b)` is the integer root of `t`. Failing
/// any of the three, `x^(1/n)` is irrational — neither exact nor a
/// nearest mode tie, since a tie value terminates and hence is
/// rational — and the kernel's unconditional `INEXACT` is correct in
/// every mode. All three conditions are decided in bounded integer
/// arithmetic with no factoring: `v₂` / `v₅` by trial division,
/// `t = s^b` by [`nth_root_u128`].
///
/// For `n < 0` the value is `s^(−1) · 2^(−α/b) · 5^(−β/b)`; the
/// negative powers of 2 and 5 fold into the decimal exponent, but
/// `1/s` with `s ≥ 2` coprime to 10 is a non-terminating rational —
/// not representable and not a midpoint — so exactness additionally
/// requires `s = 1`. This is `pow`'s bail proof at `a = 1`, and it
/// transfers because the argument never used `a > 1`: it is unique
/// factorization of the denominator, not the size of the numerator.
///
/// The rational value is assembled as `coef · 10^w`: with
/// `u = ±α/b`, `v = ±β/b` (signs following `n`), `w = min(u, v)`, the
/// coefficient `s · 2^(u−w) · 5^(v−w)` is already stripped (at most
/// one of the 2- and 5-exponents is nonzero, and `s` is coprime to
/// 10), so the `PRECISION + 1` digit gate and [`pack_value`] finish
/// the job: an exact representable value packs `OK`, a
/// `PRECISION + 1`-digit value rounds with the correct tie rule,
/// directed sides, and flags, and an out-of-range exponent
/// over/underflows through the rounder with exactly the §7.4
/// disposition.
///
/// ## `|x| = 1` is inside the criterion, not beside it
///
/// A stripped `c = 1, e = 0` gives `α = β = 0` and `t = 1`, so every
/// `b` divides both and `s = 1`: the classifier returns exactly `1`,
/// status `OK`, for every `n` and every rounding direction, including
/// `n < 0`. Unlike [`pow_exact_input`] this needs no pre-check,
/// because `b = |n|` is bounded by `i32` construction rather than by
/// a rational reduction that could overflow. The kernel's hug-at-1
/// anchor arm leans on this: past the classifier `|x| ≠ 1`, so
/// `ln|x| ≠ 0` and the arm's strict side theorem holds.
///
/// ## Ties exist, and only at negative orders
///
/// A midpoint's stripped coefficient has exactly `PRECISION + 1`
/// digits and ends in 5. Which side of `rootn` reaches that shape is
/// decidable, and the answer is asymmetric:
///
/// * **`n > 0`: no ties.** Write the result's stripped coefficient as
///   `C = s · 2^du · 5^dv` at exponent `w = min(α, β)/b`. Then the
///   input's stripped coefficient is
///   `s^b · 2^(α − bw) · 5^(β − bw) = C^b`: the input carries the
///   `b`-th power of the result's coefficient. A `PRECISION + 1`
///   digit `C` would need an input of at least `2·PRECISION + 1`
///   digits, which no format can hold. Positive orders are therefore
///   tie free, exactly as `cbrt` is.
/// * **`n < 0`: ties, one family per format.** Here `s = 1` and the
///   reciprocal swaps the roles: with `α > β` the coefficient is
///   `5^dv` with `dv = (α − β)/b`, which ends in 5 for every
///   `dv ≥ 1`, while the input carries `2^(α − β) = 2^(b·dv)` —
///   *shorter* than `5^dv` for `b = 2`, since `2^2` has fewer digits
///   than `5`. So `b = 2` (and only `b = 2`, as `2^(3·dv)` already
///   outgrows every format) reaches the midpoint: `rootn(2^2k, −2)`
///   has value `5^k · 10^−k`, and the `k` whose `5^k` is exactly
///   `PRECISION + 1` digits wide is a nearest mode tie —
///   `rootn(2^100, −2) = 2^−50` at `Decimal128`,
///   `rootn(2^48, −2) = 2^−24` at `Decimal64`,
///   `rootn(2^22, −2) = 2^−11` at `Decimal32`.
///
/// The width gate therefore admits `PRECISION + 1` digits and
/// [`pack_value`] hands the exact coefficient to the format rounder,
/// whose own tie rule resolves a value the approximation kernel
/// cannot: the true value IS the boundary.
///
/// ## Bail-site completeness proofs
///
/// Every `None` below is provably neither exact nor a tie. The
/// recurring facts: an exact-or-tie value has a stripped coefficient
/// of at most `PRECISION + 1 ≤ 35` digits, and `|α|, |β| ≤ v₂/₅(c) +
/// |e| < 7000` for every format, so `|u|, |v| ≤ 3500` and every
/// product below stays inside `i64` with orders to spare.
///
/// * `cx.hi != 0`: a format coefficient fits `u128` (≤ 34 digits), so
///   such an `abs_x` is outside every format's input set.
/// * `|n| < 2`: not this classifier's domain. The kernel answers
///   `n = ±1` by delegation (identity and a §5 division, both exact
///   or correctly rounded on their own) and `n = 0` in its special
///   cases, so no exact case is lost here.
/// * `α % b ≠ 0` or `β % b ≠ 0` or `t ≠ s^b`: the criterion above,
///   whose failure makes the value irrational.
/// * [`nth_root_u128`]'s own `b ≥ 128` bail: `t ≥ 2` and `s ≥ 2`
///   would give `s^b ≥ 2^128`, past every `u128` `t`, so no root
///   exists there either.
/// * `n < 0` with `s ≠ 1`: the non-terminating reciprocal above.
/// * Width bails (`du`/`dv` gates, `int_pow_u256`, the final digit
///   gate): `2^128` has 39 digits and `5^56` has 40, both past every
///   format's `PRECISION + 1 ≤ 35`; the coefficient is stripped, so
///   more than `PRECISION + 1` digits is neither representable nor a
///   midpoint.
/// * `w` outside `i32`: unreachable given `|w| ≤ 3500`, kept as the
///   defensive conversion `pow` carries for the same reason.
pub(crate) fn rootn_exact_input<F: DecimalFormat>(
    abs_x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef_x, exp_x, _) = abs_x.to_extended_parts()?;
    if coef_x.is_zero() {
        return None; // zero short-circuits at the kernel
    }
    let (cx, ex) = strip_trailing_zeros(coef_x, exp_x);
    if cx.hi != 0 {
        return None;
    }
    let (coef, exp) = rootn_exact_parts(cx.lo, ex, n)?;
    if coef.decimal_digit_count() > F::PRECISION + 1 {
        return None;
    }
    Some(pack_value(coef, exp, false, rm))
}

/// The number-theoretic half of [`rootn_exact_input`]: the criterion
/// and the assembled `(coefficient, exponent)` of the exact value, on
/// the stripped parts of `|x| = cx · 10^ex` and the root order `n`.
/// `None` is a bail of the criterion or of a format-independent width
/// gate; the caller adds the `PRECISION + 1` gate and the delivery.
/// Split out so the criterion is testable without a format rounder.
fn rootn_exact_parts(cx: u128, ex: i32, n: i32) -> Option<(U256, i32)> {
    // `b = |n|`, always exact in `u32` (`i32::MIN.unsigned_abs()` is
    // `2^31`); the kernel routes `|n| ≤ 1` elsewhere.
    let b32 = n.unsigned_abs();
    if b32 < 2 {
        return None;
    }
    let b64 = i64::from(b32);
    let n_neg = n < 0;
    // Factor |x| = 2^α · 5^β · t on the stripped parts.
    let v2 = factor_count(cx, 2);
    let v5 = factor_count(cx, 5);
    let t = cx / 2u128.pow(v2) / 5u128.pow(v5);
    let alpha = i64::from(v2) + i64::from(ex);
    let beta = i64::from(v5) + i64::from(ex);
    // The b-th power criterion; failing it, x^(1/n) is irrational.
    if alpha % b64 != 0 || beta % b64 != 0 {
        return None;
    }
    let s = if t == 1 {
        1u128
    } else {
        nth_root_u128(t, b32)?
    };
    if n_neg && s != 1 {
        // 1 / s is a non-terminating rational: not representable,
        // not a tie.
        return None;
    }
    // Exponents of 2 and 5 in the result. |α/b| < 3500, so the
    // negation and the difference below stay far inside i64.
    let mut u = alpha / b64;
    let mut v = beta / b64;
    if n_neg {
        u = -u;
        v = -v;
    }
    let w = u.min(v);
    let (du, dv) = (u - w, v - w); // at least one is zero
                                   // 2^128 has 39 digits and 5^56 has 40, both past every format's
                                   // PRECISION + 1 (≤ 35): wider is neither exact nor a tie.
    if du > 127 || dv > 55 {
        return None;
    }
    let pow2 = int_pow_u256(U256::from_u128(2), du as u32)?;
    let pow5 = int_pow_u256(U256::from_u128(5), dv as u32)?;
    let coef = checked_mul_u256(checked_mul_u256(U256::from_u128(s), pow2)?, pow5)?;
    Some((coef, i32::try_from(w).ok()?))
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `compound` (ADR-0059
// Track D D3, fd-4zo.25; ADR-0060 Engine A).
//
// This section is self-contained: it shares only `strip_trailing_zeros`,
// `int_pow_u256`, and `checked_mul_u256` with the classifiers above.
//
// What separates `compound` from every other classifier in this module:
// its value is ALWAYS rational. `1 + x` is an exact rational for every
// in-domain representable `x` (the D1 `logp1` exact-sum analysis), and an
// integer power of a rational is rational. The transcendental classifiers
// answer "is this one of the rare algebraic inputs?"; this one answers
// "is this rational representable, a nearest-mode midpoint, or neither?".
// The completeness obligation is unchanged and just as binding: a `None`
// must be provably neither, because the kernel raises `INEXACT`
// unconditionally past this point.

/// Strip the prime factor `p` (2 or 5) out of `n`, returning its
/// multiplicity and the reduced value. Caller guarantees `n ≥ 1` (a zero
/// `n` would loop forever, since `0 % p == 0`). The [`U256`] analog of
/// [`factor_count`], needed because `compound`'s numerator `N` can reach
/// ~70 digits while a format coefficient stays inside 34.
///
/// Bounded by `log2(N) ≤ 236` iterations over this section's gated `N`.
fn factor_count_u256(mut n: U256, p: u128) -> (u32, U256) {
    let mut c = 0;
    loop {
        let (q, r) = n.div_rem_u128(p);
        if r != 0 {
            break;
        }
        n = q;
        c += 1;
    }
    (c, n)
}

/// `Some(k)` iff `1 + x = 10^k` exactly, for the stripped decomposition
/// `|x| = a · 10^u` with sign `sign`; `None` otherwise. Caller guarantees
/// `x` is finite, nonzero, and strictly above `−1`.
///
/// ## The family, derived
///
/// `1 + x = 10^k` forces `x = 10^k − 1`, and the stripped forms are
/// unique, so the family is exactly the nines patterns (the same set
/// [`log10p1_exact`] classifies, arrived at here from the `compound`
/// side rather than reused, since the two classifiers answer different
/// questions about it):
///
/// * `k ≥ 1`: `x = 10^k − 1`, the `k`-nines integer (`9`, `99`, `999`,
///   …). It ends in 9, so its stripped exponent is `0` and its stripped
///   coefficient carries exactly `k` digits.
/// * `k = 0`: `x = 0`, disposed of by the caller's special cases.
/// * `k = −m ≤ −1`: `x = 10^−m − 1 = −(10^m − 1) · 10^−m`, the `m`-nines
///   fraction (`−0.9`, `−0.99`, …), stripped coefficient `10^m − 1` at
///   stripped exponent `−m`.
///
/// A format coefficient carries at most `F::PRECISION ≤ 34` digits, so
/// `|k| ≤ 34` and the `10^k` below stays far inside `u128`.
///
/// ## Every other shape is provably not a power of ten
///
/// * `x > 0` with stripped `u ≥ 1`: `N = a·10^u + 1 ≡ 1 (mod 10)`, while
///   `10^k ≡ 0 (mod 10)` for every `k ≥ 1` and `10^k ≤ 1` for `k ≤ 0`.
/// * `x > 0` with stripped `u < 0`: `1 + x = (10^d + a)/10^d` with
///   `d = −u ≥ 1`; a stripped `a` shares no factor of ten, so the
///   numerator is not divisible by 10 and exceeds `10^d ≥ 10`, hence is
///   no power of ten and the quotient is no power of ten either.
/// * `x < 0` (necessarily `u < 0`, since `|x| < 1`): the numerator
///   `10^d − a` is again not divisible by 10, so it is a power of ten
///   only when it equals 1, i.e. `a = 10^d − 1` — the case caught below.
fn one_plus_is_power_of_ten(a: u128, u: i32, sign: bool) -> Option<i32> {
    let digits = U256::from_u128(a).decimal_digit_count();
    // The `k`-nines coefficient is `10^digits − 1`; `digits ≤ 34` for
    // every format coefficient, so the exponentiation cannot overflow.
    let nines = 10u128.checked_pow(digits)?.checked_sub(1)?;
    if a != nines {
        return None;
    }
    if sign {
        // `x = −(10^m − 1)·10^−m` needs the nines run and the stripped
        // exponent to match: `m` nines at exponent `−m`.
        if u == -(digits as i32) {
            return Some(u);
        }
        None
    } else if u == 0 {
        // `x = 10^k − 1` at stripped exponent 0, so `k` is the run length.
        Some(digits as i32)
    } else {
        None
    }
}

/// The largest `|k·n|` [`compound_exact_parts`] hands to the format
/// rounder for the power-of-ten family. Two obligations meet here.
///
/// **Upward** it must cover every `k·n` whose disposition the shared
/// `exp` gates cannot supply. Those gates fire at `|n · ln(1+x)| >`
/// the format's limit (at most 14,221 across the three formats), i.e.
/// at `|k·n| · ln 10 > 14,221`, i.e. from `|k·n| ≥ 6,177`. This limit
/// clears that by two orders of magnitude, so everything it declines
/// has `|k·n| · ln 10 > 2.3·10^6`, past every gate with enormous
/// margin, and the kernel's saturation proxy answers it unguarded on
/// its own margin argument — the same beyond-the-gates shape
/// [`exp10_integer`] uses. Every representable exponent (`|k·n|` at
/// most 6,176) and both gate-gap integers sit far inside.
///
/// **Downward** it keeps the delivered exponent inside the range the
/// format rounder's own `i32` arithmetic is defined on: that routine
/// forms `qmin − unbiased_exp` and `unbiased_exp + excess`, which
/// overflow near `i32::MIN`/`i32::MAX` (`k·n` reaches `±6.9·10^10`
/// before this gate, since `|k| ≤ 34` and `|n| ≤ 2^31`). A million is
/// six orders inside `i32` and cannot.
const POWER_OF_TEN_EXPONENT_LIMIT: u64 = 1_000_000;

/// The IEEE 754-2019 §9.2.2 preferred quantum exponent of
/// `compound(x, n)`: `floor(n × min(0, Q(x)))`, where `Q(x)` is `x`'s
/// *stored* quantum exponent (cohort-sensitive by design, exactly as
/// §9.2.2 states it — the delivered quantum of `compound(0.05, 3)`
/// follows the `5E−2` representation, not the numeric value).
///
/// Both factors are integers, so the floor is the identity; the product
/// is formed in `i64` (`|n| ≤ 2^31`, `|Q(x)| ≤ 6176`, so it cannot
/// overflow) and saturated into the `i32` the rounder takes. Saturation
/// is behaviour-preserving: §6.3 targets `MAX(q_preferred, q_emin)` and
/// shifts only as far as the coefficient allows, so every `q_preferred`
/// past the format's quantum range is equivalent to the range endpoint.
fn preferred_quantum(q_x: i32, n: i32) -> i32 {
    let q = i64::from(n) * i64::from(q_x.min(0));
    q.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// The exact or tie value of `compound(x, n) = (1 + x)^n` decided from
/// the inputs alone; `None` routes to the kernel. The caller
/// (`crate::compound::compound_kernel_body`) has already run
/// `compound_special_cases`, so every `x` reaching here is finite,
/// nonzero, and strictly above `−1`, and `n ≠ 0`.
///
/// ## The base is an exact rational, always
///
/// Write `|x| = a · 10^u` in stripped form (`a` free of trailing zeros;
/// stripped forms are unique). Then `1 + x = N / 10^d` with
///
/// * `u ≥ 0` (so `x > 0`: a negative `x` with `|x| ≥ 1` is `≤ −1` and
///   was disposed of by the caller): `d = 0`, `N = a·10^u + 1`;
/// * `u < 0`: `d = −u`, `N = 10^d + a` for `x > 0` and `N = 10^d − a`
///   for `x < 0`, positive because `x > −1` forces `a < 10^d`.
///
/// Factoring `N = 2^i · 5^j · t` with `gcd(t, 10) = 1` gives
/// `1 + x = 2^α · 5^β · t` with `α = i − d`, `β = j − d`, and hence
///
/// > `compound(x, n) = t^n · 2^(nα) · 5^(nβ)`.
///
/// With `A = nα`, `B = nβ`, `w = min(A, B)`, the coefficient
/// `t^n · 2^(A−w) · 5^(B−w)` is already stripped (at most one of the 2-
/// and 5-exponents survives, and `t` is coprime to 10), so the
/// `PRECISION + 1` digit gate decides representability and tie-ness
/// together and the rounder finishes the job. This is `pown` on the
/// exact rational `1 + x`, the reduction ADR-0060 records for this
/// operation; the assembly is the shape [`pow_exact_input`] uses for its
/// own `2^α 5^β t` decomposition, re-derived here for the base `1 + x`.
///
/// ## The whole-range power-of-ten family (checked FIRST, load bearing)
///
/// When `1 + x = 10^k` the value is `10^(k·n)` with coefficient 1 at
/// **any** magnitude, in range or not. That family must be decided input
/// side for the same reason `exact::exp10_integer`'s is (the D2 lesson):
/// `10^(kn)` sits exactly ON a format grid point at its own exponent, a
/// distance no rung of the ADR-0059 ladder can grow, so a guarded
/// delivery would escalate forever — panicking the `ladder_audit` lane
/// and, under `unbounded-ladder`, widening without terminating. Handing
/// the rounder the exact coefficient-1 form instead makes every §7.4
/// disposition correct by construction: exact with status `OK` inside
/// `[etiny, emax]`, the overflow disposition per direction above it, the
/// underflow disposition below it.
///
/// The width gates of the general branch would bail on most of this
/// family (`N = 10^k` raised to a large `n` outruns any envelope), which
/// is exactly why it is tested first rather than left to fall through.
///
/// ## Bail-site completeness proofs
///
/// Every `None` below is provably neither exact nor a nearest-mode tie.
/// The recurring facts: an exact-or-tie value has a *stripped*
/// coefficient of at most `PRECISION + 1 ≤ 35` digits (a normal-range
/// midpoint's stripped coefficient has exactly `PRECISION + 1` digits
/// and ends in 5; a subnormal-range midpoint `(2c+1)·5·10^(etiny−1)` has
/// at most that many too), and a value whose stripped coefficient is
/// wider is neither representable nor a midpoint at any exponent.
///
/// * **`|k·n|` past [`POWER_OF_TEN_EXPONENT_LIMIT`]** (power-of-ten
///   family): then `|n · ln(1+x)| = |k·n| · ln 10 > 2.3·10^6`, past
///   every format's `exp` gates (14,150 / 887 / 224 on the overflow
///   side, 14,221 / 918 / 235 on the underflow side) by two orders of
///   magnitude, and `10^(kn)` is past `MAX` (or below half the
///   smallest subnormal) by hundreds of thousands of decades. The
///   kernel's saturation proxy answers it unguarded on its own margin
///   argument. This mirrors `exp10_integer`'s beyond-the-decode-limit
///   proof; that constant's rustdoc carries the two-sided derivation.
/// * **`u ≥ 1` with `digits(a) + u > PRECISION + 1`**: here
///   `N = a·10^u + 1 ≡ 1 (mod 10)`, so `gcd(N, 10) = 1`, `t = N`, and
///   `α = β = 0`. For `n < 0` the reciprocal `1/N^|n|` is
///   non-terminating (`N ≥ 11`), hence neither representable nor a tie.
///   For `n > 0` the coefficient is `N^n ≥ N ≥ 10^(PRECISION+1)`, and it
///   is stripped (coprime to 10), so it is too wide for either.
/// * **`d > 2·PRECISION + 2`**: with `a ≤ 10^PRECISION − 1` and
///   `d > 2·PRECISION + 2`, `N` lies within `10^PRECISION` of `10^d`, so
///   `N > 10^(d−1) ≥ 10^(2·PRECISION+2)`. For `n > 0` an exact-or-tie
///   coefficient forces `t < 10^(P+1)` and (since `d ≥ 1` makes `N`
///   indivisible by 10, so `min(i, j) = 0`) either `2^(n·i) < 10^(P+1)`
///   or `5^(n·j) < 10^(P+1)`, bounding `N = 2^i 5^j t < 10^(2P+2)` — a
///   contradiction. For `n < 0` exactness forces `t = 1`, i.e. `N = 2^i`
///   (then `5^(|n|·i) < 10^(P+1)` bounds `N ≤ 2^50 < 10^16` at
///   `Decimal128`, contradiction) or `N = 5^j` (then
///   `2^(|n|·j) < 10^(P+1)` bounds `j ≤ 116`). The residual `N = 5^j`
///   sliver is closed by exhaustive enumeration: no power of two and no
///   power of five in the reachable exponent range sits within
///   `10^PRECISION` of a power of ten at any `d > PRECISION`, at any of
///   the three format precisions. That enumeration is the unit test
///   `compound_negative_n_terminating_bases_stay_narrow`, the
///   machine-checked leg of this bail (the ADR-0060 posture: derivation
///   plus exhaustive small-range probe).
/// * **`n < 0` with `t ≠ 1`**: `1/t^|n|` in lowest terms keeps a
///   denominator with a prime factor other than 2 and 5, so the value is
///   a non-terminating decimal: not representable, and not a midpoint
///   (midpoints terminate).
/// * **The `du` / `dv` width gates and the closing digit gate**: `2^128`
///   carries 39 digits and `5^56` carries 40, both past every format's
///   `PRECISION + 1 ≤ 35`; the assembled coefficient is stripped, so
///   more than `PRECISION + 1` digits is neither representable nor a
///   midpoint.
/// * **`w` past [`POWER_OF_TEN_EXPONENT_LIMIT`]** (defensive on this
///   branch: the gates above already bound `|n| ≤ 127`, so `|w| ≤
///   127 · 310 < 40,000`): the value sits astronomically past every
///   format's exponent range, over/underflow territory with no boundary
///   structure (a tie lives inside the representable range plus one
///   quantum).
///
/// ## The ties are real, and reachable from both signs of `n`
///
/// A midpoint's stripped coefficient ends in 5, so it needs the 5-leg
/// and not the 2-leg: `A = w < B` with `t` odd (automatic — `t` is
/// coprime to 10). The smallest witnesses at `Decimal128`:
/// `compound(4, 49) = 5^49` and `compound(4, 50) = 5^50`, whose 35-digit
/// coefficients are exact nearest-mode midpoints, and their negative-`n`
/// mirror `compound(1, −49) = 2^−49 = 5^49 · 10^−49`. The siblings scale
/// the same way (`compound(4, 23)` / `compound(4, 24)` at `Decimal64`,
/// `compound(4, 11)` at `Decimal32`). The classifier hands each
/// midpoint's exact coefficient to the format rounder, whose own tie
/// rule resolves a value no approximation kernel can: the true value IS
/// the rounding boundary.
pub(crate) fn compound_exact_input<F: DecimalFormat>(
    x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef, exp, q_preferred) = compound_exact_parts::<F>(x, n)?;
    // The exact coefficient, delivered under the §9.2.2 preferred
    // quantum instead of [`pack_value`]'s §6.3 default of 0 — the one
    // place in this module where the operation names its own preferred
    // exponent. The rounding machinery itself is untouched: the
    // `q_preferred` argument has been part of its contract since fd-42l.
    // `compound`'s value is positive for every in-domain input
    // (`1 + x > 0`, and integer powers preserve that), so the sign is
    // `false` by construction, and the coefficient is the value's
    // complete truth, so `pre_sticky` is `false`.
    Some(F::round_and_pack_finite(
        coef,
        exp,
        q_preferred,
        false,
        false,
        rm,
        Status::OK,
    ))
}

/// [`compound_exact_input`]'s integer core: `Some((coefficient,
/// exponent, preferred quantum))` for an exact or tie value, `None`
/// under the bail proofs listed on that function.
///
/// Split out so the classification is testable independently of a
/// format's rounder — the verification the ADR-0060 floors lean on is
/// exactly the completeness of this decision, so it gets its own
/// witnesses rather than only end-to-end ones.
fn compound_exact_parts<F: DecimalFormat>(x: F, n: i32) -> Option<(U256, i32, i32)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if coef.is_zero() {
        return None; // ±0 short-circuits at the caller's special cases
    }
    // A format coefficient fits u128 (≤ 34 digits); bail defensively.
    // Such a coefficient names no format input, so no case is lost.
    if coef.hi != 0 {
        return None;
    }
    // §9.2.2 reads the *stored* quantum, so this is computed before
    // stripping canonicalises the cohort away.
    let q_pref = preferred_quantum(exp, n);
    let (c, u) = strip_trailing_zeros(coef, exp);
    let a = c.lo;

    // The whole-range power-of-ten family, first (see the section above).
    if let Some(k) = one_plus_is_power_of_ten(a, u, sign) {
        let kn = i64::from(k) * i64::from(n);
        if kn.unsigned_abs() > POWER_OF_TEN_EXPONENT_LIMIT {
            return None; // proof at the bail-site list above
        }
        return Some((U256::from_u128(1), kn as i32, q_pref));
    }

    if sign && u >= 0 {
        // Unreachable in domain: a stripped `a ≥ 1` at `u ≥ 0` gives
        // `|x| ≥ 1`, and a negative `x` with `|x| ≥ 1` is `≤ −1`, which
        // the caller's special cases already disposed of. Bailing here
        // loses no exact or tie case.
        return None;
    }

    // `1 + x = N / 10^d`, with `e` the integer scale on the `u ≥ 0` side.
    // At most one of `d` and `e` is nonzero.
    let d = if u < 0 { u.unsigned_abs() } else { 0 };
    let e = if u > 0 { u.unsigned_abs() } else { 0 };
    if e > 0 && c.decimal_digit_count() + e > F::PRECISION + 1 {
        return None; // proof at the bail-site list above
    }
    if d > 2 * F::PRECISION + 2 {
        return None; // proof at the bail-site list above
    }
    // Both gates keep the shift inside `U256::mul_pow10`'s documented
    // `k ≤ 76` precondition (`2·34 + 2 = 70` at the widest format) and
    // the product inside the ~78-digit envelope.
    let pow10_d = U256::from_u128(1).mul_pow10(d);
    let n_big = if sign {
        pow10_d.sub(c)
    } else {
        pow10_d.add(c.mul_pow10(e))
    };

    let (coefficient, out_exp) = integer_power_parts::<F>(n_big, -(d as i32), n)?;
    Some((coefficient, out_exp, q_pref))
}

/// The exact `(coefficient, exponent)` of `(base · 10^scale)^n` for a
/// positive `base` and a nonzero `n`, or `None` when the value is
/// non-terminating, too wide to be representable or a nearest-mode
/// midpoint, or too far out of range for the format rounder's own
/// arithmetic.
///
/// The `pown`-on-a-rational core both `compound` classifiers share:
/// [`compound_exact_parts`] applies it to the exact base `1 + x`, and
/// [`compound_huge_x_anchor`] to `x` itself, which is exactly ADR-0060's
/// "compound reduces to `pown` on the exact rational `1 + x`" and its
/// companion "huge `x`, where `compound` hugs the classified `pown`
/// value".
///
/// Factor `base = 2^i · 5^j · t` with `gcd(t, 10) = 1`, so
/// `base · 10^scale = 2^α · 5^β · t` with `α = i + scale`,
/// `β = j + scale`, and the `n`-th power is `t^n · 2^(nα) · 5^(nβ)`.
/// With `A = nα`, `B = nβ` and `w = min(A, B)`, the returned
/// coefficient `t^n · 2^(A−w) · 5^(B−w)` is already stripped (at most
/// one of the 2- and 5-legs survives and `t` is coprime to 10), which
/// is what makes the closing `PRECISION + 1` digit gate a decisive
/// exact-or-midpoint test.
///
/// Bail sites, each provably losing no exact or midpoint value:
///
/// * `n < 0` with `t ≠ 1`: `1/t^|n|` keeps a prime other than 2 and 5
///   in its denominator, so the value is a non-terminating decimal —
///   neither representable nor a midpoint (midpoints terminate).
/// * the `du` / `dv` gates: `2^128` carries 39 digits and `5^56` carries
///   40, both past every format's `PRECISION + 1 ≤ 35`.
/// * `int_pow_u256` / `checked_mul_u256` envelope bails and the closing
///   digit gate: a stripped coefficient wider than `PRECISION + 1` is
///   neither representable nor a midpoint at any exponent.
/// * `|w|` past [`POWER_OF_TEN_EXPONENT_LIMIT`]: the value is hundreds
///   of thousands of decades outside every format's range, where the
///   §7.4 disposition no longer depends on where exactly it sits, and
///   the caller's `exp` gates answer it (that constant's rustdoc
///   carries the two-sided derivation, including why the format
///   rounder's `i32` arithmetic needs the bound).
fn integer_power_parts<F: DecimalFormat>(base: U256, scale: i32, n: i32) -> Option<(U256, i32)> {
    let (v2, rest) = factor_count_u256(base, 2);
    let (v5, t) = factor_count_u256(rest, 5);
    let t_is_one = t == U256::from_u128(1);
    if n < 0 && !t_is_one {
        return None; // non-terminating reciprocal
    }
    // `|α|, |β| ≤ log2(base) + |scale| < 6,300` and `|n| ≤ 2^31`, so the
    // products stay four orders inside `i64`.
    let alpha = i64::from(v2) + i64::from(scale);
    let beta = i64::from(v5) + i64::from(scale);
    let n64 = i64::from(n);
    let a_exp = n64 * alpha;
    let b_exp = n64 * beta;
    let w = a_exp.min(b_exp);
    let (du, dv) = (a_exp - w, b_exp - w); // at least one is zero
    if du > 127 || dv > 55 {
        return None;
    }
    let t_pow = if t_is_one {
        U256::from_u128(1)
    } else {
        int_pow_u256(t, n.unsigned_abs())?
    };
    let pow2 = int_pow_u256(U256::from_u128(2), du as u32)?;
    let pow5 = int_pow_u256(U256::from_u128(5), dv as u32)?;
    let coefficient = checked_mul_u256(checked_mul_u256(t_pow, pow2)?, pow5)?;
    if coefficient.decimal_digit_count() > F::PRECISION + 1 {
        return None;
    }
    if w.unsigned_abs() > POWER_OF_TEN_EXPONENT_LIMIT {
        return None;
    }
    Some((coefficient, w as i32))
}

/// The ADR-0051 anchor for `compound`'s huge-`x` family: `Some` is the
/// exact `(coefficient, exponent)` of `x^n`, the grid point the true
/// value `(1 + x)^n` hugs; `None` routes on. Caller
/// (`crate::compound::compound_kernel_body`) has run
/// `compound_special_cases` and `compound_exact_input` first, so every
/// `x` reaching here is finite, nonzero, above `−1`, and its
/// `compound` value is provably neither representable nor a midpoint.
///
/// ## The family, and why no rung can decide it
///
/// ADR-0060 names this the second of `compound`'s two whole-range
/// on-grid families (the first being `1 + x = 10^k`). Once `x` outgrows
/// the working width, `logp1`'s wide band forms `t = 1 ⊕ x` and the `1`
/// is absorbed entirely: the kernel is then evaluating `x^n`, not
/// `(1 + x)^n`. That is harmless for *accuracy* — the absorbed 1 is a
/// relative `1/x`, astronomically inside any budget — and fatal for
/// *decidability* whenever `x^n` is itself a format grid point, because
/// the true value then sits a relative `≈ n/x` above (or below) a
/// rounding boundary, a distance no fixed rung can grow. A default
/// build would decide the directed modes by the sign of its own noise,
/// and an `unbounded-ladder` build would widen until the rung passed
/// `x`'s own digit count.
///
/// Witnessed, before this classifier existed, by
/// `compound(1E+200, 1)`: the true value `10^200 + 1` is strictly above
/// the grid point `10^200`, so `TowardPositive` owes `next_up(10^200)`,
/// and the kernel delivered `10^200`. `compound(1E+2000, 1)`,
/// `compound(1E+200, 2)` and `compound(1E+2000, 3)` are the same shape.
/// This is the D1 `log10p1` integer-anchor lesson and the D2 `exp10`
/// integer-family lesson arriving a third time, in the place ADR-0060
/// predicted.
///
/// ## The gate, mirroring the tiny-`x` arm
///
/// With `adj` the adjusted exponent of `x` and `dn` the digit count of
/// `|n|`, the arm fires when `adj ≥ F::PRECISION + dn + 4` — the exact
/// reflection of `compound::compound_anchor`'s
/// `adj ≤ −(F::PRECISION + dn + 4)`. Soundness: `x ≥ 10^adj` and
/// `|n| < 10^dn` give `|n|/x < 10^(dn − adj) ≤ 10^(−P−4)`, and
/// `|(1 + 1/x)^n − 1| ≤ 1.01 · |n|/x` on that range, so the true value
/// sits within `≈ 10^(−P−4)` relative of the anchor. The nearest
/// rounding boundary beside a grid point (or beside a midpoint, on the
/// far side) is at least `5·10^(−P−1)` relative away, so the margin is
/// a factor of `5·10^3` where the discipline asks for ten. Anchor and
/// true value therefore lie strictly between the same two boundaries
/// and round identically in every direction; the residual channel
/// supplies the side, which is the only thing left to decide.
///
/// ## The side theorem
///
/// `x > 0` here (an `x` with `|x| ≥ 1` and `x > −1` is positive), so
/// `1 + x > x > 0` and `(1 + x)^n > x^n` exactly when `n > 0`. Strict
/// for every `n ≠ 0`, so `magnitude_grows = n > 0` at the call site.
///
/// ## Completeness of the `None`s
///
/// Declining here is always safe: it routes to the working-precision
/// path, which is the right decider for every `x^n` that is not a
/// boundary. The bails are [`integer_power_parts`]'s, and each one
/// proves `x^n` is neither representable nor a midpoint — precisely the
/// case where the anchor would not be a boundary and the ladder's
/// predicate has a real distance to measure.
pub(crate) fn compound_huge_x_anchor<F: DecimalFormat>(x: F, n: i32) -> Option<(U256, i32)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if sign || coef.is_zero() || coef.hi != 0 {
        return None;
    }
    let adj = exp + coef.decimal_digit_count() as i32 - 1;
    // `n ≠ 0` by the caller's special cases, so `ilog10` is defined.
    let n_digits = n.unsigned_abs().ilog10() as i32 + 1;
    if adj < F::PRECISION as i32 + n_digits + 4 {
        return None;
    }
    let (a, u) = strip_trailing_zeros(coef, exp);
    integer_power_parts::<F>(a, u, n)
}

// ----------------------------------------------------------------------------
// Input-side exact and tie classification for `hypot` (ADR-0060 Track D
// D3). The only two-operand member of this module, and the only one
// whose criterion is a perfect-square test rather than a power test.

/// Quadratic residues of a square modulo 16: `n²  mod 16` ranges over
/// `{0, 1, 4, 9}` as `n` ranges over a complete residue system, so an
/// `S` outside that set is not a square. Rejects 12 of 16 residue
/// classes, i.e. ~75% of non-squares at uniform residues.
const QR16: [bool; 16] = {
    let mut t = [false; 16];
    let mut n = 0usize;
    while n < 16 {
        t[(n * n) % 16] = true;
        n += 1;
    }
    t
};

/// Quadratic residues of a square modulo 25 (11 of the 25 classes).
/// Rejects ~56% of the non-squares that survive [`QR16`]; the two
/// filters together leave ~11% of uniform residues for the integer
/// square root.
const QR25: [bool; 25] = {
    let mut t = [false; 25];
    let mut n = 0usize;
    while n < 25 {
        t[(n * n) % 25] = true;
        n += 1;
    }
    t
};

/// `s mod m` for a small modulus, folding the limbs with Horner over
/// the limb radix `2^128`. `m ≤ 25` keeps every intermediate inside a
/// `u128` with room to spare (`r < m` and `r · (2^128 mod m) + limb
/// mod m < m²  + m ≤ 650`).
///
/// The low limb alone does *not* decide the residue for either
/// modulus used here: `2^128 ≡ 0 (mod 16)` makes it sufficient for 16,
/// but `2^128 ≡ 6 (mod 25)`, so the 25 filter must fold. Folding both
/// through one routine keeps the two filters visibly the same
/// argument.
fn u768_mod_small(s: U768, m: u128) -> u128 {
    let radix = (1u128 << 127) % m * 2 % m; // 2^128 mod m
    let mut r = 0u128;
    for limb in s.limbs.iter().rev() {
        r = (r * radix + limb % m) % m;
    }
    r
}

/// Strip trailing decimal zeros from a [`U768`], returning the
/// stripped value and the count removed. Bounded by the type's
/// ~231-digit envelope.
fn strip_trailing_zeros_u768(mut s: U768) -> (U768, u32) {
    if s.is_zero() {
        return (s, 0);
    }
    let mut k = 0u32;
    loop {
        let (q, r) = s.div_rem10();
        if r != 0 {
            return (s, k);
        }
        s = q;
        k += 1;
    }
}

/// The exact or tie value of `hypot(x, y)` decided from the operands
/// alone, for the ADR-0060 *kernel band* (magnitude ratio above the
/// anchor gate). `cw`/`qw` are the larger operand's magnitude
/// coefficient and quantum, `cz`/`qz` the smaller one's; both are
/// finite and nonzero and the caller has already dispatched every
/// special value. `None` routes to the approximation kernel.
///
/// ## The criterion
///
/// Align the two operands on the common quantum `q = min(qw, qz)`:
/// with `A = cw · 10^(qw − q)` and `B = cz · 10^(qz − q)` (integers,
/// exactly one of the two shifts nonzero),
///
/// > `hypot(x, y) = sqrt(A² + B²) · 10^q = sqrt(S) · 10^q`.
///
/// `S` is an exact integer. If `hypot(x, y)` is representable, or sits
/// exactly on a nearest-mode midpoint, its value is a terminating
/// decimal and hence rational, so `sqrt(S)` is rational; the square
/// root of a positive integer is either an integer or irrational
/// (Niven, *Irrational Numbers* —
/// docs/references/niven-irrational-numbers.md), so `S = W²` for an
/// integer `W` and the value is exactly `W · 10^q`.
///
/// ## Completeness (why every `None` is provably neither exact nor a tie)
///
/// Three bails, each closing under the same argument:
///
/// * **`S` is not a perfect square.** Then `sqrt(S)` is irrational by
///   Niven, so the value is irrational: neither representable nor a
///   midpoint (both are terminating decimals).
/// * **The trailing-zero count `k` of `S` is odd.** Write `W = W' ·
///   10^t` with `10 ∤ W'`. Then `S = W'² · 10^(2t)` and `10 ∤ W'²`
///   (`10 | W'²` would force `2 | W'` and `5 | W'`), so `S`'s
///   trailing-zero count is exactly `2t`, even. An odd `k` therefore
///   witnesses that `S` is not a square at all.
/// * **The stripped `S' = S / 10^k` is `≥ 10^(2P + 2)`.** A
///   representable value has at most `P` significant digits and a
///   nearest-mode midpoint exactly `P + 1` (its stripped coefficient
///   ends in 5); in the subnormal region the midpoint's stripped
///   coefficient is narrower still. Either way the value's stripped
///   coefficient `W'` is below `10^(P + 1)`, so `S' = W'²` is below
///   `10^(2P + 2)`. A wider `S'` cannot come from an exact or tie
///   value.
///
/// The band premise is what bounds the integers: the caller's ratio
/// gate leaves `adj(z) ≥ adj(w) − δ₀` with `δ₀ = ⌈(P + 2)/2⌉`, so the
/// alignment shift is at most `δ₀ + P − 1` on the wide side and
/// `P − 1` on the narrow side (derivation on
/// `crate::hypot::hypot_kernel`), keeping `A` inside `10^(δ₀ + 2P − 1)`
/// and `S` inside `U768`'s envelope. The two shift bails below restate
/// that bound defensively; they are unreachable from the gated caller.
///
/// ## Delivery
///
/// The value `W' · 10^(q + k/2)` goes to the format rounder with the
/// §9.2.2 preferred quantum `min(Q(x), Q(y)) = q`. On a representable
/// result that yields the exact value with `Status::OK` (§7.5 forbids
/// `INEXACT` on an exact result); on a midpoint the rounder's own tie
/// rule and directed-mode sides resolve it correctly, which the
/// approximation kernel cannot do for a value that *is* a rounding
/// boundary; and on a value past the format's range the §7.4
/// disposition answers every mode, exactly as `exp10_integer` relies
/// on for its above-range powers of ten.
pub(crate) fn hypot_exact_or_tie<F: DecimalFormat>(
    cw: U256,
    qw: i32,
    cz: U256,
    qz: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let q = qw.min(qz);
    // In-band shift bounds (`δ₀ + P − 1` wide side, `P − 1` narrow
    // side). Unreachable bails from the gated caller; kept so the
    // routine is total and never feeds `mul_pow10` past its envelope.
    let delta0 = (F::PRECISION + 2).div_ceil(2);
    let shift_w = u32::try_from(qw - q).ok()?;
    let shift_z = u32::try_from(qz - q).ok()?;
    if shift_w > delta0 + F::PRECISION - 1 || shift_z > F::PRECISION - 1 {
        return None;
    }
    // `A < 10^(δ₀ + 2P − 1)` (85 digits at Decimal128) and `B <
    // 10^(2P − 1)` (67), both inside `U384`'s ~115-digit envelope;
    // `S < 2 · 10^(2δ₀ + 4P − 2)` (171 digits) is inside `U768`'s ~231.
    let a = U384::from_u256(cw).mul_pow10(shift_w);
    let b = U384::from_u256(cz).mul_pow10(shift_z);
    let s = u384_mul_u384_to_u768(a, a).add(u384_mul_u384_to_u768(b, b));

    // Residue prefilters: a square is a quadratic residue modulo every
    // modulus, so a failure is a proof that `S` is not a square (and
    // hence, by Niven, that the value is irrational). Sound in one
    // direction only, which is the direction a bail needs.
    if !QR16[u768_mod_small(s, 16) as usize] || !QR25[u768_mod_small(s, 25) as usize] {
        return None;
    }

    let (s1, k) = strip_trailing_zeros_u768(s);
    if k % 2 != 0 {
        return None;
    }
    // Width gate, in two steps: the top three limbs decide `S' ≥
    // 2^384 > 10^(2P + 2)` without a digit walk, and the survivors
    // compare against the threshold in `U256`.
    if s1.limbs[2] != 0 || s1.limbs[3] != 0 || s1.limbs[4] != 0 || s1.limbs[5] != 0 {
        return None;
    }
    let s1_256 = U256 {
        lo: s1.limbs[0],
        hi: s1.limbs[1],
    };
    let width_gate = U256::from_u128(1).mul_pow10(2 * F::PRECISION + 2);
    if s1_256.cmp(width_gate) != Ordering::Less {
        return None;
    }
    // `S' < 10^(2P + 2) ≤ 10^70 < 2^234` satisfies `isqrt`'s documented
    // caller invariant (root inside `u128`).
    let (root, rem) = s1_256.isqrt();
    if !rem.is_zero() {
        return None;
    }
    // `S'` carries no trailing zero, so neither does `root` (`10 |
    // root` forces `10 | root²`): its width is its stripped width, and
    // the gate above put it below `10^(P + 1)`.
    let exp = q.checked_add(i32::try_from(k / 2).ok()?)?;
    Some(F::round_and_pack_finite(
        U256::from_u128(root),
        exp,
        q,
        false,
        false,
        rm,
        Status::OK,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(n: u128) -> U256 {
        U256::from_u128(n)
    }

    #[test]
    fn strip_trailing_zeros_canonicalises() {
        // 8000 × 10^-3 == 8 × 10^0.
        assert_eq!(strip_trailing_zeros(u(8000), -3), (u(8), 0));
        // 1 × 10^72 has no trailing zeros in the coefficient.
        assert_eq!(strip_trailing_zeros(u(1), 72), (u(1), 72));
        // 100 × 10^0 == 1 × 10^2.
        assert_eq!(strip_trailing_zeros(u(100), 0), (u(1), 2));
        // Zero is left alone.
        assert_eq!(strip_trailing_zeros(U256::ZERO, -5), (U256::ZERO, -5));
        // No trailing zero: unchanged.
        assert_eq!(strip_trailing_zeros(u(123), -2), (u(123), -2));
    }

    use crate::mock_format::ValueFmt128;

    fn v128(coef: u128, exp: i32) -> ValueFmt128 {
        ValueFmt128 {
            coef,
            exp,
            sign: false,
        }
    }

    #[test]
    fn cbrt_u128_finds_exact_roots() {
        assert_eq!(cbrt_u128(1), Some(1));
        assert_eq!(cbrt_u128(8), Some(2));
        assert_eq!(cbrt_u128(27), Some(3));
        assert_eq!(cbrt_u128(1_000_000), Some(100));
        // 12-digit root at the format ceiling: 999999999999³.
        let t = 999_999_999_999u128;
        assert_eq!(cbrt_u128(t * t * t), Some(t));
        // The u128 ceiling itself: ⌊u128::MAX^(1/3)⌋³ is found, and
        // u128::MAX (not a cube) is rejected without overflow.
        let top = 6_981_463_658_331u128;
        assert_eq!(cbrt_u128(top * top * top), Some(top));
        assert_eq!(cbrt_u128(u128::MAX), None);
    }

    #[test]
    fn cbrt_u128_rejects_non_cubes() {
        for c in [2u128, 7, 9, 26, 28, 100, 124, 126, 999_999_999_998] {
            assert_eq!(cbrt_u128(c), None, "{c} is not a cube");
        }
    }

    /// Cross-check the input-side decision against the retired
    /// post-hoc witness: wherever `cbrt_exact_input`'s number theory
    /// says "exact root `t · 10^(e/3)`", cubing that root must
    /// reproduce the input value exactly (`cube_is_exact`), and
    /// wherever it says "no", the witness must agree for every
    /// candidate rounding of the true root. Two independent proofs of
    /// the same boundary fact.
    #[test]
    fn cbrt_input_decision_matches_posthoc_witness() {
        // Exact: (input coef, input exp) -> (root coef, root exp).
        let exact = [
            (8u128, 0i32, 2u128, 0i32),
            (27, -3, 3, -1),
            (125, 3, 5, 1),
            (9261, 30, 21, 10),
            (1, 72, 1, 24),
            (912_673, -6, 97, -2),
        ];
        for (cx, ex, ct, et) in exact {
            let (c, e) = strip_trailing_zeros(U256::from_u128(cx), ex);
            assert_eq!(e % 3, 0, "{cx}e{ex}: exponent divisible by 3");
            let t = cbrt_u128(c.lo).expect("perfect cube");
            assert_eq!((t, e / 3), (ct, et), "{cx}e{ex}: root parts");
            assert!(
                cube_is_exact(v128(ct, et), v128(cx, ex)),
                "{cx}e{ex}: witness confirms t³ reproduces the input"
            );
        }
        // Not exact: coefficient a cube but exponent not divisible by
        // 3, and vice versa; the witness agrees the candidate root of
        // the nearest shape does not cube back.
        let (c, e) = strip_trailing_zeros(U256::from_u128(27), -2);
        assert_ne!(e % 3, 0, "0.27 fails the exponent test");
        assert!(!cube_is_exact(v128(3, -1), v128(27, -2)));
        let (c9, _) = strip_trailing_zeros(U256::from_u128(9), 0);
        assert_eq!(cbrt_u128(c9.lo), None, "9 fails the cube test");
        assert!(!cube_is_exact(v128(2, 0), v128(9, 0)));
        let _ = c;
    }

    #[test]
    fn nth_root_u128_finds_exact_roots() {
        assert_eq!(nth_root_u128(4, 2), Some(2));
        assert_eq!(nth_root_u128(9, 2), Some(3));
        assert_eq!(nth_root_u128(27, 3), Some(3));
        assert_eq!(nth_root_u128(1_419_857, 5), Some(17)); // 17^5
        assert_eq!(nth_root_u128(2u128.pow(127), 127), Some(2));
        // Largest square in u128.
        let s = 18_446_744_073_709_551_615u128; // 2^64 - 1
        assert_eq!(nth_root_u128(s * s, 2), Some(s));
    }

    #[test]
    fn nth_root_u128_rejects_non_powers() {
        assert_eq!(nth_root_u128(2, 2), None);
        assert_eq!(nth_root_u128(8, 2), None);
        assert_eq!(nth_root_u128(26, 3), None);
        assert_eq!(nth_root_u128(u128::MAX, 2), None);
        // b ≥ 128: only 1 is a b-th power, and t = 1 never reaches here.
        assert_eq!(nth_root_u128(2, 128), None);
        assert_eq!(nth_root_u128(u128::MAX, 200), None);
    }

    /// Cross-check the input-side pow decision against the retired
    /// post-hoc witness: for each classified-exact case, the delivered
    /// `(coef, exp)` raised back through `y` must reproduce the input
    /// (`power_is_exact`), and refused cases must fail the witness for
    /// the nearest candidate result. Two independent proofs of the
    /// same boundary fact.
    #[test]
    fn pow_input_decision_matches_posthoc_witness() {
        // (x coef, x exp, y coef, y exp, y neg) -> (result coef, exp).
        let exact: [(u128, i32, u128, i32, bool, u128, i32); 6] = [
            // pow(4, 0.5) = 2.
            (4, 0, 5, -1, false, 2, 0),
            // pow(16, -0.25) = 0.5.
            (16, 0, 25, -2, true, 5, -1),
            // pow(2.25, 0.5) = 1.5 (s = 3 > 1 path).
            (225, -2, 5, -1, false, 15, -1),
            // pow(10, 300) = 1E+300.
            (10, 0, 300, 0, false, 1, 300),
            // pow(0.2, 2) = 0.04.
            (2, -1, 2, 0, false, 4, -2),
            // pow(1.5, 3) = 3.375.
            (15, -1, 3, 0, false, 3375, -3),
        ];
        for (cx, ex, cy, ey, yneg, cr, er) in exact {
            let x = v128(cx, ex);
            let y = ValueFmt128 {
                coef: cy,
                exp: ey,
                sign: yneg,
            };
            assert!(
                power_is_exact(v128(cr, er), x, y),
                "pow({cx}e{ex}, ±{cy}e{ey}): witness confirms the classified result"
            );
        }
        // Refusals: irrational powers must fail the witness for the
        // nearby candidates the kernel could plausibly deliver.
        assert!(!power_is_exact(v128(2, 0), v128(3, 0), v128(5, -1)));
        assert!(!power_is_exact(v128(17, -1), v128(3, 0), v128(5, -1)));
    }

    #[test]
    fn cube_u256_small_values() {
        assert_eq!(cube_u256(u(2)), Some(u(8)));
        assert_eq!(cube_u256(u(3)), Some(u(27)));
        assert_eq!(cube_u256(u(10)), Some(u(1000)));
        // 12-digit input is in range: (10^12 - 1)³ < 10^36 fits U256.
        let c = u(999_999_999_999);
        let cubed = cube_u256(c).expect("12-digit cube fits");
        // Cross-check against an independent U256 multiply.
        let sq = checked_mul_u256(c, c).unwrap();
        assert_eq!(cube_u256(c), checked_mul_u256(sq, c));
        let _ = cubed;
    }

    #[test]
    fn cube_u256_bails_above_twelve_digits() {
        // 13-digit input: cannot be an exact cube root of a ≤34-digit value.
        assert_eq!(cube_u256(u(10_000_000_000_000)), None);
    }

    #[test]
    fn checked_mul_u256_detects_overflow() {
        // 40-digit × 40-digit = ~80 digits, exceeds U256 (~78).
        let big = U256::from_u128(1).mul_pow10(40);
        assert_eq!(checked_mul_u256(big, big), None);
        // 30-digit × 30-digit = ~60 digits, fits.
        let mid = U256::from_u128(1).mul_pow10(30);
        assert_eq!(
            checked_mul_u256(mid, mid),
            Some(U256::from_u128(1).mul_pow10(60))
        );
    }

    #[test]
    fn value_eq_ignores_cohort() {
        // 8 × 10^0 == 8000 × 10^-3 == 80 × 10^-1.
        assert!(value_eq(u(8), 0, u(8000), -3));
        assert!(value_eq(u(8000), -3, u(80), -1));
        // 27 × 10^0 == 27, but not 27 × 10^1.
        assert!(value_eq(u(27), 0, u(27), 0));
        assert!(!value_eq(u(27), 0, u(27), 1));
        // Different decades.
        assert!(!value_eq(u(2), 0, u(2), 3));
        // 1 × 10^72 == 10^72.
        assert!(value_eq(u(1), 72, U256::from_u128(1).mul_pow10(72), 0));
    }

    #[test]
    fn factor_count_counts_factors() {
        assert_eq!(factor_count(8, 2), 3);
        assert_eq!(factor_count(125, 5), 3);
        assert_eq!(factor_count(40, 2), 3); // 40 = 2^3 · 5
        assert_eq!(factor_count(40, 5), 1);
        assert_eq!(factor_count(7, 2), 0);
        assert_eq!(factor_count(1, 2), 0);
    }

    #[test]
    fn reduce_rational_integer_y() {
        // y = 3 (cy=3, ey=0) -> 3/1.
        assert_eq!(reduce_rational(3, 0), Some((3, 1)));
        // y = 300 (cy=3, ey=2) -> 300/1.
        assert_eq!(reduce_rational(3, 2), Some((300, 1)));
        // y = 1 -> 1/1.
        assert_eq!(reduce_rational(1, 0), Some((1, 1)));
    }

    #[test]
    fn reduce_rational_fractions() {
        // 0.5 = 5 × 10^-1 -> 1/2.
        assert_eq!(reduce_rational(5, -1), Some((1, 2)));
        // 0.25 = 25 × 10^-2 -> 1/4.
        assert_eq!(reduce_rational(25, -2), Some((1, 4)));
        // 0.2 = 2 × 10^-1 -> 1/5.
        assert_eq!(reduce_rational(2, -1), Some((1, 5)));
        // 1.5 = 15 × 10^-1 -> 3/2.
        assert_eq!(reduce_rational(15, -1), Some((3, 2)));
        // 0.123 = 123 × 10^-3 -> 123/1000 (123 shares no 2 or 5).
        assert_eq!(reduce_rational(123, -3), Some((123, 1000)));
        // 0.75 = 75 × 10^-2 -> 3/4.
        assert_eq!(reduce_rational(75, -2), Some((3, 4)));
    }

    #[test]
    fn reduce_rational_bails_on_overflow() {
        // ey large enough that 10^ey overflows u128 (>38 digits).
        assert_eq!(reduce_rational(1, 39), None);
        // Tiny fraction whose denominator 10^d overflows: d=40, cy coprime
        // to 10, so b = 10^40 overflows u128.
        assert_eq!(reduce_rational(3, -40), None);
    }

    #[test]
    fn int_pow_u256_basic() {
        assert_eq!(int_pow_u256(u(2), 0), Some(u(1)));
        assert_eq!(int_pow_u256(u(2), 10), Some(u(1024)));
        assert_eq!(int_pow_u256(u(5), 2), Some(u(25)));
        assert_eq!(int_pow_u256(u(10), 3), Some(u(1000)));
        // base 1 stays 1 for any exponent (the power-of-ten case).
        assert_eq!(int_pow_u256(u(1), 6000), Some(u(1)));
    }

    #[test]
    fn int_pow_u256_bails_on_overflow() {
        // 2^300 ≈ 10^90, far past U256's ~78 digits.
        assert_eq!(int_pow_u256(u(2), 300), None);
        // A 34-digit base squared twice already exceeds U256.
        let wide = U256::from_u128(1).mul_pow10(33); // 34 digits
        assert_eq!(int_pow_u256(wide, 4), None);
    }

    /// Cross-check the input-side `rsqrt` decision against the
    /// independent post-hoc witness: wherever the number theory says
    /// "exact value `t · 10^w`", squaring that value and multiplying by
    /// the input must reproduce exactly 1 (`rsqrt_is_exact`), and
    /// wherever it says "no", the witness must refuse the candidate the
    /// kernel would deliver. Two independent proofs of the same
    /// boundary fact.
    #[test]
    fn rsqrt_input_decision_matches_posthoc_witness() {
        // (x coef, x exp) -> (result coef, result exp).
        let exact: [(u128, i32, u128, i32); 10] = [
            (4, 0, 5, -1),       // rsqrt(4) = 0.5
            (4, -2, 5, 0),       // rsqrt(0.04) = 5
            (625, -2, 4, -1),    // rsqrt(6.25) = 0.4
            (25, -2, 2, 0),      // rsqrt(0.25) = 2
            (1, 0, 1, 0),        // rsqrt(1) = 1
            (1, -6, 1, 3),       // rsqrt(1E-6) = 1000
            (1, 72, 1, -36),     // rsqrt(1E+72) = 1E-36
            (16, 0, 25, -2),     // rsqrt(16) = 0.25
            (625, -4, 4, 0),     // rsqrt(0.0625) = 4
            (1024, 0, 3125, -5), // rsqrt(1024) = 0.03125
        ];
        for (cx, ex, cr, er) in exact {
            let (c, e) = strip_trailing_zeros(U256::from_u128(cx), ex);
            assert_eq!(
                rsqrt_exact_parts(c.lo, e),
                Some((cr, er)),
                "rsqrt({cx}e{ex}): classified parts"
            );
            assert!(
                rsqrt_is_exact(v128(cr, er), v128(cx, ex)),
                "rsqrt({cx}e{ex}): witness confirms x·y² = 1"
            );
        }
        // Refusals, one per bail site. `s ≠ 1` (a factor coprime to
        // ten survives), an odd power of two, an odd power of five, and
        // an odd power of ten; the witness agrees the candidate value
        // the kernel would deliver does not square back.
        for (cx, ex) in [(3u128, 0i32), (2, 0), (5, -1), (1, -1), (18, 0)] {
            let (c, e) = strip_trailing_zeros(U256::from_u128(cx), ex);
            assert_eq!(
                rsqrt_exact_parts(c.lo, e),
                None,
                "rsqrt({cx}e{ex}) is irrational"
            );
        }
        // 1/√2 truncated to 34 digits is not exact, and the witness
        // says so: the post-hoc check cannot be fooled by the value the
        // kernel actually delivers there.
        assert!(!rsqrt_is_exact(
            v128(7_071_067_811_865_475_244_008_443_621_048_490, -34),
            v128(2, 0)
        ));
    }

    /// The tie family, at all three real format widths: `5^d` of
    /// exactly `PRECISION + 1` digits ends in 5, so it is a genuine
    /// nearest mode midpoint, and its input `2^(2d)` is representable.
    /// Pinned here because the rounding of those midpoints is witnessed
    /// only in the per-format integration tests, and this is the proof
    /// that the inputs exist at all.
    #[test]
    fn rsqrt_tie_family_exists_at_every_format_width() {
        // (PRECISION, d, input exponent of two).
        for (precision, d) in [(34u32, 49u32), (16, 24), (7, 11)] {
            let pow5 = 5u128.pow(d);
            assert_eq!(
                U256::from_u128(pow5).decimal_digit_count(),
                precision + 1,
                "5^{d} is the PRECISION + 1 width at precision {precision}"
            );
            assert_eq!(pow5 % 10, 5, "a power of five ends in 5");
            // The input `2^(2d)` is representable at that precision.
            let input = 1u128 << (2 * d);
            assert!(
                U256::from_u128(input).decimal_digit_count() <= precision,
                "2^{} fits {precision} digits",
                2 * d
            );
            assert_eq!(
                rsqrt_exact_parts(input, 0),
                Some((pow5, -(d as i32))),
                "rsqrt(2^{}) = 5^{d}·10^-{d}",
                2 * d
            );
            assert!(rsqrt_is_exact(v128(pow5, -(d as i32)), v128(input, 0)));
        }
    }

    /// Cohorts of one value classify identically: the decision is made
    /// on the stripped form, and trailing zeros move the stored
    /// exponent by exactly the amount `strip_trailing_zeros` gives
    /// back. `4`, `4.0`, and `400E-2` are one value and one answer.
    #[test]
    fn rsqrt_classification_is_cohort_invariant() {
        for (cx, ex) in [(4u128, 0i32), (40, -1), (400, -2), (4_000_000, -6)] {
            let (c, e) = strip_trailing_zeros(U256::from_u128(cx), ex);
            assert_eq!(
                rsqrt_exact_parts(c.lo, e),
                Some((5, -1)),
                "rsqrt({cx}e{ex}) = 0.5 in every cohort"
            );
        }
    }

    /// The width bail on the `5^d` branch is reachable and correct:
    /// `2^112` is a representable 34-digit `Decimal128` coefficient
    /// whose exact `1/√x` is `5^56 · 10^-56`, a 40-digit coefficient
    /// past the `u128` envelope and past every format's
    /// `PRECISION + 1`. The parts helper must decline rather than wrap.
    #[test]
    fn rsqrt_wide_power_of_five_bails() {
        let input = 1u128 << 112;
        assert_eq!(
            U256::from_u128(input).decimal_digit_count(),
            34,
            "2^112 is a 34-digit coefficient"
        );
        assert_eq!(rsqrt_exact_parts(input, 0), None);
        // One step narrower is inside the envelope and classified.
        let input = 1u128 << 110;
        assert_eq!(rsqrt_exact_parts(input, 0), Some((5u128.pow(55), -55)));
    }

    /// The `powi` classifier's number theory, at the `Decimal128`
    /// shape: every result narrow enough to be a format value or a
    /// nearest-mode midpoint comes back as its exact `(coefficient,
    /// exponent)`, stripped.
    #[test]
    fn powi_exact_parts_finds_the_narrow_powers() {
        // (x coef, x exp, n) -> (result coef, result exp).
        let cases: [(u128, i32, i32, u128, i32); 12] = [
            (15, -1, 3, 3375, -3),           // 1.5^3 = 3.375
            (2, -1, 2, 4, -2),               // 0.2^2 = 0.04
            (2, 0, 3, 8, 0),                 // 2^3
            (2, 0, -3, 125, -3),             // 2^-3 = 0.125
            (2, 0, 112, 1 << 112, 0),        // the widest exact power of two
            (2, 0, 113, (1u128 << 113), 0),  // PRECISION + 1 digits
            (5, 0, 49, 5u128.pow(49), 0),    // the tie
            (2, 0, -49, 5u128.pow(49), -49), // its negative mirror
            (10, 0, 300, 1, 300),            // the power-of-ten family
            (1, 1, -300, 1, -300),           // ... at a stripped input
            (1, 0, i32::MAX, 1, 0),          // |x| = 1 at any exponent
            (1, 0, i32::MIN, 1, 0),          // ... including i32::MIN
        ];
        for (cx, ex, n, want_c, want_e) in cases {
            let got = powi_exact_parts::<ValueFmt128>(v128(cx, ex), n);
            assert_eq!(got, Some((u(want_c), want_e)), "powi({cx}e{ex}, {n}) parts");
        }
    }

    /// Every bail the classifier owes a completeness proof for, one
    /// case each: a non-terminating reciprocal, a coefficient past the
    /// `PRECISION + 1` gate, a `U256` overflow, and the 2- and 5-power
    /// width gates.
    #[test]
    fn powi_exact_parts_declines_what_it_cannot_express() {
        // 1/3^1 is a non-terminating rational: neither exact nor a tie.
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(3, 0), -1), None);
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(15, -1), -2), None);
        // 2^117 has 36 digits, one past the PRECISION + 1 gate.
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(2, 0), 117), None);
        // 3^100 has 48 digits; 3^500 overflows U256 inside int_pow_u256.
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(3, 0), 100), None);
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(3, 0), 500), None);
        // 2^-200 carries 5^200: past the du/dv width gates.
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(2, 0), -200), None);
    }

    /// The exponent window, and the reason it is a *soundness* bound:
    /// `round_and_pack_finite` computes `qmin − exp` in `i32`, so an
    /// exponent near `i32::MAX` wraps there. Inside the window the
    /// classifier delivers; outside it declines, and the kernel's
    /// `exp` gate owns the disposition (the true value's logarithm
    /// exceeds 230,000 past the window, well past every format's
    /// gate).
    #[test]
    fn powi_exact_parts_respects_the_exponent_window() {
        let window = i32::try_from(EXACT_EXPONENT_WINDOW).unwrap();
        // x = 10, so w = n exactly: the window edge and one past it.
        assert_eq!(
            powi_exact_parts::<ValueFmt128>(v128(10, 0), window),
            Some((u(1), window))
        );
        assert_eq!(
            powi_exact_parts::<ValueFmt128>(v128(10, 0), -window),
            Some((u(1), -window))
        );
        assert_eq!(
            powi_exact_parts::<ValueFmt128>(v128(10, 0), window + 1),
            None
        );
        assert_eq!(
            powi_exact_parts::<ValueFmt128>(v128(10, 0), -window - 1),
            None
        );
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(10, 0), i32::MAX), None);
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(10, 0), i32::MIN), None);
    }

    /// The exponent product is `i64` arithmetic, and this is the case
    /// that proves it: `x = 10^4096` with `n = 1048576` has
    /// `j·n = 2^32`, which an `i32` multiply wraps to exactly **zero**
    /// — the classifier would then deliver `1 × 10^0 = 1` for a value
    /// astronomically past `MAX`, with clean flags. A wrong answer
    /// that looks entirely plausible is the one this routine cannot
    /// afford, so the guard is a test rather than a comment.
    #[test]
    fn powi_exact_parts_forms_the_exponent_product_in_i64() {
        let n = 1_048_576i32;
        let j = 4096i32;
        assert_eq!(i64::from(j) * i64::from(n), 1i64 << 32);
        assert_eq!(j.wrapping_mul(n), 0, "the i32 product wraps to zero");
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(1, j), n), None);
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(1, -j), n), None);
        assert_eq!(powi_exact_parts::<ValueFmt128>(v128(1, j), -n), None);
    }

    /// Cohort insensitivity: the classifier reads the stripped form,
    /// so every cohort of a base produces the identical parts.
    #[test]
    fn powi_exact_parts_is_cohort_insensitive() {
        for n in [-3i32, -1, 1, 2, 6, 7] {
            let a = powi_exact_parts::<ValueFmt128>(v128(2, 0), n);
            for (c, e) in [(20u128, -1i32), (200, -2), (2_000_000, -6)] {
                assert_eq!(
                    powi_exact_parts::<ValueFmt128>(v128(c, e), n),
                    a,
                    "powi({c}e{e}, {n}) must match powi(2, {n})"
                );
            }
        }
    }

    /// The `rootn` criterion's own decisions, cross-checked against
    /// the retired post-hoc `pow` witness read in the inverse
    /// direction: the classifier says `rootn(x, n) = r`, and the
    /// witness independently confirms `x = r^n` (for `n < 0`,
    /// `x · r^|n| = 1`, which is the same statement). Two independent
    /// proofs of the same boundary fact.
    #[test]
    fn rootn_input_decision_matches_posthoc_witness() {
        // (x coef, x exp, n) -> (root coef, root exp).
        let exact: [(u128, i32, i32, u128, i32); 9] = [
            // rootn(8, 3) = 2 — cbrt's own family.
            (8, 0, 3, 2, 0),
            // rootn(1E+30, 5) = 1E+6.
            (1, 30, 5, 1, 6),
            // rootn(0.001, 3) = 0.1.
            (1, -3, 3, 1, -1),
            // rootn(32, 5) = 2, and its negative order 1/2.
            (32, 0, 5, 2, 0),
            (32, 0, -5, 5, -1),
            // rootn(1024, 10) = 2 (t = 1, α = 10).
            (1024, 0, 10, 2, 0),
            // rootn(2.25, 2) = 1.5 — the `s > 1` path (t = 9 = 3²).
            (225, -2, 2, 15, -1),
            // rootn(1, n) = 1, both signs of a large order.
            (1, 0, 1000, 1, 0),
            (1, 0, -1000, 1, 0),
        ];
        for (cx, ex, n, cr, er) in exact {
            let (coef, exp) = rootn_exact_parts(cx, ex, n)
                .unwrap_or_else(|| panic!("rootn({cx}e{ex}, {n}) must classify"));
            assert_eq!(
                (coef, exp),
                (u(cr), er),
                "rootn({cx}e{ex}, {n}): assembled parts"
            );
            // The witness: x == r^n, with the sign of `n` selecting
            // `power_is_exact`'s reciprocal branch.
            let n_datum = ValueFmt128 {
                coef: u128::from(n.unsigned_abs()),
                exp: 0,
                sign: n < 0,
            };
            assert!(
                power_is_exact(v128(cx, ex), v128(cr, er), n_datum),
                "rootn({cx}e{ex}, {n}): witness confirms x = r^n"
            );
        }
    }
    /// The criterion's refusals, one per bail. Each `None` is a value
    /// whose root is provably irrational (or a non-terminating
    /// reciprocal), so the kernel's unconditional `INEXACT` past the
    /// classifier is correct.
    #[test]
    fn rootn_criterion_refuses_the_irrationals() {
        // b ∤ α: 2 is not a cube (α = 1).
        assert_eq!(rootn_exact_parts(2, 0, 3), None);
        // b ∤ β: 5 is not a square (β = 1).
        assert_eq!(rootn_exact_parts(5, 0, 2), None);
        // b ∤ exponent: 8e1 = 80 = 2^4 · 5 is not a cube.
        assert_eq!(rootn_exact_parts(8, 1, 3), None);
        // t not a perfect b-th power: 9 = 3² is not a cube.
        assert_eq!(rootn_exact_parts(9, 0, 3), None);
        // n < 0 with s > 1: 1/3 does not terminate, though 9 IS a
        // perfect square — the bail that only the negative order needs.
        assert_eq!(rootn_exact_parts(9, 0, -2), None);
        // ... while the same value at positive order classifies.
        assert_eq!(rootn_exact_parts(9, 0, 2), Some((u(3), 0)));
        // |n| ≤ 1 is not this classifier's domain.
        for n in [-1, 0, 1] {
            assert_eq!(rootn_exact_parts(8, 0, n), None);
        }
        // A huge order over a value that is not 1: no root.
        assert_eq!(rootn_exact_parts(2, 0, i32::MAX), None);
        assert_eq!(rootn_exact_parts(2, 0, i32::MIN), None);
    }
    /// The tie asymmetry, pinned as arithmetic. Negative orders reach
    /// `PRECISION + 1` midpoints (`rootn(2^2k, −2) = 5^k · 10^−k`,
    /// whose coefficient ends in 5), positive orders provably cannot
    /// (the input carries the `b`-th power of the result's
    /// coefficient, so a `PRECISION + 1` digit result needs a
    /// `2·PRECISION + 1` digit input).
    #[test]
    fn rootn_ties_live_at_negative_orders_only() {
        // The three per-format tie inputs, and their exact values.
        // (input 2^2k, k, digits of 5^k = PRECISION + 1 of a format)
        for (k, want_digits) in [(50u32, 35u32), (24, 17), (11, 8)] {
            let x = 1u128 << (2 * k);
            let (coef, exp) = rootn_exact_parts(x, 0, -2)
                .unwrap_or_else(|| panic!("2^{} is a perfect square", 2 * k));
            assert_eq!((coef, exp), (u(5u128.pow(k)), -(k as i32)));
            assert_eq!(
                coef.decimal_digit_count(),
                want_digits,
                "5^{k} is the PRECISION + 1 width of its format"
            );
            // A midpoint's coefficient ends in 5.
            assert_eq!(coef.div_rem10().1, 5);
            // The witness: x · r^2 = 1, i.e. r = x^(−1/2).
            let n_datum = ValueFmt128 {
                coef: 2,
                exp: 0,
                sign: true,
            };
            assert!(power_is_exact(
                v128(x, 0),
                v128(5u128.pow(k), -(k as i32)),
                n_datum
            ));
        }
        // The positive-order side of the same family: `rootn(x, 2)`
        // delivers the square root's coefficient, whose square is the
        // input's — so the input, not the output, is the wide one.
        let (coef, exp) = rootn_exact_parts(1u128 << 100, 0, 2).expect("2^100 is a perfect square");
        assert_eq!((coef, exp), (u(1u128 << 50), 0));
        assert_eq!(coef.decimal_digit_count(), 16);
    }
    /// `|x| = 1` is answered exactly for every order, including the
    /// extremes of `i32` on both signs — the fact the kernel's
    /// hug-at-1 arm leans on for its strict side theorem.
    #[test]
    fn rootn_classifies_one_at_every_order() {
        for n in [2, 3, 7, 1_000_000, i32::MAX, -2, -3, -7, i32::MIN] {
            assert_eq!(
                rootn_exact_parts(1, 0, n),
                Some((u(1), 0)),
                "rootn(1, {n}) must classify as exactly 1"
            );
        }
        // A cohort of 1 strips to the same parts, so it lands here too.
        let (c, e) = strip_trailing_zeros(u(1_000_000), -6);
        assert_eq!((c.lo, e), (1, 0));
    }

    #[test]
    fn factor_count_u256_splits_twos_and_fives() {
        assert_eq!(factor_count_u256(u(1), 2), (0, u(1)));
        assert_eq!(factor_count_u256(u(8), 2), (3, u(1)));
        assert_eq!(factor_count_u256(u(125), 5), (3, u(1)));
        assert_eq!(factor_count_u256(u(20), 2), (2, u(5)));
        assert_eq!(factor_count_u256(u(21), 2), (0, u(21)));
        // Past the u128 envelope: 2^130 splits completely.
        let big = int_pow_u256(u(2), 130).expect("2^130 fits U256");
        assert_eq!(factor_count_u256(big, 2), (130, u(1)));
    }
    #[test]
    fn one_plus_is_power_of_ten_finds_the_nines_patterns() {
        // x = 9, 99, 999 → 1 + x = 10^1, 10^2, 10^3.
        assert_eq!(one_plus_is_power_of_ten(9, 0, false), Some(1));
        assert_eq!(one_plus_is_power_of_ten(99, 0, false), Some(2));
        assert_eq!(one_plus_is_power_of_ten(999, 0, false), Some(3));
        // x = −0.9, −0.99 → 1 + x = 10^−1, 10^−2.
        assert_eq!(one_plus_is_power_of_ten(9, -1, true), Some(-1));
        assert_eq!(one_plus_is_power_of_ten(99, -2, true), Some(-2));
        // Nines run present but the exponent does not match the run.
        assert_eq!(one_plus_is_power_of_ten(99, -1, true), None);
        assert_eq!(one_plus_is_power_of_ten(9, -2, true), None);
        // Not a nines run at all.
        assert_eq!(one_plus_is_power_of_ten(19, 0, false), None);
        assert_eq!(one_plus_is_power_of_ten(1, 0, false), None);
        // Positive x with a nonzero stripped exponent: `N ≡ 1 (mod 10)`.
        assert_eq!(one_plus_is_power_of_ten(9, 1, false), None);
        assert_eq!(one_plus_is_power_of_ten(9, -1, false), None);
    }
    #[test]
    fn preferred_quantum_follows_the_stored_exponent() {
        // §9.2.2: floor(n × min(0, Q(x))).
        assert_eq!(preferred_quantum(-2, 3), -6); // compound(0.05, 3)
        assert_eq!(preferred_quantum(0, 2), 0); // compound(5, 2) = 36
        assert_eq!(preferred_quantum(3, 2), 0); // Q(x) > 0 clamps to 0
        assert_eq!(preferred_quantum(-2, -1), 2); // compound(0.25, −1)
                                                  // Saturating rather than overflowing at the corners.
        assert_eq!(preferred_quantum(-6176, i32::MAX), i32::MIN);
        assert_eq!(preferred_quantum(-6176, i32::MIN), i32::MAX);
    }
    /// The machine-checked leg of [`compound_exact_input`]'s
    /// `d > 2·PRECISION + 2` bail (ADR-0060's derivation-plus-probe
    /// posture, at this classifier's scale).
    ///
    /// For `n < 0` an exact or tie result forces `t = 1`, i.e. the
    /// numerator `N` of `1 + x = N/10^d` is a bare power of two or a
    /// bare power of five. The analytic argument bounds those by
    /// `2^((P+1)/log10 5)` and `5^((P+1)/log10 2)` respectively; the
    /// first is far below `10^(2P+2)` outright, and this test closes the
    /// second by enumerating it. Claim verified here: across the whole
    /// reachable exponent range, no power of two and no power of five
    /// sits within `10^PRECISION` of a power of ten at any `d > PRECISION`
    /// — so `N = 10^d ± a` with `a` a format coefficient is never such a
    /// power once `d` passes `PRECISION`, let alone `2·PRECISION + 2`.
    ///
    /// A failure here means the gate can drop an exact or tie case and
    /// the bail's proof is void: widen the gate, do not weaken the test.
    #[test]
    fn compound_negative_n_terminating_bases_stay_narrow() {
        use std::vec::Vec;

        // `N` as little-endian decimal digits, so the enumeration runs
        // past `U256`'s 78-digit envelope (`5^127` carries 89) without
        // borrowing a wider carrier for a test-only walk.
        fn times(digits: &mut Vec<u8>, base: u8) {
            let mut carry = 0u8;
            for d in digits.iter_mut() {
                let v = *d * base + carry;
                *d = v % 10;
                carry = v / 10;
            }
            while carry > 0 {
                digits.push(carry % 10);
                carry /= 10;
            }
        }

        for precision in [34usize, 16, 7] {
            // The negative-`n` coefficient gates, at the loosest `|n| = 1`
            // (a larger `|n|` only shrinks them); `127` and `55` are the
            // classifier's own `du` / `dv` constants, used here so the
            // enumeration covers everything the code can reach.
            //   N = 2^i needs 5^i < 10^(P+1)  → the `dv > 55` gate
            //   N = 5^j needs 2^j < 10^(P+1)  → the `du > 127` gate
            for (base, cap) in [(2u8, 55usize), (5u8, 127)] {
                let mut n_digits: Vec<u8> = Vec::new();
                n_digits.push(1);
                for e in 1..=cap {
                    times(&mut n_digits, base);
                    let len = n_digits.len();
                    // Only two `d` can put `N` within `10^P` of `10^d`,
                    // since `10^(len−1) ≤ N < 10^len`; any other `d` is
                    // a whole decade away, far past `10^P`.
                    //
                    // `d = len` is the `x < 0` shape `N = 10^d − a`:
                    // within `10^P` iff every digit above position `P`
                    // is a 9.
                    if len > precision {
                        assert!(
                            !n_digits[precision..len].iter().all(|&d| d == 9),
                            "P={precision}: {base}^{e} sits within a format \
                             coefficient below 10^{len} (d > P) — the \
                             `d > 2·PRECISION + 2` bail in \
                             `compound_exact_parts` can drop an exact or \
                             tie case; widen the gate, do not weaken this \
                             test"
                        );
                    }
                    // `d = len − 1` is the `x > 0` shape `N = 10^d + a`:
                    // within `10^P` iff the leading digit is 1 and
                    // everything above position `P` below it is 0.
                    if len > precision + 1 {
                        let leading_one_then_zeros = n_digits[len - 1] == 1
                            && n_digits[precision..len - 1].iter().all(|&d| d == 0);
                        assert!(
                            !leading_one_then_zeros,
                            "P={precision}: {base}^{e} sits within a format \
                             coefficient above 10^{} (d > P) — the \
                             `d > 2·PRECISION + 2` bail in \
                             `compound_exact_parts` can drop an exact or \
                             tie case; widen the gate, do not weaken this \
                             test",
                            len - 1
                        );
                    }
                }
            }
        }
    }
    /// The classifier core's decisions on the shapes the derivation
    /// names, at `Decimal128`'s precision through the mock format.
    /// Asserted on the integer triple, upstream of any format rounder.
    #[test]
    fn compound_exact_parts_decides_the_named_shapes() {
        use crate::mock_format::ValueFmt128;
        let val = |coef: u128, exp: i32, sign: bool| ValueFmt128 { coef, exp, sign };
        let parts = |c: u128, e: i32, s: bool, n: i32| compound_exact_parts(val(c, e, s), n);

        // (1.05)^3 = 1.157625, exact; §9.2.2 quantum 3 × min(0, −2) = −6.
        assert_eq!(parts(5, -2, false, 3), Some((u(1_157_625), -6, -6)));
        // (1 + 9)^2 = 100 through the power-of-ten family; quantum 0.
        assert_eq!(parts(9, 0, false, 2), Some((u(1), 2, 0)));
        // (1 − 0.99)^2 = 0.0001, the negative nines family; quantum −4.
        assert_eq!(parts(99, -2, true, 2), Some((u(1), -4, -4)));
        // (1 + 99)^5 = 10^10, coefficient 1 at exponent k·n = 2 × 5.
        assert_eq!(parts(99, 0, false, 5), Some((u(1), 10, 0)));
        // (1 + 0.25)^−1 = 0.8: the terminating reciprocal, t = 1.
        assert_eq!(parts(25, -2, false, -1), Some((u(8), -1, 2)));
        // (1 + 19)^2 = 400: N = 20 is divisible by 10, exercising the
        // `min(v2, v5) > 0` shape the `d = 0` branch admits.
        assert_eq!(parts(19, 0, false, 2), Some((u(4), 2, 0)));
        // (1 + 5)^2 = 36 with `Q(x) = 0`: preferred exponent 0.
        assert_eq!(parts(5, 0, false, 2), Some((u(36), 0, 0)));

        // (1 + 0.5)^−1 = 2/3: t = 3 ≠ 1, non-terminating; declined.
        assert_eq!(parts(5, -1, false, -1), None);
        // The exponent gate on the power-of-ten family: a million is
        // delivered, a million and one is not, and everything declined
        // is past the shared `exp` gates by two orders of magnitude.
        assert_eq!(parts(9, 0, false, 1_000_000), Some((u(1), 1_000_000, 0)));
        assert_eq!(parts(9, 0, false, -1_000_000), Some((u(1), -1_000_000, 0)));
        assert_eq!(parts(9, 0, false, 1_000_001), None);
        assert_eq!(parts(99, 0, false, 500_001), None);
        assert_eq!(parts(9, 0, false, i32::MAX), None);
        assert_eq!(parts(9, 0, false, i32::MIN), None);
        // Every representable exponent stays inside it, both gate-gap
        // integers included.
        assert_eq!(parts(9, 0, false, 6145), Some((u(1), 6145, 0)));
        assert_eq!(parts(9, -1, true, 6176), Some((u(1), -6176, -6176)));
        // Too wide to be exact or a tie: 1.05^40 needs 21^40.
        assert_eq!(parts(5, -2, false, 40), None);
        // The `u ≥ 1` width gate: N = 10^40 + 1 exceeds PRECISION + 1.
        assert_eq!(parts(1, 40, false, 1), None);

        // The ties, both signs of `n`: 5^49 and 5^50 carry exactly
        // PRECISION + 1 = 35 digits, and 2^−49 mirrors the first.
        let five49 = int_pow_u256(u(5), 49).expect("5^49 fits U256");
        let five50 = int_pow_u256(u(5), 50).expect("5^50 fits U256");
        assert_eq!(five49.decimal_digit_count(), 35);
        assert_eq!(parts(4, 0, false, 49), Some((five49, 0, 0)));
        assert_eq!(parts(4, 0, false, 50), Some((five50, 0, 0)));
        assert_eq!(parts(1, 0, false, -49), Some((five49, -49, 0)));
        // One past the tie window the coefficient is 36 digits: declined.
        assert_eq!(parts(4, 0, false, 51), None);
    }
    /// The huge-`x` anchor: the exact `x^n` grid point the true value
    /// hugs, and the gate that separates the family from the band the
    /// ladder decides on its own.
    #[test]
    fn compound_huge_x_anchor_finds_the_pown_grid_point() {
        use crate::mock_format::ValueFmt128;
        let val = |coef: u128, exp: i32, sign: bool| ValueFmt128 { coef, exp, sign };
        let anchor = |c: u128, e: i32, n: i32| compound_huge_x_anchor(val(c, e, false), n);

        // x = 10^200: x^n = 10^(200n), coefficient 1.
        assert_eq!(anchor(1, 200, 1), Some((u(1), 200)));
        assert_eq!(anchor(1, 200, 2), Some((u(1), 400)));
        assert_eq!(anchor(1, 2000, 3), Some((u(1), 6000)));
        assert_eq!(anchor(1, 200, -1), Some((u(1), -200)));
        // A base that is not a power of ten: 3 · 10^200, and its
        // terminating reciprocal 1/3·10^-200 does NOT exist, so the
        // negative direction declines there.
        assert_eq!(anchor(3, 200, 1), Some((u(3), 200)));
        assert_eq!(anchor(3, 200, -1), None);
        // 2 · 10^200 does have a terminating reciprocal: 5 · 10^-201.
        assert_eq!(anchor(2, 200, -1), Some((u(5), -201)));

        // The gate: `adj ≥ PRECISION + digits(n) + 4` = 39 at a
        // one-digit `n`, and 48 at a ten-digit one.
        assert_eq!(anchor(1, 39, 1), Some((u(1), 39)));
        assert_eq!(anchor(1, 38, 1), None);
        assert_eq!(anchor(1, 48, i32::MAX), None); // 10^48^i32::MAX is past the limit
        assert_eq!(anchor(1, 47, 1_000_000_000), None); // gate: adj < 34 + 10 + 4

        // A negative `x` never reaches this family (`x > −1` bounds it),
        // and neither does a zero.
        assert_eq!(compound_huge_x_anchor(val(1, 200, true), 1), None);
        assert_eq!(compound_huge_x_anchor(val(0, 0, false), 1), None);

        // Past the exponent limit the classifier declines and the
        // shared `exp` saturation gate answers instead.
        assert_eq!(anchor(1, 6000, 1000), None);
    }

    #[test]
    fn value_is_one_detects_unit_value() {
        use crate::extended::u256_mul_u256;
        // 100 × 10^-2 == 1.
        assert!(value_is_one(U384::from_u128(100), -2));
        // 1 × 10^0 == 1.
        assert!(value_is_one(U384::from_u128(1), 0));
        // 25 × 4 × 10^-2 == 1 (the pow(4,-0.5) shape).
        let prod = u256_mul_u256(u(25), u(4));
        assert!(value_is_one(prod, -2));
        // 100 × 10^-1 == 10, not 1.
        assert!(!value_is_one(U384::from_u128(100), -1));
        // 2 × 10^0 == 2, not 1.
        assert!(!value_is_one(U384::from_u128(2), 0));
        // Zero is not 1.
        assert!(!value_is_one(U384::ZERO, 0));
    }
}
