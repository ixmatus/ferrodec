//! 384-bit unsigned integer used as the FMA alignment buffer.
//!
//! Single-rounding `fusedMultiplyAdd(a, b, c)` holds the exact product
//! `a × b` (up to 68 decimal digits ≈ 226 bits) and aligns it against
//! `c` (up to 34 digits ≈ 113 bits) before a single rounding step.
//! The aligned coefficient can grow past `U256` capacity (78 digits)
//! when the operand quanta differ — `U384` (≈ 115 digits) gives us
//! enough headroom for the realistic shifts the conformance suite
//! exercises, while still being small enough to live on the stack on
//! Cortex-M0+.
//!
//! Surface mirrors `U256` deliberately so the FMA kernel reads like the
//! addsub kernel: `add`, `sub`, `cmp`, `mul10`, `mul_pow10`, `div_rem10`,
//! `decimal_digit_count`, `is_zero`. The one extra is
//! [`U384::shift_right_to_u256`] — when the combined coefficient must be
//! handed to [`round_and_pack_finite`], we shift low-order digits into a
//! sticky bit until the residue fits in a `U256`.

use crate::multiword::U256;
use core::cmp::Ordering;

/// 384-bit unsigned integer, little-endian limbs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct U384 {
    pub(crate) lo: u128,
    pub(crate) mid: u128,
    pub(crate) hi: u128,
}

impl U384 {
    #[allow(dead_code)] // symmetric with U256::ZERO; used by future helpers
    pub(crate) const ZERO: Self = Self {
        lo: 0,
        mid: 0,
        hi: 0,
    };

    #[inline]
    pub(crate) const fn from_u128(x: u128) -> Self {
        Self {
            lo: x,
            mid: 0,
            hi: 0,
        }
    }

    #[inline]
    pub(crate) const fn from_u256(u: U256) -> Self {
        Self {
            lo: u.lo,
            mid: u.hi,
            hi: 0,
        }
    }

    #[inline]
    pub(crate) const fn is_zero(self) -> bool {
        self.lo == 0 && self.mid == 0 && self.hi == 0
    }

    #[inline]
    pub(crate) fn cmp(self, other: Self) -> Ordering {
        match self.hi.cmp(&other.hi) {
            Ordering::Equal => match self.mid.cmp(&other.mid) {
                Ordering::Equal => self.lo.cmp(&other.lo),
                ord => ord,
            },
            ord => ord,
        }
    }

    /// Wrapping `self + other`. Caller is responsible for ensuring the
    /// true sum fits in 384 bits.
    #[inline]
    pub(crate) const fn add(self, other: Self) -> Self {
        let (lo, c0) = self.lo.overflowing_add(other.lo);
        let (mid_a, c1) = self.mid.overflowing_add(other.mid);
        let (mid, c2) = mid_a.overflowing_add(c0 as u128);
        let hi = self
            .hi
            .wrapping_add(other.hi)
            .wrapping_add(c1 as u128)
            .wrapping_add(c2 as u128);
        Self { lo, mid, hi }
    }

    /// `self - other`. Pre-condition: `self >= other`.
    #[inline]
    pub(crate) const fn sub(self, other: Self) -> Self {
        let (lo, b0) = self.lo.overflowing_sub(other.lo);
        let (mid_a, b1) = self.mid.overflowing_sub(other.mid);
        let (mid, b2) = mid_a.overflowing_sub(b0 as u128);
        let hi = self
            .hi
            .wrapping_sub(other.hi)
            .wrapping_sub(b1 as u128)
            .wrapping_sub(b2 as u128);
        Self { lo, mid, hi }
    }

    /// `self * 10`. Pre-condition: result fits in 384 bits.
    #[inline]
    pub(crate) fn mul10(self) -> Self {
        // Multiply each limb by 10, propagate carries upward.
        let (lo_hi, lo_lo) = widening_mul_u128_by_10(self.lo);
        let (mid_hi, mid_lo) = widening_mul_u128_by_10(self.mid);
        let (hi_hi, hi_lo) = widening_mul_u128_by_10(self.hi);
        debug_assert!(hi_hi == 0, "U384::mul10 overflow");

        let lo = lo_lo;
        let (mid, c0) = mid_lo.overflowing_add(lo_hi);
        let hi = hi_lo.wrapping_add(mid_hi).wrapping_add(c0 as u128);
        Self { lo, mid, hi }
    }

    /// `self * 10^k`. Bounded loop; caller keeps `k` within the buffer.
    pub(crate) fn mul_pow10(mut self, k: u32) -> Self {
        let mut i = 0;
        while i < k {
            self = self.mul10();
            i += 1;
        }
        self
    }

    /// `self / 10` returning `(quotient, remainder_digit)`.
    pub(crate) fn div_rem10(self) -> (Self, u32) {
        // Long division top-down: divide each limb feeding the
        // remainder of the higher limb into the next.
        let (q_hi, r_after_hi) = div_rem_u128_by_10(self.hi);
        let (q_mid, r_after_mid) = div_rem_u128_with_carry_in_by_10(self.mid, r_after_hi);
        let (q_lo, r_after_lo) = div_rem_u128_with_carry_in_by_10(self.lo, r_after_mid);
        (
            Self {
                lo: q_lo,
                mid: q_mid,
                hi: q_hi,
            },
            r_after_lo,
        )
    }

    /// Number of significant decimal digits in `self`. Returns `1` for
    /// zero. Bounded by ⌈384·log10(2)⌉ = 116 iterations.
    #[allow(dead_code)] // currently only the unit tests; future shrink path will use it
    pub(crate) fn decimal_digit_count(self) -> u32 {
        if self.is_zero() {
            return 1;
        }
        let mut digits = 0u32;
        let mut cur = self;
        while !cur.is_zero() {
            cur = cur.div_rem10().0;
            digits += 1;
        }
        digits
    }

    /// Shift `self` right by enough decimal digits that the residue fits
    /// in a `U256`, accumulating dropped digits into a sticky bit.
    ///
    /// Returns `(residue, exp_shift, sticky)`:
    /// * `residue` — the kept high-order digits as a `U256`.
    /// * `exp_shift` — the number of digits dropped (the caller adds
    ///   this to the quantum exponent).
    /// * `sticky` — `true` if any dropped digit was non-zero, OR'd with
    ///   `pre_sticky`.
    ///
    /// We deliberately keep the residue inside `U256` (not just under
    /// `PRECISION` digits) so the existing `round_and_pack_finite`
    /// pipeline still receives enough digits for its own round-digit
    /// extraction; the sticky bit tracked here only covers digits below
    /// the round position.
    pub(crate) fn shift_right_to_u256(mut self, pre_sticky: bool) -> (U256, u32, bool) {
        let mut sticky = pre_sticky;
        let mut shift = 0u32;
        // Loop until self fits in 256 bits. We test by checking that the
        // high limb is zero — i.e. the value fits in `(mid, lo)`.
        while self.hi != 0 {
            let (q, r) = self.div_rem10();
            if r != 0 {
                sticky = true;
            }
            self = q;
            shift += 1;
        }
        (
            U256 {
                lo: self.lo,
                hi: self.mid,
            },
            shift,
            sticky,
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// `a * 10` returning `(hi, lo)` such that `hi · 2^128 + lo = a · 10`.
#[inline]
const fn widening_mul_u128_by_10(a: u128) -> (u128, u128) {
    // a = a_hi · 2^64 + a_lo
    // a * 10 = a_hi*10 · 2^64 + a_lo*10
    let a_lo = a as u64 as u128;
    let a_hi = a >> 64;
    let p_lo = a_lo * 10; // <= (2^64 - 1) * 10 < 2^68 — fits u128
    let p_hi = a_hi * 10; // same bound

    // Combine: result_lo = (p_hi << 64) + p_lo, mod 2^128.
    // result_hi = (p_hi >> 64) + carry from low add.
    let p_hi_lo = p_hi << 64;
    let p_hi_hi = p_hi >> 64;
    let (lo, carry) = p_lo.overflowing_add(p_hi_lo);
    let hi = p_hi_hi.wrapping_add(carry as u128);
    (hi, lo)
}

/// `n / 10` and `n % 10`. Returns `(quotient, remainder)`.
#[inline]
const fn div_rem_u128_by_10(n: u128) -> (u128, u32) {
    let q = n / 10;
    let r = (n - q * 10) as u32;
    (q, r)
}

/// Compute `(carry_in · 2^128 + n) / 10` along with the remainder digit.
/// `carry_in` must be `< 10`.
#[inline]
fn div_rem_u128_with_carry_in_by_10(n: u128, carry_in: u32) -> (u128, u32) {
    debug_assert!(carry_in < 10);
    if carry_in == 0 {
        return div_rem_u128_by_10(n);
    }
    // Split n into 64-bit halves so we can fold carry_in into the top
    // half without overflowing.
    let top = (n >> 64) | ((carry_in as u128) << 64);
    let q_top = top / 10;
    let r_top = top - q_top * 10;
    let bot = (n & 0xFFFF_FFFF_FFFF_FFFF) | (r_top << 64);
    let q_bot = bot / 10;
    let r_bot = bot - q_bot * 10;
    let q = (q_top << 64) | q_bot;
    (q, r_bot as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiword::u256::widening_mul_u128;

    #[test]
    fn add_no_carry() {
        let a = U384::from_u128(1);
        let b = U384::from_u128(2);
        assert_eq!(a.add(b), U384::from_u128(3));
    }

    #[test]
    fn add_carries_through_all_limbs() {
        let a = U384 {
            lo: u128::MAX,
            mid: u128::MAX,
            hi: 0,
        };
        let b = U384::from_u128(1);
        let c = a.add(b);
        assert_eq!(c.lo, 0);
        assert_eq!(c.mid, 0);
        assert_eq!(c.hi, 1);
    }

    #[test]
    fn sub_borrow_through_limbs() {
        let a = U384 {
            lo: 0,
            mid: 0,
            hi: 1,
        };
        let b = U384::from_u128(1);
        let c = a.sub(b);
        assert_eq!(c.lo, u128::MAX);
        assert_eq!(c.mid, u128::MAX);
        assert_eq!(c.hi, 0);
    }

    #[test]
    fn cmp_orders_top_down() {
        let small = U384 {
            lo: u128::MAX,
            mid: u128::MAX,
            hi: 0,
        };
        let big = U384 {
            lo: 0,
            mid: 0,
            hi: 1,
        };
        assert_eq!(small.cmp(big), Ordering::Less);
        let same_hi_small = U384 {
            lo: 1,
            mid: 0,
            hi: 7,
        };
        let same_hi_big = U384 {
            lo: 0,
            mid: 1,
            hi: 7,
        };
        assert_eq!(same_hi_small.cmp(same_hi_big), Ordering::Less);
    }

    #[test]
    fn mul10_below_u128() {
        let a = U384::from_u128(123_456_789);
        assert_eq!(a.mul10(), U384::from_u128(1_234_567_890));
    }

    #[test]
    fn mul10_carries_into_mid() {
        let a = U384::from_u128(u128::MAX);
        let r = a.mul10();
        // u128::MAX * 10 = 9 * 2^128 + (2^128 - 10) — let's check via widening.
        let (hi, lo) = widening_mul_u128(u128::MAX, 10);
        assert_eq!(r.lo, lo);
        assert_eq!(r.mid, hi);
        assert_eq!(r.hi, 0);
    }

    #[test]
    fn mul_pow10_zero_is_identity() {
        let a = U384::from_u128(42);
        assert_eq!(a.mul_pow10(0), a);
    }

    #[test]
    fn mul_pow10_powers_of_ten() {
        let one = U384::from_u128(1);
        for k in 0u32..=38 {
            let expected = U384::from_u128(10u128.pow(k));
            assert_eq!(one.mul_pow10(k), expected, "10^{k}");
        }
    }

    #[test]
    fn mul_pow10_above_u256() {
        // 10^80 doesn't fit in u256 (78 digits max), but fits in u384.
        let a = U384::from_u128(1).mul_pow10(80);
        assert!(a.hi != 0 || a.mid != 0);
        // Round-trip via div_rem10:
        let mut cur = a;
        for _ in 0..80 {
            let (q, r) = cur.div_rem10();
            assert_eq!(r, 0);
            cur = q;
        }
        assert_eq!(cur, U384::from_u128(1));
    }

    #[test]
    fn mul_pow10_close_to_full() {
        // 10^115 ≈ 2^382, well inside U384.
        let a = U384::from_u128(1).mul_pow10(115);
        assert!(a.hi != 0);
    }

    #[test]
    fn div_rem10_zero() {
        let (q, r) = U384::ZERO.div_rem10();
        assert!(q.is_zero());
        assert_eq!(r, 0);
    }

    #[test]
    fn div_rem10_inverts_mul10() {
        let cases = [0u128, 1, 9, 10, 11, u128::MAX / 11, u128::MAX / 10];
        for &v in &cases {
            let a = U384::from_u128(v);
            let prod = a.mul10();
            let (q, r) = prod.div_rem10();
            assert_eq!(q, a, "div_rem10(mul10({v})) quotient");
            assert_eq!(r, 0, "div_rem10(mul10({v})) remainder");
        }
    }

    #[test]
    fn div_rem10_full_buffer() {
        // Build a value that occupies all three limbs and round-trip.
        let v = U384::from_u128(123_456_789_012_345_678_901_234u128).mul_pow10(70);
        let mut cur = v;
        let mut digits = 0;
        while !cur.is_zero() {
            cur = cur.div_rem10().0;
            digits += 1;
        }
        // 24 digits of starting value + 70 zeros = 94 digits.
        assert_eq!(digits, 94);
    }

    #[test]
    fn decimal_digit_count_basics() {
        assert_eq!(U384::ZERO.decimal_digit_count(), 1);
        assert_eq!(U384::from_u128(1).decimal_digit_count(), 1);
        assert_eq!(U384::from_u128(10).decimal_digit_count(), 2);
        assert_eq!(U384::from_u128(1).mul_pow10(80).decimal_digit_count(), 81);
    }

    #[test]
    fn shift_right_to_u256_already_fits() {
        let v = U384::from_u128(12345);
        let (residue, shift, sticky) = v.shift_right_to_u256(false);
        assert_eq!(residue.lo, 12345);
        assert_eq!(residue.hi, 0);
        assert_eq!(shift, 0);
        assert!(!sticky);
    }

    #[test]
    fn shift_right_to_u256_sets_sticky_for_dropped_nonzero() {
        // 12345 × 10^80 — high limb is non-zero.
        let v = U384::from_u128(12345).mul_pow10(80);
        // The lowest non-zero digit of `12345 × 10^80` is at position 80
        // (the trailing zeros), so dropping any number of zeros leaves
        // sticky false until we drop a non-zero digit. Build a value with
        // a non-zero low digit on top.
        let v_with_low = v.add(U384::from_u128(7));
        let (_residue, shift, sticky) = v_with_low.shift_right_to_u256(false);
        assert!(shift > 0);
        assert!(sticky, "expected sticky after dropping non-zero low digits");
    }

    #[test]
    fn shift_right_to_u256_preserves_pre_sticky() {
        let v = U384::from_u128(12345);
        let (_, _, sticky) = v.shift_right_to_u256(true);
        assert!(sticky);
    }

    #[test]
    fn from_u256_preserves_value() {
        let u = U256 {
            lo: 0xDEADBEEF,
            hi: 0xCAFEBABE,
        };
        let f = U384::from_u256(u);
        assert_eq!(f.lo, 0xDEADBEEF);
        assert_eq!(f.mid, 0xCAFEBABE);
        assert_eq!(f.hi, 0);
    }
}
