//! Exact-result detection for `cbrt`, `pow`, `exp2`, `log2`, and
//! `log10` (IEEE 754-2019 §7.5).
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
//! special inputs short-circuit. Five functions are the exceptions:
//! `exp2(n)` with `2^n` in reach of the width gate, `log2(2^k)`,
//! `log10(10^k)` (fd-aqs.8), a perfect cube under `cbrt`, and an
//! exact rational power under `pow` (the decimal Lauter–Lefèvre
//! criterion). Every one is decided from the *input alone* and
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
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::U256;

#[cfg(test)]
use crate::extended::Extended;
#[cfg(test)]
use core::cmp::Ordering;
#[cfg(test)]
use ferrodec_multiword::U384;

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
// criterion below). Delivery through [`pack_value`] then yields the
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
    // `n = c · 10^e`; anything past `limit` (≤ 200 for every caller)
    // cannot be exact, so coarse bails are fine.
    if e > 3 || c.lo > limit {
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
/// standard rational-power argument; Niven, *Irrational Numbers*,
/// ch. 2). A midpoint's stripped coefficient has at most
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
/// * `w` outside `i32`: `|w| > 2^31` puts the value astronomically
///   past every format's range — over/underflow territory with no
///   boundary structure (ties live within the representable range
///   plus one quantum).
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
    let exp = i32::try_from(w).ok()?;
    Some(pack_value(coef, exp, false, rm))
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
