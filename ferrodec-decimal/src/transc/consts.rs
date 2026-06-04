//! The transcendental constants `ln 2`, `ln 10`, and `1 / ln 10`, computed on
//! demand at a requested working precision by a Machin-like `atanh` series, and
//! a small [`ConstCache`] that memoizes the highest precision computed so far.
//! No stored constant table and no global mutable state, so the surface stays
//! `no_std` clean.
//!
//! # Derivation
//!
//! `atanh(z) = z + z^3/3 + z^5/5 + ... = sum_{k>=0} z^(2k+1) / (2k+1)`, and
//! `2*atanh(z) = ln((1+z)/(1-z))`. Choosing `z` so the logarithm's argument is
//! the wanted ratio gives fast-converging series for small `z`:
//!
//! - `(1+z)/(1-z) = 2` at `z = 1/3`, so `ln 2 = 2*atanh(1/3)` (ratio `1/9` per
//!   paired term).
//! - `(1+z)/(1-z) = 5/4` at `z = 1/9`, so `ln(5/4) = 2*atanh(1/9)`, and since
//!   `10 = 8 * 5/4`, `ln 10 = 3*ln 2 + 2*atanh(1/9)` (ratio `1/81`).
//!
//! `1 / ln 10` is then `1 / ln10` to the working precision (it is also
//! `log10(e)`, the scale that turns a natural log into a base-ten log).
//!
//! Derived fresh from the `atanh` identity and the General Decimal Arithmetic
//! precision model; see Muller, *Elementary Functions*, for the range-reduction
//! and series-evaluation framing.

use super::work::Work;
use ferrodec_multiword::DecBig;

/// Extra digits carried beyond a caller's requested precision so a constant is
/// accurate to well below one unit in the last place of that precision: it
/// absorbs both the series truncation and the accumulated per-term rounding.
const CONST_GUARD: u32 = 12;

/// Memoizes `ln 2`, `ln 10`, and `1 / ln 10` at the highest internal precision
/// requested so far. Threaded through a kernel call by value (no global state);
/// a request at a precision already covered reuses the stored value, a request
/// beyond it recomputes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ConstCache {
    ln2: Option<(u32, Work)>,
    ln10: Option<(u32, Work)>,
    inv_ln10: Option<(u32, Work)>,
}

impl ConstCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `ln 2`, accurate to well under one ulp at `wp` significant digits.
    pub(crate) fn ln2(&mut self, wp: u32) -> Work {
        let ip = wp + CONST_GUARD;
        if self.ln2.as_ref().is_none_or(|(have, _)| *have < ip) {
            self.ln2 = Some((ip, compute_ln2(ip)));
        }
        self.ln2.as_ref().expect("just set").1.clone()
    }

    /// `ln 10`, accurate to well under one ulp at `wp` significant digits.
    pub(crate) fn ln10(&mut self, wp: u32) -> Work {
        let ip = wp + CONST_GUARD;
        if self.ln10.as_ref().is_none_or(|(have, _)| *have < ip) {
            self.ln10 = Some((ip, compute_ln10(ip)));
        }
        self.ln10.as_ref().expect("just set").1.clone()
    }

    /// `1 / ln 10` (`= log10(e)`), accurate to well under one ulp at `wp`.
    pub(crate) fn inv_ln10(&mut self, wp: u32) -> Work {
        let ip = wp + CONST_GUARD;
        if self.inv_ln10.as_ref().is_none_or(|(have, _)| *have < ip) {
            let ln10 = self.ln10(ip);
            self.inv_ln10 = Some((ip, Work::one().div_to(&ln10, ip)));
        }
        self.inv_ln10.as_ref().expect("just set").1.clone()
    }
}

/// `ln 2 = 2 * atanh(1/3)` at internal precision `ip`.
fn compute_ln2(ip: u32) -> Work {
    let a = atanh_recip(3, ip);
    a.add(&a, ip)
}

/// `ln 10 = 3 * ln 2 + 2 * atanh(1/9)` at internal precision `ip`.
fn compute_ln10(ip: u32) -> Work {
    let three_ln2 = compute_ln2(ip).mul(&Work::from_i64(3));
    let a9 = atanh_recip(9, ip);
    three_ln2.add(&a9.add(&a9, ip), ip)
}

/// Term count at or above which `atanh(1/m)` uses binary splitting rather than
/// the term-by-term loop. Tuned against the ADR-0043 bench.
const BS_MIN_TERMS: u64 = 32;

/// `atanh(1/m) = (1/m) * sum_{k>=0} (1/m^2)^k / (2k+1)` at internal precision
/// `ip`, for a small integer `m`. The series argument `1/m^2` is a small
/// rational, so above a term-count threshold this uses binary splitting
/// (`atanh_recip_bs`); below it the term-by-term loop is cheaper and is kept.
fn atanh_recip(m: u32, ip: u32) -> Work {
    let n_terms = atanh_recip_terms(m, ip);
    if n_terms >= BS_MIN_TERMS {
        atanh_recip_bs(m, ip, n_terms)
    } else {
        atanh_recip_simple(m, ip)
    }
}

/// A safe upper bound on the non-negligible term count of the `atanh(1/m)`
/// series at `ip` digits. Terms shrink by `1/m^2` each step, so `(1/m^2)^k`
/// falls below `10^-(ip+slack)` once `k` exceeds `(ip+slack)/log10(m^2)`. The
/// `m = 3` case (`m^2 = 9`) sits in the boundary decade, so it uses the
/// `1/0.95 = 20/19` ratio (`log10(9) > 0.95`); larger `m` use the digit-count
/// lower bound on the decay.
fn atanh_recip_terms(m: u32, ip: u32) -> u64 {
    let d = DecBig::from_u32(m * m).decimal_digit_count(); // digits of m^2
    let bound = u64::from(ip) + 6;
    if d >= 2 {
        bound / (d - 1) + 6
    } else {
        bound * 20 / 19 + 6
    }
}

/// The term-by-term sum, used below the binary-splitting threshold. The per-term
/// step is a division by the small integer `m^2` (linear in the digit count),
/// so at low precision it beats the splitting's recursion overhead.
fn atanh_recip_simple(m: u32, ip: u32) -> Work {
    let m_sq = Work::from_i64(i64::from(m) * i64::from(m));
    // The `k = 0` term is `1/m`; `power` tracks `(1/m)^(2k+1)`.
    let mut power = Work::one().div_to(&Work::from_i64(i64::from(m)), ip);
    let mut sum = power.clone();
    let mut k: i64 = 1;
    let max_iter = i64::from(ip) * 4 + 16;
    while k <= max_iter {
        power = power.div_to(&m_sq, ip);
        let term = power.div_to(&Work::from_i64(2 * k + 1), ip);
        let negligible = window_floor(&sum, ip) > leading_power(&term);
        sum = sum.add(&term, ip);
        if negligible {
            break;
        }
        k += 1;
    }
    sum
}

/// `atanh(1/m)` by binary splitting over `n_terms` terms (a safe upper bound on
/// the non-negligible count). The series `S = sum_{k>=0} 1/((2k+1) M^k)` with
/// `M = m^2` is a small-rational hypergeometric series, so the partial sum is an
/// exact ratio of big integers `(Q + T) / Q` (the `k = 0` term is the `1`), and
/// `atanh(1/m) = (1/m) * S = (Q + T) / (m * Q)` is one final divide at `ip`
/// digits. Computing `T` and `Q` by a balanced product tree costs
/// `O(M(D) log D)` rather than the loop's `O(D^2)`. Derived from the series; see
/// Haible and Papanikolaou, "Fast multiprecision evaluation of series of
/// rational numbers" (1998), and Brent and Zimmermann, *Modern Computer
/// Arithmetic*, 4.9, for the method (not transcribed).
fn atanh_recip_bs(m: u32, ip: u32, n_terms: u64) -> Work {
    let m_sq = DecBig::from_u128(u128::from(m) * u128::from(m));
    // P, Q, T over [1, n_terms); the k = 0 term (value 1) is added back as Q.
    let (_p, q, t) = bs_split(1, n_terms, &m_sq);
    let num = q.add(&t); // Q * (1 + T/Q) = Q + T = Q * S
    let den = q.mul(&DecBig::from_u32(m)); // m * Q
    Work::new(false, num, 0).div_to(&Work::new(false, den, 0), ip)
}

/// The Haible-Papanikolaou `(P, Q, T)` triple for the `atanh(1/m)` series over
/// the index range `[a, b)`, with `a >= 1` and the per-term ratio
/// `c_k / c_{k-1} = (2k-1) / (m^2 * (2k+1))`. `P` is the product of the
/// numerators `2k-1`, `Q` the product of the denominators `m^2*(2k+1)`, and `T`
/// the unnormalized partial sum, so `sum_{k=a}^{b-1} c_k = T / Q` relative to
/// `c_0 = 1`. The merge is `T = T_left * Q_right + P_left * T_right`.
fn bs_split(a: u64, b: u64, m_sq: &DecBig) -> (DecBig, DecBig, DecBig) {
    if b - a == 1 {
        let k = u128::from(a);
        let p = DecBig::from_u128(2 * k - 1);
        let q = m_sq.mul(&DecBig::from_u128(2 * k + 1));
        (p.clone(), q, p) // single term: T = p_a
    } else {
        let mid = a + (b - a) / 2;
        let (p_l, q_l, t_l) = bs_split(a, mid, m_sq);
        let (p_r, q_r, t_r) = bs_split(mid, b, m_sq);
        let t = t_l.mul(&q_r).add(&p_l.mul(&t_r));
        (p_l.mul(&p_r), q_l.mul(&q_r), t)
    }
}

/// The power of ten of a nonzero `Work`'s leading digit.
fn leading_power(w: &Work) -> i64 {
    w.exp + w.digits() - 1
}

/// The power of ten just below the `ip`-digit window of `sum`: a term whose
/// leading digit sits below this contributes nothing to the `ip`-digit result.
fn window_floor(sum: &Work, ip: u32) -> i64 {
    leading_power(sum) - i64::from(ip) - 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The leading significant digits of a positive `Work` value below 10, as a
    /// digit string (no decimal point). For `0.6931...` returns `"6931..."`.
    fn sig_digits(w: &Work) -> alloc::string::String {
        assert!(!w.sign && !w.coeff.is_zero());
        w.coeff.to_string()
    }

    // Reference significant digits (more than the precision under test).
    const LN2: &str = "69314718055994530941723212145817656807550013436025525412068000949339";
    const LN10: &str = "23025850929940456840179914546843642076011014886287729760333279009675";
    const LOG10E: &str = "43429448190325182765112891891660508229439700580366656611445378316586";

    #[test]
    fn ln2_matches_reference() {
        let v = compute_ln2(60);
        let got = sig_digits(&v);
        // Compare the leading 48 digits, leaving the guard tail out of the check.
        assert_eq!(&got[..48], &LN2[..48], "ln2 got {got}");
    }

    #[test]
    fn ln10_matches_reference() {
        let v = compute_ln10(60);
        let got = sig_digits(&v);
        assert_eq!(&got[..48], &LN10[..48], "ln10 got {got}");
    }

    #[test]
    fn inv_ln10_matches_reference() {
        let mut cache = ConstCache::new();
        let v = cache.inv_ln10(48);
        let got = sig_digits(&v);
        assert_eq!(&got[..48], &LOG10E[..48], "1/ln10 got {got}");
    }

    #[test]
    fn bs_matches_simple() {
        // Binary splitting and the term-by-term loop must agree to the working
        // precision, for m = 3 (m^2 = 9, the boundary decade) and m = 9 (81),
        // at a precision high enough to force the split.
        for m in [3u32, 9] {
            let ip = 200u32;
            let mut simple = atanh_recip_simple(m, ip);
            let mut bs = atanh_recip_bs(m, ip, atanh_recip_terms(m, ip));
            simple.normalize_to(180);
            bs.normalize_to(180);
            assert_eq!(simple.coeff, bs.coeff, "coeff m={m}");
            assert_eq!(simple.exp, bs.exp, "exp m={m}");
        }
    }

    #[test]
    fn cache_reuses_and_grows() {
        let mut cache = ConstCache::new();
        let small = cache.ln2(20);
        // A request within the cached precision reuses (same stored digits).
        let again = cache.ln2(10);
        assert_eq!(small.coeff, again.coeff);
        // A larger request grows the cache: more digits.
        let big = cache.ln2(80);
        assert!(big.digits() > small.digits());
        assert_eq!(&sig_digits(&big)[..48], &LN2[..48]);
    }

    #[test]
    fn product_of_inv_ln10_and_ln10_is_one() {
        // 1/ln10 * ln10 == 1 to the working precision.
        let mut cache = ConstCache::new();
        let ln10 = cache.ln10(40);
        let inv = cache.inv_ln10(40);
        let prod = inv.mul_to(&ln10, 40);
        // Round to 30 digits: 0.999...9 or 1.000...0; check the leading digits.
        let s = sig_digits(&prod);
        assert!(
            s.starts_with("999999999999999999999999999")
                || s.starts_with("100000000000000000000000000"),
            "product near one: {s}"
        );
    }
}
