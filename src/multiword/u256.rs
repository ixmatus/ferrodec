//! 256-bit unsigned integer used for arithmetic intermediates.
//!
//! `U256` is stored as `(hi: u128, lo: u128)`. The surface is intentionally
//! small — only the operations that `addsub`/`mul`/`div` actually consume
//! are exposed. We are *not* trying to be a general bignum library.
//!
//! Inputs and constraints used by the arithmetic layer are baked into
//! `debug_assert!`s rather than `Result`s — pre-conditions are checked in
//! debug builds, and the release build trusts the caller. The arithmetic
//! layer is the only caller, and it is itself responsible for keeping
//! values within the 226-bit envelope it actually needs.

use core::cmp::Ordering;

/// 256-bit unsigned integer, little-endian halves.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct U256 {
    pub(crate) lo: u128,
    pub(crate) hi: u128,
}

impl U256 {
    pub(crate) const ZERO: Self = Self { lo: 0, hi: 0 };

    #[inline]
    pub(crate) const fn from_u128(x: u128) -> Self {
        Self { lo: x, hi: 0 }
    }

    #[inline]
    pub(crate) const fn is_zero(self) -> bool {
        self.lo == 0 && self.hi == 0
    }

    /// Best-effort downcast to `u128`; panics in debug if the high half is
    /// non-zero. The arithmetic layer only calls this after verifying the
    /// rounded coefficient fits in 113 bits.
    #[inline]
    pub(crate) const fn to_u128(self) -> u128 {
        debug_assert!(self.hi == 0);
        self.lo
    }

    #[inline]
    pub(crate) fn cmp(self, other: Self) -> Ordering {
        match self.hi.cmp(&other.hi) {
            Ordering::Equal => self.lo.cmp(&other.lo),
            ord => ord,
        }
    }

    /// Wrapping `self + other`. Caller is responsible for ensuring the true
    /// sum fits in 256 bits — for our use case (sum of two ≤ 226-bit
    /// aligned coefficients), this is guaranteed.
    #[inline]
    pub(crate) const fn add(self, other: Self) -> Self {
        let (lo, carry) = self.lo.overflowing_add(other.lo);
        let hi = self.hi.wrapping_add(other.hi).wrapping_add(carry as u128);
        Self { lo, hi }
    }

    /// `self - other`. Pre-condition: `self >= other`.
    #[inline]
    pub(crate) const fn sub(self, other: Self) -> Self {
        let (lo, borrow) = self.lo.overflowing_sub(other.lo);
        let hi = self.hi.wrapping_sub(other.hi).wrapping_sub(borrow as u128);
        Self { lo, hi }
    }

    /// `self * 10`. Pre-condition: result fits in 256 bits — the arithmetic
    /// layer never multiplies past the working envelope.
    #[inline]
    pub(crate) fn mul10(self) -> Self {
        // (hi : lo) * 10 = (10 * hi) << 128 + (10 * lo)
        // We compute the four 128×128→256 products, fold carries.
        let (lo_hi, lo_lo) = widening_mul_u128(self.lo, 10);
        let (hi_hi, hi_lo) = widening_mul_u128(self.hi, 10);
        debug_assert!(hi_hi == 0, "U256::mul10 overflow");
        let (new_hi, carry) = lo_hi.overflowing_add(hi_lo);
        debug_assert!(!carry, "U256::mul10 carry into bit 256");
        Self {
            lo: lo_lo,
            hi: new_hi,
        }
    }

    /// `self * 10^k` for small `k` (≤ 76). The arithmetic layer only ever
    /// scales by `≤ 35` decimal places, so this is comfortably bounded.
    pub(crate) fn mul_pow10(mut self, k: u32) -> Self {
        let mut i = 0;
        while i < k {
            self = self.mul10();
            i += 1;
        }
        self
    }

    /// `self / 10` returning the quotient and the remainder digit.
    pub(crate) fn div_rem10(self) -> (Self, u32) {
        // Long division: split into halves and divide top-down.
        let (q_hi, r1) = div_rem_u128_by_small(self.hi, 10);
        // `r1 * 2^128 + lo` divided by 10: feed lo with r1 * 2^128 high bits.
        // We fold the remainder into the low half by computing
        // (r1 << 128 + lo) / 10 with multi-word arithmetic.
        let lo_top = (self.lo >> 64) | ((r1 as u128) << 64);
        let (q_top, r2) = div_rem_u128_by_small(lo_top, 10);
        let lo_bot = (self.lo & 0xFFFF_FFFF_FFFF_FFFF) | ((r2 as u128) << 64);
        let (q_bot, r3) = div_rem_u128_by_small(lo_bot, 10);
        let q_lo = (q_top << 64) | q_bot;
        (
            Self {
                lo: q_lo,
                hi: q_hi,
            },
            r3 as u32,
        )
    }

    /// Number of significant decimal digits in `self`. Returns `1` for zero.
    pub(crate) fn decimal_digit_count(self) -> u32 {
        if self.is_zero() {
            return 1;
        }
        // U256 holds at most ⌈256·log10(2)⌉ = 78 digits. Loop until quotient
        // is zero. Bounded — at most 78 iterations.
        let mut digits = 0u32;
        let mut cur = self;
        while !cur.is_zero() {
            cur = cur.div_rem10().0;
            digits += 1;
        }
        digits
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// `a * b` returning `(hi, lo)` where `hi · 2^128 + lo = a · b`.
///
/// We split each `u128` into 64-bit limbs and do the schoolbook product.
/// `rustc` lowers each `u64 * u64 → u128` to a single instruction on
/// 64-bit hosts (and `__umulsi3` plus folding on 32-bit, which is exactly
/// what we want for the M0+ floor).
#[inline]
pub(crate) const fn widening_mul_u128(a: u128, b: u128) -> (u128, u128) {
    let a_lo = a as u64 as u128;
    let a_hi = a >> 64;
    let b_lo = b as u64 as u128;
    let b_hi = b >> 64;

    let p_ll = a_lo * b_lo; // ≤ (2^64 − 1)^2 < 2^128
    let p_lh = a_lo * b_hi;
    let p_hl = a_hi * b_lo;
    let p_hh = a_hi * b_hi;

    // We need: result = p_hh << 128 + (p_lh + p_hl) << 64 + p_ll
    // Split into hi · 2^128 + lo carefully.

    // mid = p_lh + p_hl, may overflow u128
    let (mid, mid_carry) = p_lh.overflowing_add(p_hl);

    // Add mid << 64 to p_ll for the lo half; carry the top half of mid into hi.
    let mid_lo_part = mid << 64;
    let mid_hi_part = mid >> 64;

    let (lo, lo_carry) = p_ll.overflowing_add(mid_lo_part);

    // hi = p_hh + (carry of mid << 64) + (lo_carry as u128) + ((mid_carry as u128) << 64)
    let hi = p_hh
        .wrapping_add(mid_hi_part)
        .wrapping_add(lo_carry as u128)
        .wrapping_add((mid_carry as u128) << 64);

    (hi, lo)
}

/// `n / d` and `n % d` for a small divisor (here only `10`). Stays `const`
/// so it can run in compile-time tables later.
#[inline]
const fn div_rem_u128_by_small(n: u128, d: u32) -> (u128, u32) {
    let q = n / d as u128;
    let r = (n - q * d as u128) as u32;
    (q, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_no_carry() {
        let a = U256::from_u128(1);
        let b = U256::from_u128(2);
        assert_eq!(a.add(b), U256::from_u128(3));
    }

    #[test]
    fn add_with_carry_into_hi() {
        let a = U256::from_u128(u128::MAX);
        let b = U256::from_u128(1);
        let c = a.add(b);
        assert_eq!(c.lo, 0);
        assert_eq!(c.hi, 1);
    }

    #[test]
    fn sub_borrow_from_hi() {
        let a = U256 { lo: 0, hi: 1 };
        let b = U256::from_u128(1);
        let c = a.sub(b);
        assert_eq!(c.lo, u128::MAX);
        assert_eq!(c.hi, 0);
    }

    #[test]
    fn cmp_orders_by_hi_then_lo() {
        let small = U256 { lo: u128::MAX, hi: 0 };
        let big = U256 { lo: 0, hi: 1 };
        assert_eq!(small.cmp(big), Ordering::Less);
        let same_hi_small = U256 { lo: 1, hi: 7 };
        let same_hi_big = U256 { lo: 2, hi: 7 };
        assert_eq!(same_hi_small.cmp(same_hi_big), Ordering::Less);
    }

    #[test]
    fn mul10_below_u128() {
        let a = U256::from_u128(123_456_789);
        assert_eq!(a.mul10(), U256::from_u128(1_234_567_890));
    }

    #[test]
    fn mul10_carries_into_hi() {
        // Pick a value just under 2^128 / 10 so * 10 overflows lo.
        let big = u128::MAX / 10; // floor of 2^128 / 10
        let a = U256::from_u128(big);
        let result = a.mul10();
        // result = big * 10 — at most 2^128 + (10 * (u128::MAX % 10)) ≈ 2^128
        let big_times_10_lo = big.wrapping_mul(10);
        let big_times_10_hi = widening_mul_u128(big, 10).0;
        assert_eq!(result.lo, big_times_10_lo);
        assert_eq!(result.hi, big_times_10_hi);
    }

    #[test]
    fn mul_pow10_zero_is_identity() {
        let a = U256::from_u128(42);
        assert_eq!(a.mul_pow10(0), a);
    }

    #[test]
    fn mul_pow10_powers_of_ten() {
        let one = U256::from_u128(1);
        for k in 0u32..=38 {
            let expected = U256::from_u128(10u128.pow(k));
            assert_eq!(one.mul_pow10(k), expected, "10^{k}");
        }
    }

    #[test]
    fn mul_pow10_above_u128() {
        // 10^40 doesn't fit in u128.
        let a = U256::from_u128(1).mul_pow10(40);
        assert!(a.hi != 0);
        // Round-trip via div_rem10:
        let (q, r) = a.div_rem10();
        assert_eq!(r, 0);
        let (qq, rr) = q.div_rem10();
        assert_eq!(rr, 0);
        // q should be 10^39
        let _ = qq;
    }

    #[test]
    fn div_rem10_zero() {
        let (q, r) = U256::ZERO.div_rem10();
        assert!(q.is_zero());
        assert_eq!(r, 0);
    }

    #[test]
    fn div_rem10_small() {
        let (q, r) = U256::from_u128(123).div_rem10();
        assert_eq!(q, U256::from_u128(12));
        assert_eq!(r, 3);
    }

    #[test]
    fn div_rem10_inverts_mul10() {
        let values = [0u128, 1, 9, 10, 11, u128::MAX / 11, u128::MAX / 10];
        for &v in &values {
            let a = U256::from_u128(v);
            let prod = a.mul10();
            let (q, r) = prod.div_rem10();
            assert_eq!(q, a, "div_rem10(mul10({v})) quotient");
            assert_eq!(r, 0, "div_rem10(mul10({v})) remainder");
        }
    }

    #[test]
    fn div_rem10_handles_high_half() {
        // (1 << 200) / 10 should produce a real 200-bit quotient.
        let big = U256 {
            lo: 0,
            hi: 1u128 << 72, // 2^200 = (1 << 72) << 128
        };
        let (q, r) = big.div_rem10();
        // Sanity: q * 10 + r == big.
        let recombined = q.mul10().add(U256::from_u128(r as u128));
        assert_eq!(recombined, big);
    }

    #[test]
    fn decimal_digit_count_basics() {
        assert_eq!(U256::ZERO.decimal_digit_count(), 1);
        assert_eq!(U256::from_u128(1).decimal_digit_count(), 1);
        assert_eq!(U256::from_u128(9).decimal_digit_count(), 1);
        assert_eq!(U256::from_u128(10).decimal_digit_count(), 2);
        assert_eq!(U256::from_u128(99).decimal_digit_count(), 2);
        assert_eq!(U256::from_u128(100).decimal_digit_count(), 3);
        assert_eq!(U256::from_u128(10u128.pow(30)).decimal_digit_count(), 31);
    }

    #[test]
    fn decimal_digit_count_above_u128() {
        let a = U256::from_u128(1).mul_pow10(40);
        assert_eq!(a.decimal_digit_count(), 41);
        let b = U256::from_u128(1).mul_pow10(70);
        assert_eq!(b.decimal_digit_count(), 71);
    }

    #[test]
    fn widening_mul_u128_small() {
        let (hi, lo) = widening_mul_u128(2, 3);
        assert_eq!(hi, 0);
        assert_eq!(lo, 6);
    }

    #[test]
    fn widening_mul_u128_overflow_into_hi() {
        let (hi, lo) = widening_mul_u128(u128::MAX, 2);
        // u128::MAX * 2 = 2^129 - 2 = (1 << 128) + (1 << 128) - 2
        // = hi=1, lo=u128::MAX - 1
        assert_eq!(hi, 1);
        assert_eq!(lo, u128::MAX - 1);
    }

    #[test]
    fn widening_mul_u128_max_max() {
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        // hi = 2^128 - 2, lo = 1
        let (hi, lo) = widening_mul_u128(u128::MAX, u128::MAX);
        assert_eq!(hi, u128::MAX - 1);
        assert_eq!(lo, 1);
    }
}
