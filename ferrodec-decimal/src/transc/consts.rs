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

/// `atanh(1/m) = sum_{k>=0} (1/m)^(2k+1) / (2k+1)` at internal precision `ip`,
/// for a small integer `m`. Terms shrink by `1/m^2` each step, so the loop
/// stops once a term falls entirely below the `ip`-digit window of the sum.
///
/// This keeps the term-by-term loop rather than the rectangular splitting the
/// logarithm's value series uses: here the per-term step is a division by the
/// small integer `m^2` (linear in the digit count), not a full-width multiply,
/// so it is already cheap and rectangular splitting would only add full-width
/// multiplies. Binary splitting (the small-rational accelerator) would beat this
/// at very high precision and is a possible follow-up.
fn atanh_recip(m: u32, ip: u32) -> Work {
    let m_sq = Work::from_i64(i64::from(m) * i64::from(m));
    // The `k = 0` term is `1/m`; `power` tracks `(1/m)^(2k+1)`.
    let mut power = Work::one().div_to(&Work::from_i64(i64::from(m)), ip);
    let mut sum = power.clone();
    let mut k: i64 = 1;
    // Bound the iteration count generously; the geometric decay terminates the
    // loop long before this for any `m >= 3`.
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
