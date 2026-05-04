//! 512-bit unsigned integer used for the wide Payne-Hanek windowed
//! product.
//!
//! Argument reduction for `sin` / `cos` extracts a window of `2/π` up
//! to ~115 decimal digits and multiplies by the 34-digit Decimal128
//! coefficient, producing a value of up to ~149 digits. That product
//! exceeds the `U384` envelope (≈ 115 digits) at the high end, so we
//! drop into `U512` (≈ 154 digits ≈ 4 × `u128` limbs) for the
//! multiply, then collapse back down to `Extended`'s `U256` envelope.
//!
//! Surface mirrors [`U384`]: `add`, `sub`, `cmp`, `mul10`,
//! `mul_pow10`, `div_rem10`, `decimal_digit_count`, plus
//! [`U512::shift_right_to_u256`] for the collapse step.

// Most of `U512`'s surface mirrors `U384` for symmetry — only a
// subset is exercised by `argred` today (struct construction +
// `div_rem10`). Keep the rest available for future use; silence the
// dead-code lint at module scope.
#![allow(dead_code)]

use crate::multiword::{U256, U384};
use core::cmp::Ordering;

/// 512-bit unsigned integer, little-endian limbs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct U512 {
    pub(crate) lo: u128,
    pub(crate) mid_lo: u128,
    pub(crate) mid_hi: u128,
    pub(crate) hi: u128,
}

impl U512 {
    pub(crate) const ZERO: Self = Self {
        lo: 0,
        mid_lo: 0,
        mid_hi: 0,
        hi: 0,
    };

    #[inline]
    pub(crate) const fn from_u128(x: u128) -> Self {
        Self {
            lo: x,
            mid_lo: 0,
            mid_hi: 0,
            hi: 0,
        }
    }

    #[inline]
    pub(crate) const fn from_u384(u: U384) -> Self {
        Self {
            lo: u.lo,
            mid_lo: u.mid,
            mid_hi: u.hi,
            hi: 0,
        }
    }

    #[inline]
    pub(crate) const fn is_zero(self) -> bool {
        self.lo == 0 && self.mid_lo == 0 && self.mid_hi == 0 && self.hi == 0
    }

    #[inline]
    pub(crate) fn cmp(self, other: Self) -> Ordering {
        match self.hi.cmp(&other.hi) {
            Ordering::Equal => match self.mid_hi.cmp(&other.mid_hi) {
                Ordering::Equal => match self.mid_lo.cmp(&other.mid_lo) {
                    Ordering::Equal => self.lo.cmp(&other.lo),
                    ord => ord,
                },
                ord => ord,
            },
            ord => ord,
        }
    }

    /// Wrapping `self + other`. Caller must ensure the true sum fits
    /// in 512 bits.
    #[inline]
    pub(crate) const fn add(self, other: Self) -> Self {
        let (lo, c0) = self.lo.overflowing_add(other.lo);
        let (m_lo_a, c1) = self.mid_lo.overflowing_add(other.mid_lo);
        let (mid_lo, c1b) = m_lo_a.overflowing_add(c0 as u128);
        let (m_hi_a, c2) = self.mid_hi.overflowing_add(other.mid_hi);
        let (mid_hi, c2b) = m_hi_a.overflowing_add((c1 as u128) + (c1b as u128));
        let hi = self
            .hi
            .wrapping_add(other.hi)
            .wrapping_add(c2 as u128)
            .wrapping_add(c2b as u128);
        Self {
            lo,
            mid_lo,
            mid_hi,
            hi,
        }
    }

    /// `self - other`. Pre-condition: `self >= other`.
    #[inline]
    pub(crate) const fn sub(self, other: Self) -> Self {
        let (lo, b0) = self.lo.overflowing_sub(other.lo);
        let (m_lo_a, b1) = self.mid_lo.overflowing_sub(other.mid_lo);
        let (mid_lo, b1b) = m_lo_a.overflowing_sub(b0 as u128);
        let (m_hi_a, b2) = self.mid_hi.overflowing_sub(other.mid_hi);
        let (mid_hi, b2b) = m_hi_a.overflowing_sub((b1 as u128) + (b1b as u128));
        let hi = self
            .hi
            .wrapping_sub(other.hi)
            .wrapping_sub(b2 as u128)
            .wrapping_sub(b2b as u128);
        Self {
            lo,
            mid_lo,
            mid_hi,
            hi,
        }
    }

    /// `self * 10`. Pre-condition: result fits in 512 bits.
    #[inline]
    pub(crate) fn mul10(self) -> Self {
        let (lo_hi, lo_lo) = widening_mul_u128_by_10(self.lo);
        let (m_lo_hi, m_lo_lo) = widening_mul_u128_by_10(self.mid_lo);
        let (m_hi_hi, m_hi_lo) = widening_mul_u128_by_10(self.mid_hi);
        let (hi_hi, hi_lo) = widening_mul_u128_by_10(self.hi);
        debug_assert!(hi_hi == 0, "U512::mul10 overflow");

        let lo = lo_lo;
        let (mid_lo, c0) = m_lo_lo.overflowing_add(lo_hi);
        let (mid_hi_a, c1) = m_hi_lo.overflowing_add(m_lo_hi);
        let (mid_hi, c2) = mid_hi_a.overflowing_add(c0 as u128);
        let hi = hi_lo
            .wrapping_add(m_hi_hi)
            .wrapping_add(c1 as u128)
            .wrapping_add(c2 as u128);
        Self {
            lo,
            mid_lo,
            mid_hi,
            hi,
        }
    }

    /// `self * 10^k`. Bounded loop; caller keeps `k` within capacity.
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
        let (q_hi, r_after_hi) = div_rem_u128_by_10(self.hi);
        let (q_mid_hi, r_after_mid_hi) =
            div_rem_u128_with_carry_in_by_10(self.mid_hi, r_after_hi);
        let (q_mid_lo, r_after_mid_lo) =
            div_rem_u128_with_carry_in_by_10(self.mid_lo, r_after_mid_hi);
        let (q_lo, r_after_lo) = div_rem_u128_with_carry_in_by_10(self.lo, r_after_mid_lo);
        (
            Self {
                lo: q_lo,
                mid_lo: q_mid_lo,
                mid_hi: q_mid_hi,
                hi: q_hi,
            },
            r_after_lo,
        )
    }

    /// Number of significant decimal digits. Returns `1` for zero.
    #[allow(dead_code)]
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

    /// Shift `self` right by enough decimal digits that the residue
    /// fits in a `U256`, accumulating dropped digits into a sticky bit.
    ///
    /// Returns `(residue, shift, sticky)` exactly like
    /// [`U384::shift_right_to_u256`] — the residue fits in `U256`, the
    /// shift is the number of digits dropped (caller adds to the
    /// quantum exponent), and sticky is set if any dropped digit was
    /// non-zero or if `pre_sticky` was set.
    pub(crate) fn shift_right_to_u256(mut self, pre_sticky: bool) -> (U256, u32, bool) {
        let mut sticky = pre_sticky;
        let mut shift = 0u32;
        // Loop until self fits in 256 bits — i.e. mid_hi and hi are zero.
        while self.hi != 0 || self.mid_hi != 0 {
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
                hi: self.mid_lo,
            },
            shift,
            sticky,
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers (mirror the `U384` versions; the duplication is intentional —
// each multiword module owns its own digit-extraction primitives so the
// arithmetic surface stays self-contained).

/// `a * 10` returning `(hi, lo)` such that `hi · 2^128 + lo = a · 10`.
#[inline]
const fn widening_mul_u128_by_10(a: u128) -> (u128, u128) {
    let a_lo = a as u64 as u128;
    let a_hi = a >> 64;
    let p_lo = a_lo * 10;
    let p_hi = a_hi * 10;
    let p_hi_lo = p_hi << 64;
    let p_hi_hi = p_hi >> 64;
    let (lo, carry) = p_lo.overflowing_add(p_hi_lo);
    let hi = p_hi_hi.wrapping_add(carry as u128);
    (hi, lo)
}

#[inline]
const fn div_rem_u128_by_10(n: u128) -> (u128, u32) {
    let q = n / 10;
    let r = (n - q * 10) as u32;
    (q, r)
}

#[inline]
fn div_rem_u128_with_carry_in_by_10(n: u128, carry_in: u32) -> (u128, u32) {
    debug_assert!(carry_in < 10);
    if carry_in == 0 {
        return div_rem_u128_by_10(n);
    }
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

    #[test]
    fn add_no_carry() {
        let a = U512::from_u128(1);
        let b = U512::from_u128(2);
        assert_eq!(a.add(b), U512::from_u128(3));
    }

    #[test]
    fn add_carries_through_all_limbs() {
        let a = U512 {
            lo: u128::MAX,
            mid_lo: u128::MAX,
            mid_hi: u128::MAX,
            hi: 0,
        };
        let b = U512::from_u128(1);
        let c = a.add(b);
        assert_eq!(c.lo, 0);
        assert_eq!(c.mid_lo, 0);
        assert_eq!(c.mid_hi, 0);
        assert_eq!(c.hi, 1);
    }

    #[test]
    fn sub_borrow_through_limbs() {
        let a = U512 {
            lo: 0,
            mid_lo: 0,
            mid_hi: 0,
            hi: 1,
        };
        let b = U512::from_u128(1);
        let c = a.sub(b);
        assert_eq!(c.lo, u128::MAX);
        assert_eq!(c.mid_lo, u128::MAX);
        assert_eq!(c.mid_hi, u128::MAX);
        assert_eq!(c.hi, 0);
    }

    #[test]
    fn mul10_round_trips() {
        let cases = [0u128, 1, 9, 10, 11, u128::MAX / 11, u128::MAX / 10];
        for &v in &cases {
            let a = U512::from_u128(v);
            let prod = a.mul10();
            let (q, r) = prod.div_rem10();
            assert_eq!(q, a, "div_rem10(mul10({v})) quotient");
            assert_eq!(r, 0);
        }
    }

    #[test]
    fn mul_pow10_powers_of_ten() {
        let one = U512::from_u128(1);
        for k in 0u32..=38 {
            let expected = U512::from_u128(10u128.pow(k));
            assert_eq!(one.mul_pow10(k), expected, "10^{k}");
        }
    }

    #[test]
    fn mul_pow10_above_u384() {
        // 10^120 ≈ 2^399 — overflows U384, fits comfortably in U512.
        let v = U512::from_u128(1).mul_pow10(120);
        assert!(v.hi != 0 || v.mid_hi != 0);
        // Round-trip via div_rem10:
        let mut cur = v;
        for _ in 0..120 {
            let (q, r) = cur.div_rem10();
            assert_eq!(r, 0);
            cur = q;
        }
        assert_eq!(cur, U512::from_u128(1));
    }

    #[test]
    fn mul_pow10_close_to_full() {
        // 10^150 ≈ 2^498 — well inside U512.
        let v = U512::from_u128(1).mul_pow10(150);
        assert!(v.hi != 0);
    }

    #[test]
    fn decimal_digit_count_basics() {
        assert_eq!(U512::ZERO.decimal_digit_count(), 1);
        assert_eq!(U512::from_u128(1).decimal_digit_count(), 1);
        assert_eq!(U512::from_u128(10).decimal_digit_count(), 2);
        assert_eq!(
            U512::from_u128(1).mul_pow10(150).decimal_digit_count(),
            151
        );
    }

    #[test]
    fn shift_right_to_u256_collapses_high_limbs() {
        // 12345 × 10^120 — bits up to ~position 412, well past U256.
        // Shift loop stops when both `mid_hi` and `hi` are zero, i.e.
        // when the residue fits in U256 (≤ 2^256 ≈ 1.16 × 10^77).
        let v = U512::from_u128(12345).mul_pow10(120);
        let (_residue, shift, sticky) = v.shift_right_to_u256(false);
        // 12345 × 10^(120−shift) must be < 2^256. Solving gives
        // shift ≥ 48. Allow a tiny window in case the loop exits one
        // iteration earlier or later depending on bit boundaries.
        assert!((48..=50).contains(&shift), "shift = {shift}");
        assert!(!sticky, "all dropped digits are zero");
    }

    #[test]
    fn shift_right_to_u256_sets_sticky_for_dropped_nonzero() {
        // Tail with non-zero low digit.
        let v = U512::from_u128(12345).mul_pow10(120).add(U512::from_u128(7));
        let (_, shift, sticky) = v.shift_right_to_u256(false);
        assert!(shift > 0);
        assert!(sticky);
    }

    #[test]
    fn from_u384_preserves_value() {
        let u = U384 {
            lo: 0xDEAD,
            mid: 0xBEEF,
            hi: 0xCAFE,
        };
        let f = U512::from_u384(u);
        assert_eq!(f.lo, 0xDEAD);
        assert_eq!(f.mid_lo, 0xBEEF);
        assert_eq!(f.mid_hi, 0xCAFE);
        assert_eq!(f.hi, 0);
    }
}
