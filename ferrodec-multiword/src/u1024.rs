//! 1024-bit unsigned integer for the ADR-0060 exact integer
//! adjudicator's widest comparisons.
//!
//! The adjudicator decides the true value's side of one rounding
//! boundary by comparing aligned integer relations. Most of the
//! algebraic group fits `U384` (`rSqrt`) or `U768` (`hypot`, `rootn`
//! to `|n| = 5`, `pown` to `n = −5`, `compound`), but the widest two
//! operands do not: `rootn` at `|n| = 6` aligns a comparand of
//! ~238 digits and `pown` at `n = −6` one of ~239, both past `U768`'s
//! 231-digit envelope. This type's 308 digits (`10^308 < 2^1024`)
//! cover them with two decades of margin, which is what puts those
//! operands inside the unconditional tier instead of leaving the
//! range asymmetric.
//!
//! Surface is deliberately minimal — the adjudicator compares, it
//! never divides or collapses: `add`, `sub`, `cmp`, `mul_u128`,
//! `mul10`, `mul_pow10`, `div_rem10` (digit counting only),
//! `decimal_digit_count`, plus the constructors `from_u128` /
//! `from_u768` and the widening product [`u768_mul_u128_to_u1024`]
//! (the `M^q · a` comparand build). Representation follows
//! [`crate::U768`]: array limbs and looped carries, the same
//! semantics one level up.

#![allow(dead_code)]

use crate::u256::widening_mul_u128;
use crate::U768;
use core::cmp::Ordering;

/// 1024-bit unsigned integer, little-endian `u128` limbs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct U1024 {
    pub limbs: [u128; 8],
}

impl U1024 {
    pub const ZERO: Self = Self { limbs: [0; 8] };

    #[inline]
    pub const fn from_u128(x: u128) -> Self {
        Self {
            limbs: [x, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    #[inline]
    pub const fn from_u768(u: U768) -> Self {
        Self {
            limbs: [
                u.limbs[0], u.limbs[1], u.limbs[2], u.limbs[3], u.limbs[4], u.limbs[5], 0, 0,
            ],
        }
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        let mut i = 0;
        while i < 8 {
            if self.limbs[i] != 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[inline]
    pub fn cmp(self, other: Self) -> Ordering {
        for i in (0..8).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => {}
                ord => return ord,
            }
        }
        Ordering::Equal
    }

    /// Wrapping `self + other`. Caller must ensure the true sum fits
    /// in 1024 bits.
    #[inline]
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        let mut out = [0u128; 8];
        let mut carry = 0u128;
        let mut i = 0;
        while i < 8 {
            let (s, c0) = self.limbs[i].overflowing_add(other.limbs[i]);
            let (s, c1) = s.overflowing_add(carry);
            out[i] = s;
            carry = (c0 as u128) + (c1 as u128);
            i += 1;
        }
        Self { limbs: out }
    }

    /// `self - other`. Pre-condition: `self >= other`.
    #[inline]
    #[must_use]
    pub const fn sub(self, other: Self) -> Self {
        let mut out = [0u128; 8];
        let mut borrow = 0u128;
        let mut i = 0;
        while i < 8 {
            let (d, b0) = self.limbs[i].overflowing_sub(other.limbs[i]);
            let (d, b1) = d.overflowing_sub(borrow);
            out[i] = d;
            borrow = (b0 as u128) + (b1 as u128);
            i += 1;
        }
        Self { limbs: out }
    }

    /// `self * m` for a `u128` multiplier. Debug-asserts the product
    /// fits, mirroring [`U768::mul_u128`]'s envelope assertion style.
    #[must_use]
    pub fn mul_u128(self, m: u128) -> Self {
        let mut out = [0u128; 8];
        let mut carry: u128 = 0;
        for (o, &limb) in out.iter_mut().zip(self.limbs.iter()) {
            let (hi, lo) = widening_mul_u128(limb, m);
            let (s, c0) = lo.overflowing_add(carry);
            *o = s;
            // hi < 2^128 − 1 and c0 ≤ 1, so this never wraps.
            carry = hi + (c0 as u128);
        }
        debug_assert!(carry == 0, "U1024::mul_u128 overflow");
        Self { limbs: out }
    }

    /// `self * 10`. Pre-condition: result fits in 1024 bits.
    #[inline]
    #[must_use]
    pub fn mul10(self) -> Self {
        self.mul_u128(10)
    }

    /// `self * 10^k`. Bounded loop; caller keeps `k` within capacity.
    #[must_use]
    pub fn mul_pow10(mut self, k: u32) -> Self {
        let mut i = 0;
        while i < k {
            self = self.mul10();
            i += 1;
        }
        self
    }

    /// Full `self × other`, or `None` when the product exceeds 1024
    /// bits. Schoolbook over the 8 × 8 limb grid, bailing on any
    /// partial or carry that lands past the top limb. The
    /// adjudicator's powering folds (`a^n`, `C^q`, `N^n`) run
    /// entirely through this one routine, so the envelope arguments
    /// live at its call sites and this stays a plain checked product.
    #[must_use]
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let mut out = [0u128; 8];
        for (i, &ai) in self.limbs.iter().enumerate() {
            if ai == 0 {
                continue;
            }
            let mut carry: u128 = 0;
            for (j, &bj) in other.limbs.iter().enumerate() {
                if bj == 0 && carry == 0 {
                    continue;
                }
                let k = i + j;
                if k >= 8 {
                    // A surviving partial past the top limb overflows.
                    return None;
                }
                let (hi, lo) = widening_mul_u128(ai, bj);
                let (s, c0) = out[k].overflowing_add(lo);
                let (s, c1) = s.overflowing_add(carry);
                out[k] = s;
                // hi ≤ 2^128 − 2 and each c ≤ 1, so the running carry
                // never wraps.
                carry = hi + (c0 as u128) + (c1 as u128);
            }
            // A carry out of the top column targets limb `i + 8`.
            if carry != 0 {
                return None;
            }
        }
        Some(Self { limbs: out })
    }

    /// `self / 10` returning `(quotient, remainder_digit)`.
    pub fn div_rem10(self) -> (Self, u32) {
        let mut out = [0u128; 8];
        let mut rem: u32 = 0;
        for i in (0..8).rev() {
            let (q, r) = div_rem_u128_with_carry_in_by_10(self.limbs[i], rem);
            out[i] = q;
            rem = r;
        }
        (Self { limbs: out }, rem)
    }

    /// Number of significant decimal digits. Returns `1` for zero.
    pub fn decimal_digit_count(self) -> u32 {
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
}

/// Full `U768 × u128` product (up to 896 bits). The adjudicator's
/// `M^q · a` comparand build: `M^q` fills `U768` at the widest
/// operands and the 34-digit format coefficient pushes the product
/// past it. Every partial lands inside the 8-limb accumulator by
/// construction, so no envelope assertion is needed.
#[must_use]
pub fn u768_mul_u128_to_u1024(a: U768, m: u128) -> U1024 {
    let mut out = [0u128; 8];
    let mut carry: u128 = 0;
    for (i, &limb) in a.limbs.iter().enumerate() {
        let (hi, lo) = widening_mul_u128(limb, m);
        let (s, c0) = out[i].overflowing_add(lo);
        let (s, c1) = s.overflowing_add(carry);
        out[i] = s;
        // hi ≤ 2^128 − 2 and each c ≤ 1, so the running carry never
        // wraps.
        carry = hi + (c0 as u128) + (c1 as u128);
    }
    out[6] = carry;
    U1024 { limbs: out }
}

// ---------------------------------------------------------------------------
// Helpers (mirror the `U768` versions; the duplication is intentional —
// each multiword module owns its own digit-extraction primitives so the
// arithmetic surface stays self-contained).

#[inline]
fn div_rem_u128_with_carry_in_by_10(n: u128, carry_in: u32) -> (u128, u32) {
    debug_assert!(carry_in < 10);
    if carry_in == 0 {
        let q = n / 10;
        let r = (n - q * 10) as u32;
        return (q, r);
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

    fn from_decimal(s: &str) -> U1024 {
        let mut acc = U1024::ZERO;
        for b in s.bytes() {
            acc = acc.mul10().add(U1024::from_u128(u128::from(b - b'0')));
        }
        acc
    }

    #[test]
    fn add_carries_through_all_limbs() {
        let mut a = U1024 {
            limbs: [u128::MAX; 8],
        };
        a.limbs[7] = 0;
        let c = a.add(U1024::from_u128(1));
        assert_eq!(c.limbs, [0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn sub_borrows_through_all_limbs() {
        let a = U1024 {
            limbs: [0, 0, 0, 0, 0, 0, 0, 1],
        };
        let c = a.sub(U1024::from_u128(1));
        assert_eq!(c.limbs[7], 0);
        for i in 0..7 {
            assert_eq!(c.limbs[i], u128::MAX);
        }
    }

    #[test]
    fn mul10_round_trips() {
        for &v in &[0u128, 1, 9, 10, 11, u128::MAX / 11, u128::MAX / 10] {
            let a = U1024::from_u128(v);
            let (q, r) = a.mul10().div_rem10();
            assert_eq!(q, a);
            assert_eq!(r, 0);
        }
    }

    #[test]
    fn mul_pow10_to_full_width() {
        // 10^307 ≈ 2^1020: the top of the envelope.
        let v = U1024::from_u128(1).mul_pow10(307);
        assert!(v.limbs[7] != 0);
        assert_eq!(v.decimal_digit_count(), 308);
        let mut cur = v;
        for _ in 0..307 {
            let (q, r) = cur.div_rem10();
            assert_eq!(r, 0);
            cur = q;
        }
        assert_eq!(cur, U1024::from_u128(1));
    }

    #[test]
    fn decimal_digit_count_basics() {
        assert_eq!(U1024::ZERO.decimal_digit_count(), 1);
        assert_eq!(U1024::from_u128(1).decimal_digit_count(), 1);
        assert_eq!(U1024::from_u128(10).decimal_digit_count(), 2);
    }

    #[test]
    fn mul_u128_matches_widening_convention() {
        // Pin the (hi, lo) convention of widening_mul_u128 through a
        // value whose product overflows one limb.
        let a = U1024::from_u128(u128::MAX);
        let p = a.mul_u128(7);
        // u128::MAX * 7 = 7 · 2^128 − 7 → lo = 2^128 − 7, next limb 6.
        assert_eq!(p.limbs[0], u128::MAX - 6);
        assert_eq!(p.limbs[1], 6);
    }

    #[test]
    fn cmp_orders_by_high_limb_first() {
        let lo = U1024::from_u128(u128::MAX);
        let hi = U1024 {
            limbs: [0, 0, 0, 0, 0, 0, 0, 1],
        };
        assert_eq!(lo.cmp(hi), Ordering::Less);
        assert_eq!(hi.cmp(lo), Ordering::Greater);
        assert_eq!(hi.cmp(hi), Ordering::Equal);
    }

    #[test]
    fn u768_mul_u128_widens_past_the_u768_envelope() {
        // (10^231 − 1) · (10^34 − 1): a full-width U768 times a full
        // 34-digit format coefficient lands at 265 digits, well past
        // U768, and must agree with the digit-fold construction.
        let nines231 = {
            let mut v = U768::from_u128(0);
            for _ in 0..231 {
                v = v.mul10().add(U768::from_u128(9));
            }
            v
        };
        let m = 10u128.pow(34) - 1;
        let got = u768_mul_u128_to_u1024(nines231, m);
        // (10^231 − 1)(10^34 − 1) = 10^265 − 10^231 − 10^34 + 1.
        let expected = U1024::from_u128(1)
            .mul_pow10(265)
            .sub(U1024::from_u128(1).mul_pow10(231))
            .sub(U1024::from_u128(1).mul_pow10(34))
            .add(U1024::from_u128(1));
        assert_eq!(got, expected);
        assert_eq!(got.decimal_digit_count(), 265);
    }

    #[test]
    fn u768_mul_u128_small_agrees_with_mul_u128() {
        let a = U768::from_u128(12345);
        let widened = u768_mul_u128_to_u1024(a, 6789);
        assert_eq!(widened, U1024::from_u128(12345 * 6789));
    }

    #[test]
    fn checked_mul_small_and_pow10() {
        let six = U1024::from_u128(2).checked_mul(U1024::from_u128(3));
        assert_eq!(six, Some(U1024::from_u128(6)));

        // 10^154 × 10^154 = 10^308: the last power-of-ten square inside
        // the envelope (10^308 < 2^1024 < 10^309).
        let p154 = U1024::from_u128(1).mul_pow10(154);
        let prod = p154.checked_mul(p154).expect("10^308 fits 1024 bits");
        assert_eq!(prod, U1024::from_u128(1).mul_pow10(308));
        assert_eq!(prod.decimal_digit_count(), 309);
    }

    #[test]
    fn checked_mul_full_width_digits() {
        // (10^154 − 1)² = 10^308 − 2·10^154 + 1: exercises the carry
        // chains across the full grid without leaving the envelope.
        let nines154 = U1024::from_u128(1).mul_pow10(154).sub(U1024::from_u128(1));
        let prod = nines154
            .checked_mul(nines154)
            .expect("(10^154 − 1)² fits 1024 bits");
        let expected = U1024::from_u128(1)
            .mul_pow10(308)
            .sub(U1024::from_u128(2).mul_pow10(154))
            .add(U1024::from_u128(1));
        assert_eq!(prod, expected);
    }

    #[test]
    fn checked_mul_overflow_is_none() {
        // 2^512 × 2^512 = 2^1024: one bit past the envelope.
        let half = U1024 {
            limbs: [0, 0, 0, 0, 1, 0, 0, 0],
        };
        assert_eq!(half.checked_mul(half), None);
        // A carry-driven overflow: (2^1024 − 1) × 2.
        let max = U1024 {
            limbs: [u128::MAX; 8],
        };
        assert_eq!(max.checked_mul(U1024::from_u128(2)), None);
    }

    #[test]
    fn from_decimal_crosses_the_first_limb() {
        // 2^128 rendered in decimal: the digit-fold construction must
        // agree with limb arithmetic across the first limb boundary.
        assert_eq!(
            from_decimal("340282366920938463463374607431768211456"),
            U1024::from_u128(u128::MAX).add(U1024::from_u128(1))
        );
    }

    #[test]
    fn from_u768_preserves_value() {
        let u = U768 {
            limbs: [0xDEAD, 0xBEEF, 0xCAFE, 0xF00D, 0xFACE, 0xFEED],
        };
        let f = U1024::from_u768(u);
        assert_eq!(
            f.limbs,
            [0xDEAD, 0xBEEF, 0xCAFE, 0xF00D, 0xFACE, 0xFEED, 0, 0]
        );
    }
}
