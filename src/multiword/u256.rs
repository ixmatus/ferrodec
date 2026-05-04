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
        (Self { lo: q_lo, hi: q_hi }, r3)
    }

    /// Long-division `self / divisor` returning `(quotient, remainder)`.
    ///
    /// Bit-by-bit shift-and-subtract. Each iteration shifts a 129-bit
    /// running remainder left by one (tracking the overflow bit
    /// separately, since `divisor` can be up to `u128::MAX`) and
    /// conditionally subtracts `divisor`. 256 iterations.
    ///
    /// Pre-condition: `divisor != 0`.
    pub(crate) fn div_rem_u128(self, divisor: u128) -> (Self, u128) {
        debug_assert!(divisor != 0);

        // Short path when the high half is zero.
        if self.hi == 0 {
            let q = self.lo / divisor;
            let r = self.lo - q * divisor;
            return (Self::from_u128(q), r);
        }

        let mut rem: u128 = 0;
        let mut q_hi: u128 = 0;
        let mut q_lo: u128 = 0;

        let mut i: u32 = 256;
        while i > 0 {
            i -= 1;
            // Bit `i` of `self`.
            let bit = if i >= 128 {
                (self.hi >> (i - 128)) & 1
            } else {
                (self.lo >> i) & 1
            };

            // Shift the running remainder left by 1, tracking the
            // overflow into a virtual 129th bit.
            let carry_out = (rem >> 127) & 1;
            rem = (rem << 1) | bit;

            // After the shift the running value is
            //   (carry_out << 128) | rem
            // Compare against `divisor`. Because `divisor < 2^128`, any
            // `carry_out` makes the running value strictly greater.
            let geq = carry_out == 1 || rem >= divisor;
            if geq {
                rem = rem.wrapping_sub(divisor);
                if i >= 128 {
                    q_hi |= 1u128 << (i - 128);
                } else {
                    q_lo |= 1u128 << i;
                }
            }
        }

        (Self { lo: q_lo, hi: q_hi }, rem)
    }

    /// Floor of the integer square root, with the unsquared remainder.
    ///
    /// Returns `(s, r)` where `s = ⌊√self⌋` and `r = self − s²`. For our
    /// use the input is bounded by `10^70 < 2^234`, so `s` fits in
    /// `u128`. We assert that here.
    ///
    /// Algorithm: Newton's method on integers, started from an upper
    /// bound `2^⌈bitlen/2⌉`. Each step computes `n / x` via
    /// `div_rem_u128` and averages with overflow-safe `(a/2 + b/2 + a&b&1)`.
    /// Converges in O(log log n) once close — for 234-bit inputs that
    /// is well under 20 iterations.
    pub(crate) fn isqrt(self) -> (u128, Self) {
        if self.is_zero() {
            return (0, Self::ZERO);
        }

        let bit_len = if self.hi == 0 {
            128 - self.lo.leading_zeros()
        } else {
            256 - self.hi.leading_zeros()
        };
        let half_bits = bit_len.div_ceil(2);
        debug_assert!(half_bits < 128, "isqrt input exceeds supported range");

        let mut x: u128 = 1u128 << half_bits;

        // Newton's method from above. Each iteration is monotonically
        // non-increasing until the floor is reached.
        loop {
            let (q, _r) = self.div_rem_u128(x);
            debug_assert!(q.hi == 0, "isqrt quotient exceeds u128");
            let q_u128 = q.lo;
            let avg = (x >> 1) + (q_u128 >> 1) + (x & q_u128 & 1);
            if avg >= x {
                break;
            }
            x = avg;
        }

        let (sq_hi, sq_lo) = widening_mul_u128(x, x);
        let rem = self.sub(Self {
            lo: sq_lo,
            hi: sq_hi,
        });
        (x, rem)
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
        let small = U256 {
            lo: u128::MAX,
            hi: 0,
        };
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
    fn div_rem_u128_short_path() {
        let (q, r) = U256::from_u128(123).div_rem_u128(10);
        assert_eq!(q, U256::from_u128(12));
        assert_eq!(r, 3);
    }

    #[test]
    fn div_rem_u128_zero_numerator() {
        let (q, r) = U256::ZERO.div_rem_u128(7);
        assert!(q.is_zero());
        assert_eq!(r, 0);
    }

    #[test]
    fn div_rem_u128_full_inverts_mul() {
        // n * d + r = result, where r < d. Sweep with widening_mul to
        // build a U256 numerator.
        let cases: &[(u128, u128)] = &[
            (1, 1),
            (123, 7),
            (u128::MAX, 7),
            (u128::MAX, u128::MAX),
            (10u128.pow(34), 10u128.pow(17)),
            (10u128.pow(34) - 1, 10u128.pow(33)),
        ];
        for &(a, b) in cases {
            let (hi, lo) = widening_mul_u128(a, b);
            let n = U256 { lo, hi };
            // Divide back by `b` (assume b != 0).
            let (q, r) = n.div_rem_u128(b);
            assert_eq!(q, U256::from_u128(a), "a={a}, b={b}");
            assert_eq!(r, 0, "a={a}, b={b}");
        }
    }

    #[test]
    fn div_rem_u128_with_remainder() {
        // Build a U256 by multiplying then adding a non-zero remainder.
        let a = 10u128.pow(20);
        let b = 7u128;
        let r_in = 3u128;
        let (hi, lo) = widening_mul_u128(a, b);
        let n = U256 { lo, hi }.add(U256::from_u128(r_in));
        let (q, r) = n.div_rem_u128(b);
        assert_eq!(q, U256::from_u128(a));
        assert_eq!(r, r_in);
    }

    #[test]
    fn div_rem_u128_full_u256() {
        // Numerator with both halves non-zero. Build n = (10^60) and
        // divide by 10^30 — quotient should be 10^30, remainder 0.
        let n = U256::from_u128(1).mul_pow10(60);
        let d = 10u128.pow(30);
        let (q, r) = n.div_rem_u128(d);
        assert_eq!(q, U256::from_u128(10u128.pow(30)));
        assert_eq!(r, 0);
    }

    #[test]
    fn isqrt_zero() {
        let (s, r) = U256::ZERO.isqrt();
        assert_eq!(s, 0);
        assert!(r.is_zero());
    }

    #[test]
    fn isqrt_perfect_squares() {
        for &x in &[1u128, 2, 3, 7, 16, 100, 1_000_000, u128::from(u32::MAX)] {
            let n = U256::from_u128(x * x);
            let (s, r) = n.isqrt();
            assert_eq!(s, x, "sqrt({x}^2)");
            assert!(r.is_zero(), "remainder of perfect square {x}^2");
        }
    }

    #[test]
    fn isqrt_off_by_one_remainders() {
        // sqrt(x^2 + k) for small k < 2x+1 should still give x.
        for &x in &[7u128, 100, 12_345] {
            for k in 0..=(2 * x) {
                let n = x * x + k;
                let (s, _) = U256::from_u128(n).isqrt();
                assert_eq!(s, x, "sqrt({n}) ≠ {x}");
            }
            // x^2 + 2x + 1 = (x+1)^2.
            let (s, _) = U256::from_u128(x * x + 2 * x + 1).isqrt();
            assert_eq!(s, x + 1);
        }
    }

    #[test]
    fn isqrt_large() {
        // sqrt(10^60) = 10^30.
        let n = U256::from_u128(1).mul_pow10(60);
        let (s, r) = n.isqrt();
        assert_eq!(s, 10u128.pow(30));
        assert!(r.is_zero());

        // sqrt(10^70 - 1) ≈ 10^35 - tiny. We only check s^2 + r == n.
        let n = U256::from_u128(1).mul_pow10(70).sub(U256::from_u128(1));
        let (s, r) = n.isqrt();
        let (sq_hi, sq_lo) = widening_mul_u128(s, s);
        let recombined = U256 {
            lo: sq_lo,
            hi: sq_hi,
        }
        .add(r);
        assert_eq!(recombined, n);
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
