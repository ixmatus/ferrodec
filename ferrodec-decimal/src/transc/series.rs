//! Rectangular (Paterson-Stockmeyer) evaluation of the `atanh` power series
//! shared by the logarithm kernel and the constant computations.
//!
//! Both `ln` (via [`super::ln`]) and the constants `ln 2` / `ln 10` (via
//! [`super::consts`]) reduce to the sum
//!
//! ```text
//! S = sum_{k>=0} z^k / (2k+1)
//! ```
//!
//! for a [`Work`] argument `z` with `|z| < 1`, since `atanh(w) = w * S` with
//! `z = w^2`. Summed term by term the series costs one full-width `Work`
//! multiply per term to advance `z^k`, so a kernel evaluation runs in roughly
//! `O(wp^3)`: the dominant cost at high precision (ADR-0043).
//!
//! Paterson-Stockmeyer splitting evaluates the same polynomial with
//! `O(sqrt(N))` full multiplies instead of `O(N)`. Pick a block size `s`, write
//! `k = i*s + j`, and group:
//!
//! ```text
//! S = sum_i (z^s)^i * B_i,   B_i = sum_{j<s} z^j / (2(i*s+j)+1).
//! ```
//!
//! Precompute `z^0..z^{s-1}` once (`s-1` full multiplies) and `z^s`; each block
//! `B_i` is then a sum of those precomputed powers scaled by a divide-by-small-
//! integer (cheap, `O(wp)` each); the blocks recombine by a Horner recurrence in
//! `z^s` (`t` full multiplies). With `s ~ sqrt(N)` the full-multiply count drops
//! to about `2*sqrt(N)`. Derived from the series; see Paterson and Stockmeyer,
//! "On the number of nonscalar multiplications necessary to evaluate
//! polynomials" (1973), and Brent and Zimmermann, *Modern Computer Arithmetic*,
//! 4.4.3, for the method (not transcribed).
//!
//! This evaluator is for the logarithm's *value* series, where `z = w^2` is an
//! arbitrary full-precision argument and advancing `z^k` costs a full-width
//! multiply, so cutting the multiply count is the win. The *constant* series
//! (`ln 2` / `ln 10`, in [`super::consts`]) is a different shape: there `z` is a
//! small rational `1/m^2` and the per-term step is a division by the integer
//! `m^2`, already linear in the digit count, so rectangular splitting would only
//! add full-width multiplies and is not used there. (Binary splitting, which
//! needs the small-integer term ratio the constants have, would accelerate them
//! and is a possible follow-up.)
//!
//! Below a small term count the term-by-term loop wins (the precomputed powers
//! are not yet amortized) and is kept; the split engages only above the
//! threshold, which is set from the ADR-0043 bench.

use super::work::Work;
use alloc::vec::Vec;
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// Term count below which the term-by-term loop beats the split (its precomputed
/// powers are not yet amortized). Tuned against the ADR-0043 bench: at this
/// threshold the precision-16 logarithm stays on the simple path (no regression)
/// while precision 34 and up take the split (a clear win).
const RECT_MIN_TERMS: usize = 48;

/// `S = sum_{k>=0} z^k / (2k+1)` to `ip` significant digits, for `|z| < 1`.
///
/// `atanh(w) = w * S` with `z = w^2`; the logarithm kernel and the `ln 2` /
/// `ln 10` constants are both built on this sum. Chooses the term-by-term loop
/// at low precision and Paterson-Stockmeyer splitting above the threshold.
pub(super) fn atanh_sum(z: &Work, ip: u32) -> Work {
    if z.is_zero() {
        return Work::one(); // only the k = 0 term survives
    }
    // Precondition: |z| <= 1/9 (the atanh series argument is z = w^2 with
    // |w| <= 1/3). 0.1112 is just above 1/9 = 0.1111...; the bound is what makes
    // the term-count estimate below safe in the boundary decade.
    debug_assert!(
        z.cmp_magnitude(&Work::new(false, DecBig::from_u32(1112), -4)) != Ordering::Greater,
        "atanh_sum: |z| must be at most 1/9"
    );

    // A safe upper bound on the non-negligible term count, used to size the
    // split. Terms are z^k/(2k+1) <= |z|^k, and |z| < 10^(adj+1), so |z|^k drops
    // below 10^(-bound) once k*(-(adj+1)) >= bound. In the boundary decade
    // (adj == -1) that bound is vacuous, so there the contract |z| <= 1/9 gives
    // a decay of at least 0.95 digits per term (1/0.95 = 20/19 terms per digit).
    // The 1/(2k+1) factor only shrinks the terms faster, so this over-counts;
    // the +6 margin absorbs the rounding tail.
    let adj = z.adj_exp();
    let bound = i64::from(ip) + 6;
    let n_terms = if adj <= -2 {
        (bound / (-adj - 1)) as usize + 6
    } else {
        (bound * 20 / 19) as usize + 6
    };
    if n_terms < RECT_MIN_TERMS {
        atanh_sum_simple(z, ip)
    } else {
        atanh_sum_rect(z, ip, n_terms)
    }
}

/// The term-by-term sum, used below the split threshold.
fn atanh_sum_simple(z: &Work, ip: u32) -> Work {
    let mut zpow = Work::one(); // z^k, starting at z^0
    let mut sum = Work::one(); // the k = 0 term, z^0 / 1
    let mut k: i64 = 1;
    let max_iter = i64::from(ip) * 4 + 16;
    while k <= max_iter {
        zpow = zpow.mul_to(z, ip);
        let term = zpow.div_to(&Work::from_i64(2 * k + 1), ip);
        let negligible = term.is_zero() || sum.adj_exp() - i64::from(ip) - 2 > term.adj_exp();
        sum = sum.add(&term, ip);
        if negligible {
            break;
        }
        k += 1;
    }
    sum
}

/// Paterson-Stockmeyer split sum over `n_terms` terms, a safe upper bound on the
/// non-negligible count (the extra high-order terms are below the window and
/// contribute nothing at `ip` digits). About `2*sqrt(n_terms)` full multiplies.
fn atanh_sum_rect(z: &Work, ip: u32, n_terms: usize) -> Work {
    let s = n_terms.isqrt().max(1);
    let t = n_terms.div_ceil(s);

    // Precompute the block powers z^0 .. z^{s-1}, then z^s.
    let mut zpows: Vec<Work> = Vec::with_capacity(s);
    zpows.push(Work::one());
    for _ in 1..s {
        let next = zpows.last().expect("non-empty").mul_to(z, ip);
        zpows.push(next);
    }
    let zs = zpows.last().expect("non-empty").mul_to(z, ip);

    // Horner over blocks, most significant (smallest terms) first.
    let mut acc = Work::new(false, DecBig::zero(), 0);
    for i in (0..t).rev() {
        let mut block = Work::new(false, DecBig::zero(), 0);
        for (j, zpow) in zpows.iter().enumerate() {
            let denom = 2 * (i * s + j) as i64 + 1;
            block = block.add(&zpow.div_to(&Work::from_i64(denom), ip), ip);
        }
        acc = acc.mul_to(&zs, ip).add(&block, ip);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The split path (via [`atanh_sum`]) must agree with the self-terminating
    /// term-by-term sum on in-contract arguments (`|z| <= 1/9`), at a precision
    /// high enough to force the split and a slowly converging `z = 1/9` that
    /// needs the most terms. Catches a term-count estimate that truncates early.
    #[test]
    fn rect_matches_simple() {
        let ip = 300u32;
        let cmp_p = 270u32; // below the working precision, so tails do not matter
        for (num, den) in [(1i64, 9i64), (1, 25), (1, 81), (1, 10_000)] {
            let z = Work::from_i64(num).div_to(&Work::from_i64(den), ip);
            let mut simple = atanh_sum_simple(&z, ip);
            let mut split = atanh_sum(&z, ip); // dispatches to the split at this ip
            simple.normalize_to(cmp_p);
            split.normalize_to(cmp_p);
            assert_eq!(simple.coeff, split.coeff, "coeff {num}/{den}");
            assert_eq!(simple.exp, split.exp, "exp {num}/{den}");
        }
    }

    /// `2 * atanh(1/3) = ln 2`: a known-value check on the sum, independent of
    /// the constant cache. The first 30 significant digits of `ln 2` are
    /// `0.693147180559945309417232121458...`.
    #[test]
    fn two_atanh_third_is_ln2() {
        let ip = 50;
        let z = Work::one().div_to(&Work::from_i64(9), ip); // (1/3)^2
        let s = atanh_sum(&z, ip);
        let third = Work::one().div_to(&Work::from_i64(3), ip);
        let atanh = third.mul_to(&s, ip);
        let mut ln2 = atanh.add(&atanh, ip);
        ln2.normalize_to(30); // truncate to the leading 30 digits
        assert_eq!(
            ln2.coeff.to_u128(),
            Some(693_147_180_559_945_309_417_232_121_458)
        );
    }
}
