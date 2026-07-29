//! 768-bit unsigned integer for the ADR-0059 rung-2 escalation path.
//!
//! The 110-digit `Extended2` coefficient lives in a `U384`; its full
//! products reach 220 digits and the `reduce_wide` windowed product
//! (a ~181-digit `2/π` window times a 34-digit coefficient) reaches
//! ~215 digits. Both exceed `U512` (~154 digits), so the wide path
//! multiplies in `U768` (6 × `u128` limbs, ~231 digits) and collapses
//! back to the `U384` envelope.
//!
//! Surface mirrors [`crate::U512`]: `add`, `sub`, `cmp`, `mul10`,
//! `mul_pow10`, `div_rem10`, `decimal_digit_count`, plus
//! [`U768::shift_right_to_u384`] for the collapse step and the two
//! product constructors [`u384_mul_u384_to_u768`] and
//! [`U768::mul_u128`].
//!
//! Representation departs from the named-field style of the narrower
//! types: at six limbs a hand-unrolled carry chain is the error-prone
//! choice, so the limbs are an array and the carries are loops. The
//! semantics mirror `U512` exactly.

#![allow(dead_code)]

use crate::u256::widening_mul_u128;
use crate::U384;
use core::cmp::Ordering;

/// 768-bit unsigned integer, little-endian `u128` limbs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct U768 {
    pub limbs: [u128; 6],
}

impl U768 {
    pub const ZERO: Self = Self { limbs: [0; 6] };

    #[inline]
    pub const fn from_u128(x: u128) -> Self {
        Self {
            limbs: [x, 0, 0, 0, 0, 0],
        }
    }

    #[inline]
    pub const fn from_u384(u: U384) -> Self {
        Self {
            limbs: [u.lo, u.mid, u.hi, 0, 0, 0],
        }
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        let mut i = 0;
        while i < 6 {
            if self.limbs[i] != 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[inline]
    pub fn cmp(self, other: Self) -> Ordering {
        for i in (0..6).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => {}
                ord => return ord,
            }
        }
        Ordering::Equal
    }

    /// Wrapping `self + other`. Caller must ensure the true sum fits
    /// in 768 bits.
    #[inline]
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        let mut out = [0u128; 6];
        let mut carry = 0u128;
        let mut i = 0;
        while i < 6 {
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
        let mut out = [0u128; 6];
        let mut borrow = 0u128;
        let mut i = 0;
        while i < 6 {
            let (d, b0) = self.limbs[i].overflowing_sub(other.limbs[i]);
            let (d, b1) = d.overflowing_sub(borrow);
            out[i] = d;
            borrow = (b0 as u128) + (b1 as u128);
            i += 1;
        }
        Self { limbs: out }
    }

    /// `self * m` for a `u128` multiplier. Debug-asserts the product
    /// fits (the top limb's high product must be zero after carries),
    /// mirroring `u256_mul_u256`'s envelope assertion style.
    #[must_use]
    pub fn mul_u128(self, m: u128) -> Self {
        let mut out = [0u128; 6];
        let mut carry: u128 = 0;
        for (o, &limb) in out.iter_mut().zip(self.limbs.iter()) {
            let (hi, lo) = widening_mul_u128(limb, m);
            let (s, c0) = lo.overflowing_add(carry);
            *o = s;
            // hi < 2^128 − 1 and c0 ≤ 1, so this never wraps.
            carry = hi + (c0 as u128);
        }
        debug_assert!(carry == 0, "U768::mul_u128 overflow");
        Self { limbs: out }
    }

    /// `self * 10`. Pre-condition: result fits in 768 bits.
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

    /// `self / 10` returning `(quotient, remainder_digit)`.
    pub fn div_rem10(self) -> (Self, u32) {
        let mut out = [0u128; 6];
        let mut rem: u32 = 0;
        for i in (0..6).rev() {
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

    /// Shift `self` right by enough decimal digits that the residue
    /// fits in a `U384`, accumulating dropped digits into a sticky
    /// bit. Returns `(residue, shift, sticky)` exactly like
    /// [`crate::U512::shift_right_to_u256`], one level up.
    pub fn shift_right_to_u384(mut self, pre_sticky: bool) -> (U384, u32, bool) {
        let mut sticky = pre_sticky;
        let mut shift = 0u32;
        while self.limbs[3] != 0 || self.limbs[4] != 0 || self.limbs[5] != 0 {
            let (q, r) = self.div_rem10();
            if r != 0 {
                sticky = true;
            }
            self = q;
            shift += 1;
        }
        (
            U384 {
                lo: self.limbs[0],
                mid: self.limbs[1],
                hi: self.limbs[2],
            },
            shift,
            sticky,
        )
    }
}

/// Full `U384 × U384` product (up to 768 bits). Schoolbook over the
/// 3 × 3 `u128` limb grid; every partial lands inside the 6-limb
/// accumulator by construction, so no envelope assertion is needed.
#[must_use]
pub fn u384_mul_u384_to_u768(a: U384, b: U384) -> U768 {
    let al = [a.lo, a.mid, a.hi];
    let bl = [b.lo, b.mid, b.hi];
    let mut out = [0u128; 6];
    for (i, &ai) in al.iter().enumerate() {
        let mut carry: u128 = 0;
        for (j, &bj) in bl.iter().enumerate() {
            let (hi, lo) = widening_mul_u128(ai, bj);
            // out[i+j] += lo + carry; propagate into `hi` for the next
            // column. Each addition's overflow feeds the running carry;
            // hi ≤ 2^128 − 2 so the sums below cannot wrap it.
            let (s, c0) = out[i + j].overflowing_add(lo);
            let (s, c1) = s.overflowing_add(carry);
            out[i + j] = s;
            carry = hi + (c0 as u128) + (c1 as u128);
        }
        // Propagate the final column carry upward.
        let mut k = i + 3;
        while carry != 0 {
            let (s, c) = out[k].overflowing_add(carry);
            out[k] = s;
            carry = c as u128;
            k += 1;
        }
    }
    U768 { limbs: out }
}

// ---------------------------------------------------------------------------
// Helpers (mirror the `U512` versions; the duplication is intentional —
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

    fn from_decimal(s: &str) -> U768 {
        let mut acc = U768::ZERO;
        for b in s.bytes() {
            acc = acc.mul10().add(U768::from_u128(u128::from(b - b'0')));
        }
        acc
    }

    #[test]
    fn add_carries_through_all_limbs() {
        let a = U768 {
            limbs: [u128::MAX; 6],
        };
        let mut a = a;
        a.limbs[5] = 0;
        let c = a.add(U768::from_u128(1));
        assert_eq!(c.limbs, [0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn sub_borrows_through_all_limbs() {
        let a = U768 {
            limbs: [0, 0, 0, 0, 0, 1],
        };
        let c = a.sub(U768::from_u128(1));
        assert_eq!(c.limbs[5], 0);
        for i in 0..5 {
            assert_eq!(c.limbs[i], u128::MAX);
        }
    }

    #[test]
    fn mul10_round_trips() {
        for &v in &[0u128, 1, 9, 10, 11, u128::MAX / 11, u128::MAX / 10] {
            let a = U768::from_u128(v);
            let (q, r) = a.mul10().div_rem10();
            assert_eq!(q, a);
            assert_eq!(r, 0);
        }
    }

    #[test]
    fn mul_pow10_to_full_width() {
        // 10^230 ≈ 2^764: the top of the envelope.
        let v = U768::from_u128(1).mul_pow10(230);
        assert!(v.limbs[5] != 0);
        assert_eq!(v.decimal_digit_count(), 231);
        let mut cur = v;
        for _ in 0..230 {
            let (q, r) = cur.div_rem10();
            assert_eq!(r, 0);
            cur = q;
        }
        assert_eq!(cur, U768::from_u128(1));
    }

    #[test]
    fn decimal_digit_count_basics() {
        assert_eq!(U768::ZERO.decimal_digit_count(), 1);
        assert_eq!(U768::from_u128(1).decimal_digit_count(), 1);
        assert_eq!(U768::from_u128(10).decimal_digit_count(), 2);
    }

    #[test]
    fn mul_u128_matches_widening_convention() {
        // Pin the (hi, lo) convention of widening_mul_u128 through a
        // value whose product overflows one limb.
        let a = U768::from_u128(u128::MAX);
        let p = a.mul_u128(7);
        // u128::MAX * 7 = 7 · 2^128 − 7 → lo = 2^128 − 7, next limb 6.
        assert_eq!(p.limbs[0], u128::MAX - 6);
        assert_eq!(p.limbs[1], 6);
    }

    #[test]
    fn u384_mul_u384_small_and_pow10() {
        let two = U384 {
            lo: 2,
            mid: 0,
            hi: 0,
        };
        let three = U384 {
            lo: 3,
            mid: 0,
            hi: 0,
        };
        assert_eq!(u384_mul_u384_to_u768(two, three), U768::from_u128(6));

        // 10^57 × 10^57 = 10^114 (each factor overflows u128).
        let p57 = {
            let mut v = U384 {
                lo: 1,
                mid: 0,
                hi: 0,
            };
            for _ in 0..57 {
                v = v.mul10();
            }
            v
        };
        let prod = u384_mul_u384_to_u768(p57, p57);
        assert_eq!(prod, U768::from_u128(1).mul_pow10(114));
    }

    #[test]
    fn u384_mul_u384_full_width_digits() {
        // (10^115 − 1)² = 10^230 − 2·10^115 + 1: exercises every limb
        // and the carry propagation past the 3×3 grid.
        let nines115 = {
            let mut v = U384 {
                lo: 0,
                mid: 0,
                hi: 0,
            };
            for _ in 0..115 {
                v = v.mul10();
                v = v.add(U384 {
                    lo: 9,
                    mid: 0,
                    hi: 0,
                });
            }
            v
        };
        let prod = u384_mul_u384_to_u768(nines115, nines115);
        let expected = U768::from_u128(1)
            .mul_pow10(230)
            .sub(U768::from_u128(2).mul_pow10(115))
            .add(U768::from_u128(1));
        assert_eq!(prod, expected);
        assert_eq!(prod.decimal_digit_count(), 230);
    }

    #[test]
    fn from_decimal_crosses_the_first_limb() {
        // 2^128 rendered in decimal: the digit-fold construction must
        // agree with limb arithmetic across the first limb boundary.
        assert_eq!(
            from_decimal("340282366920938463463374607431768211456"),
            U768::from_u128(u128::MAX).add(U768::from_u128(1))
        );
    }

    #[test]
    fn shift_right_to_u384_collapses_and_sets_sticky() {
        // 12345 × 10^200 exceeds U384 (~10^115.7); the collapse must
        // drop enough digits to fit and report them.
        let v = U768::from_u128(12345).mul_pow10(200);
        let (residue, shift, sticky) = v.shift_right_to_u384(false);
        assert!(!sticky, "all dropped digits are zero");
        assert!(shift > 0);
        // Reconstruct: residue × 10^shift == original.
        let back = {
            let mut acc = U768 {
                limbs: [residue.lo, residue.mid, residue.hi, 0, 0, 0],
            };
            acc = acc.mul_pow10(shift);
            acc
        };
        assert_eq!(back, v);

        let w = v.add(U768::from_u128(7));
        let (_, s2, sticky2) = w.shift_right_to_u384(false);
        assert!(s2 > 0);
        assert!(sticky2);
    }

    #[test]
    fn from_u384_preserves_value() {
        let u = U384 {
            lo: 0xDEAD,
            mid: 0xBEEF,
            hi: 0xCAFE,
        };
        let f = U768::from_u384(u);
        assert_eq!(f.limbs, [0xDEAD, 0xBEEF, 0xCAFE, 0, 0, 0]);
    }
}
