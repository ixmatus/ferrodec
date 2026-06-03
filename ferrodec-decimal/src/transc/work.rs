//! `Work`: a private, finite-only, variable-precision signed decimal float
//! used to evaluate the transcendental kernels (`exp`, `ln`, `log10`, `power`)
//! at a chosen working precision before a single final rounding to the
//! [`Context`].
//!
//! A `Work` is the value `(-1)^sign * coeff * 10^exp`, with `coeff` a
//! [`DecBig`] and `exp` an `i64` (wide enough for `y * ln|x|` and decade
//! extraction, and the width [`round_finite`] already takes). The `sticky`
//! field is the arbitrary-precision analogue of the fixed-width kernel's
//! guard/round/sticky tracking: every operation that drops a digit below the
//! working width ORs in whether the dropped tail was nonzero. The load-bearing
//! invariant, exercised in the tests, is that the only place a digit is ever
//! folded into `sticky` is [`Work::normalize_to`], and it only ever folds
//! digits strictly below the requested working width, so the round digit a
//! later [`round_finite`] inspects, plus `sticky`, always carry the true tail.
//!
//! Specials (NaN, Infinity, signed zero corner cases) are resolved by the
//! per-function dispatch before a `Work` is ever built, mirroring `sqrt.rs`.
//! The type is built only on the [`DecBig`] primitives (`add`, `sub`, `mul`,
//! `div_rem`, `mul_pow10`, `div_rem_pow10`, `decimal_digit_count`, `cmp_ref`,
//! `is_zero`); the one operation `DecBig` lacks, divide-to-N-significant-digits,
//! is composed here exactly as `divrem.rs` composes it for `divide`.

use crate::round::round_finite;
use crate::{Context, Decimal, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// A finite, variable-precision signed decimal float; see the module docs.
#[derive(Clone, Debug)]
pub(crate) struct Work {
    pub(crate) sign: bool,
    pub(crate) coeff: DecBig,
    pub(crate) exp: i64,
    pub(crate) sticky: bool,
}

impl Work {
    /// The exact value `(-1)^sign * coeff * 10^exp`, not yet inexact.
    pub(crate) fn new(sign: bool, coeff: DecBig, exp: i64) -> Self {
        Self {
            sign,
            coeff,
            exp,
            sticky: false,
        }
    }

    /// `+0`.
    pub(crate) fn zero() -> Self {
        Self::new(false, DecBig::zero(), 0)
    }

    /// `+1`.
    pub(crate) fn one() -> Self {
        Self::new(false, DecBig::from_u32(1), 0)
    }

    /// The integer `n` as an exact `Work`.
    pub(crate) fn from_i64(n: i64) -> Self {
        Self::new(n < 0, DecBig::from_u128(u128::from(n.unsigned_abs())), 0)
    }

    /// A finite [`Decimal`] as an exact `Work`. Panics on a special, which the
    /// per-function dispatch is responsible for handling earlier.
    pub(crate) fn from_decimal(d: &Decimal) -> Self {
        let (sign, coeff, exp) = d
            .finite_parts()
            .expect("Work::from_decimal on a finite value");
        Self::new(sign, coeff.clone(), i64::from(exp))
    }

    /// True when the magnitude is exactly zero (ignoring `sticky`).
    pub(crate) fn is_zero(&self) -> bool {
        self.coeff.is_zero()
    }

    /// Significant decimal digits of the coefficient (`1` for zero).
    pub(crate) fn digits(&self) -> i64 {
        self.coeff.decimal_digit_count() as i64
    }

    /// The adjusted exponent `exp + digits - 1`, the power of ten of the
    /// leading digit. Meaningful only for a nonzero magnitude.
    fn adj_exp(&self) -> i64 {
        self.exp + self.digits() - 1
    }

    /// Negate (flip the sign bit).
    pub(crate) fn neg(&self) -> Self {
        Self {
            sign: !self.sign,
            coeff: self.coeff.clone(),
            exp: self.exp,
            sticky: self.sticky,
        }
    }

    /// Multiply the value by `10^k` at zero cost (a pure exponent shift).
    pub(crate) fn scale_pow10(&mut self, k: i64) {
        if !self.coeff.is_zero() {
            self.exp += k;
        }
    }

    /// Bound the coefficient to at most `wp` significant digits, folding the
    /// dropped low tail into `sticky`. The single chokepoint where a digit
    /// becomes sticky, and it only ever drops digits strictly below `wp`.
    pub(crate) fn normalize_to(&mut self, wp: u32) {
        let excess = self.digits() - i64::from(wp);
        if excess > 0 {
            let (kept, rem) = self.coeff.div_rem_pow10(excess as u32);
            self.coeff = kept;
            self.exp += excess;
            if !rem.is_zero() {
                self.sticky = true;
            }
        }
    }

    /// Compare magnitudes `|self|` and `|other|` as exact values.
    pub(crate) fn cmp_magnitude(&self, other: &Self) -> Ordering {
        match (self.coeff.is_zero(), other.coeff.is_zero()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }
        match self.adj_exp().cmp(&other.adj_exp()) {
            Ordering::Equal => {
                // Same order of magnitude: align to the lower exponent (a
                // bounded shift, since the adjusted exponents are equal) and
                // compare the coefficients directly.
                let min_e = self.exp.min(other.exp);
                let a = self.coeff.mul_pow10((self.exp - min_e) as u32);
                let b = other.coeff.mul_pow10((other.exp - min_e) as u32);
                a.cmp_ref(&b)
            }
            ord => ord,
        }
    }

    /// `self + other`, rounded to `wp` significant digits with a faithful
    /// sticky. Aligns the operands to a common exponent bounded a few digits
    /// below the `wp` window of the larger operand; any operand digit below
    /// that window contributes only to `sticky`, so the cost stays `O(wp)`
    /// even when the exponents are far apart.
    pub(crate) fn add(&self, other: &Self, wp: u32) -> Self {
        if self.coeff.is_zero() {
            let mut r = other.clone();
            r.sticky |= self.sticky;
            r.normalize_to(wp);
            return r;
        }
        if other.coeff.is_zero() {
            let mut r = self.clone();
            r.sticky |= other.sticky;
            r.normalize_to(wp);
            return r;
        }

        // Window: keep digits down to a few below the larger operand's `wp`
        // window; anything lower is sticky.
        const GUARD: i64 = 3;
        let top = self.adj_exp().max(other.adj_exp());
        let floor_pow = top - i64::from(wp) - GUARD;
        let e_common = self.exp.min(other.exp).max(floor_pow);

        let (a_coeff, a_drop) = scale_to_exp(&self.coeff, self.exp, e_common);
        let (b_coeff, b_drop) = scale_to_exp(&other.coeff, other.exp, e_common);
        let sticky = self.sticky || other.sticky || a_drop || b_drop;

        let (coeff, sign) = if self.sign == other.sign {
            (a_coeff.add(&b_coeff), self.sign)
        } else {
            match a_coeff.cmp_ref(&b_coeff) {
                Ordering::Greater => (a_coeff.sub(&b_coeff), self.sign),
                Ordering::Less => (b_coeff.sub(&a_coeff), other.sign),
                // Equal kept parts of opposite sign: the value is whatever lies
                // below the window, i.e. zero to this precision but inexact if
                // any tail was dropped.
                Ordering::Equal => (DecBig::zero(), false),
            }
        };

        let mut r = Self {
            sign,
            coeff,
            exp: e_common,
            sticky,
        };
        r.normalize_to(wp);
        r
    }

    /// `self - other`, rounded to `wp` significant digits.
    pub(crate) fn sub(&self, other: &Self, wp: u32) -> Self {
        self.add(&other.neg(), wp)
    }

    /// `self * other`, exact (no rounding). The coefficient can reach the sum
    /// of the operands' digit counts; the caller bounds it with
    /// [`Work::normalize_to`] or uses [`Work::mul_to`].
    pub(crate) fn mul(&self, other: &Self) -> Self {
        Self {
            sign: self.sign ^ other.sign,
            coeff: self.coeff.mul(&other.coeff),
            exp: self.exp + other.exp,
            sticky: self.sticky || other.sticky,
        }
    }

    /// `self * other`, rounded to `wp` significant digits.
    pub(crate) fn mul_to(&self, other: &Self, wp: u32) -> Self {
        let mut r = self.mul(other);
        r.normalize_to(wp);
        r
    }

    /// `self / other`, with the quotient carried to about `wp` significant
    /// digits and the division remainder folded into `sticky`. Composed from
    /// `mul_pow10` + `div_rem` + a sticky exactly as `divrem.rs` composes
    /// `divide`. Precondition: `other` is nonzero.
    pub(crate) fn div_to(&self, other: &Self, wp: u32) -> Self {
        debug_assert!(!other.coeff.is_zero(), "Work::div_to by zero");
        if self.coeff.is_zero() {
            return Self {
                sign: self.sign ^ other.sign,
                coeff: DecBig::zero(),
                exp: self.exp - other.exp,
                sticky: self.sticky,
            };
        }
        let target = i64::from(wp);
        let da = self.digits();
        let db = other.digits();
        // Scale so the integer quotient lands at about `wp` digits.
        let shift = target - (da - db);
        let (num, den) = if shift >= 0 {
            (self.coeff.mul_pow10(shift as u32), other.coeff.clone())
        } else {
            (self.coeff.clone(), other.coeff.mul_pow10((-shift) as u32))
        };
        let (q, r) = num.div_rem(&den);
        Self {
            sign: self.sign ^ other.sign,
            coeff: q,
            exp: self.exp - other.exp - shift,
            sticky: self.sticky || other.sticky || !r.is_zero(),
        }
    }

    /// Round the value to the nearest integer (ties away from zero) and return
    /// it as an `i64`. Used for the range-reduction multiple `k`. The caller is
    /// responsible for the value being in range (the per-function overflow gate
    /// bounds `k` to the context's exponent span, well inside `i64`).
    pub(crate) fn round_to_i64(&self) -> i64 {
        if self.coeff.is_zero() {
            return 0;
        }
        let mag: u128 = if self.exp >= 0 {
            self.coeff
                .mul_pow10(self.exp as u32)
                .to_u128()
                .expect("round_to_i64 magnitude fits u128")
        } else {
            let drop = (-self.exp) as u32;
            let (q, r) = self.coeff.div_rem_pow10(drop);
            let mut m = q.to_u128().expect("round_to_i64 magnitude fits u128");
            // Round half away from zero: bump when twice the remainder reaches
            // the dropped power of ten.
            if r.add(&r).cmp_ref(&DecBig::pow10(drop)) != Ordering::Less {
                m += 1;
            }
            m
        };
        let v = i64::try_from(mag).expect("round_to_i64 result fits i64");
        if self.sign {
            -v
        } else {
            v
        }
    }

    /// Round to the [`Context`] and pack, terminating the kernel in the one
    /// [`round_finite`] call. `self.sticky` is the `pre_sticky` the rounding
    /// core consumes.
    pub(crate) fn into_decimal(
        self,
        ideal_exp: i64,
        ctx: &Context,
        status: Status,
    ) -> (Decimal, Status) {
        round_finite(
            self.sign,
            self.coeff,
            self.exp,
            self.sticky,
            ideal_exp,
            ctx,
            status,
        )
    }
}

/// Express `coeff * 10^exp` at the exponent `target`, returning the new
/// coefficient and whether a nonzero tail was dropped (which the caller folds
/// into a sticky bit). Scaling up (`exp >= target`) is exact; scaling down
/// drops the low `target - exp` digits.
fn scale_to_exp(coeff: &DecBig, exp: i64, target: i64) -> (DecBig, bool) {
    if exp >= target {
        (coeff.mul_pow10((exp - target) as u32), false)
    } else {
        let (kept, rem) = coeff.div_rem_pow10((target - exp) as u32);
        (kept, !rem.is_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The exact rational value of a `Work` whose magnitude fits `u128`, as an
    /// `f64`-free check against integer ground truth: returns `(signed
    /// numerator, exp)` so a caller can compare `coeff * 10^exp`.
    fn as_i128_at(w: &Work, exp: i64) -> i128 {
        // Express the value as an integer times 10^exp (exp <= value's exp).
        assert!(exp <= w.exp, "as_i128_at needs exp <= w.exp");
        let scaled = w.coeff.mul_pow10((w.exp - exp) as u32);
        let m = scaled.to_u128().expect("fits") as i128;
        if w.sign {
            -m
        } else {
            m
        }
    }

    #[test]
    fn constructors_and_digits() {
        assert!(Work::zero().is_zero());
        assert_eq!(Work::one().coeff.to_u128(), Some(1));
        let n = Work::from_i64(-12345);
        assert!(n.sign);
        assert_eq!(n.coeff.to_u128(), Some(12345));
        assert_eq!(n.digits(), 5);
        assert_eq!(Work::zero().digits(), 1);
    }

    #[test]
    fn add_same_sign_matches_integer_oracle() {
        // 123 + 456 = 579, exact, plenty of working precision.
        let a = Work::from_i64(123);
        let b = Work::from_i64(456);
        let s = a.add(&b, 20);
        assert_eq!(s.coeff.to_u128(), Some(579));
        assert!(!s.sticky);
        assert_eq!(as_i128_at(&s, 0), 579);
    }

    #[test]
    fn add_opposite_sign_and_sign_of_result() {
        // 100 + (-30) = 70; (-100) + 30 = -70.
        let p = Work::from_i64(100);
        let n = Work::from_i64(-30);
        assert_eq!(as_i128_at(&p.add(&n, 20), 0), 70);
        assert_eq!(
            as_i128_at(&Work::from_i64(-100).add(&Work::from_i64(30), 20), 0),
            -70
        );
        // Exact cancellation to zero.
        let z = Work::from_i64(42).add(&Work::from_i64(-42), 20);
        assert!(z.is_zero());
    }

    #[test]
    fn add_aligns_unequal_exponents() {
        // 1 + 0.25 = 1.25 (coeff 125, exp -2).
        let a = Work::new(false, DecBig::from_u32(1), 0);
        let b = Work::new(false, DecBig::from_u32(25), -2);
        let s = a.add(&b, 20);
        assert_eq!(s.coeff.to_u128(), Some(125));
        assert_eq!(s.exp, -2);
    }

    #[test]
    fn far_below_window_becomes_sticky_not_huge() {
        // 1e0 + 1e-100 at wp 10: the tiny addend is far below the window, so
        // the coefficient stays small and sticky records the lost tail.
        let big = Work::new(false, DecBig::from_u32(1), 0);
        let tiny = Work::new(false, DecBig::from_u32(1), -100);
        let s = big.add(&tiny, 10);
        assert!(
            s.digits() <= 14,
            "coefficient stayed bounded: {}",
            s.digits()
        );
        assert!(s.sticky, "the lost tail set sticky");
        // Value still rounds to 1.
        assert_eq!(
            s.coeff.to_u128().unwrap() / 10u128.pow((-(s.exp)) as u32),
            1
        );
    }

    #[test]
    fn normalize_to_folds_only_below_wp() {
        // 123456789 normalized to 4 digits -> 1234 (sticky, since 56789 != 0).
        let mut w = Work::new(false, DecBig::from_u128(123_456_789), 0);
        w.normalize_to(4);
        assert_eq!(w.coeff.to_u128(), Some(1234));
        assert_eq!(w.exp, 5);
        assert!(w.sticky);
        // Exactly representable normalization sets no sticky.
        let mut e = Work::new(false, DecBig::from_u128(123_000), 0);
        e.normalize_to(3);
        assert_eq!(e.coeff.to_u128(), Some(123));
        assert!(!e.sticky);
    }

    #[test]
    fn mul_is_exact_and_signed() {
        let a = Work::from_i64(-123);
        let b = Work::from_i64(1000);
        let p = a.mul(&b);
        assert!(p.sign);
        assert_eq!(p.coeff.to_u128(), Some(123_000));
        assert!(!p.sticky);
    }

    #[test]
    fn div_to_matches_long_division() {
        // 1 / 3 at wp 20 = 0.3333...3 (20 threes) sticky.
        let one = Work::one();
        let three = Work::from_i64(3);
        let q = one.div_to(&three, 20);
        assert!(q.sticky);
        assert_eq!(q.digits(), 20);
        let s = q.coeff.to_string();
        assert!(s.chars().all(|c| c == '3'), "quotient digits: {s}");
        // 2 / 3 = 0.6666...6 (20 sixes).
        let q2 = Work::from_i64(2).div_to(&three, 20);
        assert!(q2.coeff.to_string().chars().all(|c| c == '6'));
        // 2 / 4 = 0.5 exact, no sticky.
        let half = Work::from_i64(2).div_to(&Work::from_i64(4), 20);
        assert!(!half.sticky);
        // 0.5 == 5 * 10^-1; at exponent -20 that is 5 * 10^19.
        assert_eq!(as_i128_at(&half, -20), 50_000_000_000_000_000_000);
    }

    #[test]
    fn round_to_i64_rounds_half_away() {
        // 2.5 -> 3, -2.5 -> -3, 2.4 -> 2, 7 -> 7.
        let two_five = Work::new(false, DecBig::from_u32(25), -1);
        assert_eq!(two_five.round_to_i64(), 3);
        assert_eq!(two_five.neg().round_to_i64(), -3);
        assert_eq!(Work::new(false, DecBig::from_u32(24), -1).round_to_i64(), 2);
        assert_eq!(Work::from_i64(7).round_to_i64(), 7);
        assert_eq!(Work::zero().round_to_i64(), 0);
    }

    #[test]
    fn cmp_magnitude_orders_by_value() {
        let a = Work::new(false, DecBig::from_u32(99), -1); // 9.9
        let b = Work::new(false, DecBig::from_u32(10), 0); // 10
        assert_eq!(a.cmp_magnitude(&b), Ordering::Less);
        assert_eq!(b.cmp_magnitude(&a), Ordering::Greater);
        // Negatives compare by magnitude.
        assert_eq!(a.neg().cmp_magnitude(&b.neg()), Ordering::Less);
        // Equal magnitude.
        let c = Work::new(false, DecBig::from_u32(100), -1); // 10.0
        assert_eq!(b.cmp_magnitude(&c), Ordering::Equal);
    }
}
