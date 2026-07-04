//! `DecBig`: a growable, base-`10^9` decimal-limb unsigned integer.
//!
//! This is the coefficient backend for arbitrary-precision decimal
//! (`ferrodec-decimal`). Unlike the fixed-width [`U256`](crate::U256) /
//! [`U384`](crate::U384) / [`U512`](crate::U512) types, `DecBig` grows on
//! the heap, so it lives behind the crate's optional `alloc` feature and is
//! the only part of `ferrodec-multiword` that allocates.
//!
//! # Why decimal limbs
//!
//! Each limb is a `u32` holding one nine-digit decimal group, value in
//! `[0, 10^9)`. Storing the coefficient in radix `10^9` rather than a binary
//! radix makes the operations a decimal kernel actually leans on cheap:
//! scaling by a power of ten (`mul_pow10` / `div_rem_pow10`) is a limb shift
//! plus one small multiply or divide, the decimal digit count is a length
//! computation, and extracting trailing digits for rounding needs no
//! binary-to-decimal conversion. The cost is a slightly more expensive
//! general multiply than a binary radix would pay; that trade favours the
//! decimal workload.
//!
//! Radix `10^9` in a `u32` limb (rather than `10^19` in a `u64`) keeps the
//! per-limb product inside a `u64` accumulator, which lowers to cheap
//! instructions on the 32-bit Cortex-M0+ floor; a `10^19` radix would force
//! `u128` `__multi3` / `__udivti3` libcalls.
//!
//! # Representation invariant
//!
//! Limbs are little-endian (`limbs[0]` is least significant), every limb is
//! `< 10^9`, and there are no most-significant zero limbs. Zero is the empty
//! limb vector. Every constructor and operation re-establishes this
//! normal form, so structural equality (`derive(PartialEq)`) is numeric
//! equality and the length is a faithful magnitude key for comparison.
//!
//! # Provenance
//!
//! The long-division routine [`DecBig::div_rem`] is derived from Knuth, *The
//! Art of Computer Programming*, Volume 2 (*Seminumerical Algorithms*),
//! §4.3.1, Algorithm D, specialized to radix `B = 10^9`. It was derived from
//! the algorithm's description, not transcribed from any implementation. The
//! remaining routines (schoolbook add / subtract / multiply, small-divisor
//! division, Newton integer square root) are standard and written fresh.

use alloc::vec::Vec;
use core::cmp::Ordering;

/// Radix: each limb holds one nine-digit decimal group, `[0, 10^9)`.
const B: u64 = 1_000_000_000;

/// Number of decimal digits packed into one limb.
const LIMB_DIGITS: u32 = 9;

/// Operands with at least this many limbs multiply by Karatsuba; smaller ones
/// use the schoolbook product. The crossover is set from the ADR-0043 bench:
/// below it the recursion's add/sub/split overhead outweighs the saved partial
/// products, above it the `O(n^1.585)` recurrence wins. 32 limbs is 288 digits.
const KARATSUBA_THRESHOLD: usize = 32;

/// Growable base-`10^9` decimal-limb unsigned integer. See the module
/// documentation for the representation invariant and provenance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DecBig {
    /// Little-endian base-`10^9` limbs, each `< 10^9`, no trailing zeros.
    limbs: Vec<u32>,
}

impl DecBig {
    /// The zero value (empty limb vector).
    #[must_use]
    pub const fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    /// Construct from a `u32`, normalizing (a `u32` can exceed `10^9`, so the
    /// result may take two limbs).
    #[must_use]
    pub fn from_u32(x: u32) -> Self {
        Self::from_u64(u64::from(x))
    }

    /// Construct from a `u64`, splitting into base-`10^9` limbs.
    #[must_use]
    pub fn from_u64(mut x: u64) -> Self {
        let mut limbs = Vec::new();
        while x > 0 {
            limbs.push((x % B) as u32);
            x /= B;
        }
        Self { limbs }
    }

    /// Construct from a `u128`, splitting into base-`10^9` limbs.
    #[must_use]
    pub fn from_u128(mut x: u128) -> Self {
        let mut limbs = Vec::new();
        let radix = u128::from(B);
        while x > 0 {
            limbs.push((x % radix) as u32);
            x /= radix;
        }
        Self { limbs }
    }

    /// Construct from a slice of ASCII decimal digit bytes (`b'0'..=b'9'`),
    /// most significant first. Leading zeros are absorbed; an empty slice and
    /// an all-zero slice both yield zero. A non-digit byte panics: fd-aqs.14
    /// made this validation unconditional, because the former `debug_assert!`
    /// let a release build wrap a non-digit into a garbage `>= 10^9` limb,
    /// silently corrupting the normal-form invariant and every `Eq` / `Ord` /
    /// `Hash` / arithmetic result derived from it.
    ///
    /// The bytes are consumed in nine-digit groups from the least significant
    /// end, so each group fills one base-`10^9` limb directly.
    #[must_use]
    pub fn from_ascii_digits(digits: &[u8]) -> Self {
        assert!(
            digits.iter().all(u8::is_ascii_digit),
            "from_ascii_digits requires ASCII digits (fd-aqs.14)"
        );
        let group = LIMB_DIGITS as usize;
        let mut limbs = Vec::with_capacity(digits.len() / group + 1);
        let mut end = digits.len();
        while end > 0 {
            let start = end.saturating_sub(group);
            let mut val = 0u32;
            for &b in &digits[start..end] {
                val = val * 10 + u32::from(b - b'0');
            }
            limbs.push(val);
            end = start;
        }
        Self::from_limbs(limbs)
    }

    /// Build from a raw little-endian limb vector, stripping any
    /// most-significant zero limbs to re-establish the normal form.
    ///
    /// In debug builds this asserts every limb is `< 10^9`.
    #[must_use]
    fn from_limbs(mut limbs: Vec<u32>) -> Self {
        debug_assert!(
            limbs.iter().all(|&l| u64::from(l) < B),
            "DecBig limb out of range"
        );
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Self { limbs }
    }

    /// Number of limbs in normal form (zero has length zero).
    #[must_use]
    fn len(&self) -> usize {
        self.limbs.len()
    }

    /// Limb at index `i`, or `0` past the most significant limb.
    #[inline]
    fn get(&self, i: usize) -> u64 {
        self.limbs.get(i).copied().map_or(0, u64::from)
    }

    /// True when the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// Best-effort conversion to `u128`; `None` if the value does not fit.
    #[must_use]
    pub fn to_u128(&self) -> Option<u128> {
        let mut acc: u128 = 0;
        let radix = u128::from(B);
        for &limb in self.limbs.iter().rev() {
            acc = acc.checked_mul(radix)?.checked_add(u128::from(limb))?;
        }
        Some(acc)
    }

    /// Number of significant decimal digits. Returns `1` for zero, following
    /// the General Decimal Arithmetic convention.
    #[must_use]
    pub fn decimal_digit_count(&self) -> u64 {
        match self.limbs.last() {
            None => 1,
            Some(&top) => {
                (self.len() as u64 - 1) * u64::from(LIMB_DIGITS) + u64::from(digits_u32(top))
            }
        }
    }

    /// Three-way comparison. Valid because the normal form makes limb count a
    /// faithful magnitude key.
    #[must_use]
    pub fn cmp_ref(&self, other: &Self) -> Ordering {
        match self.len().cmp(&other.len()) {
            Ordering::Equal => {
                for i in (0..self.len()).rev() {
                    match self.limbs[i].cmp(&other.limbs[i]) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            ord => ord,
        }
    }

    /// `self + other`.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let n = self.len().max(other.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let sum = self.get(i) + other.get(i) + carry;
            out.push((sum % B) as u32);
            carry = sum / B;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        Self::from_limbs(out)
    }

    /// `self - other`. Precondition: `self >= other`.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Self {
        debug_assert!(
            self.cmp_ref(other) != Ordering::Less,
            "DecBig::sub underflow"
        );
        let mut out = Vec::with_capacity(self.len());
        let mut borrow = 0i64;
        for i in 0..self.len() {
            let diff = self.get(i) as i64 - other.get(i) as i64 - borrow;
            if diff < 0 {
                out.push((diff + B as i64) as u32);
                borrow = 1;
            } else {
                out.push(diff as u32);
                borrow = 0;
            }
        }
        debug_assert!(borrow == 0, "DecBig::sub borrow escaped");
        Self::from_limbs(out)
    }

    /// `self * other`. Schoolbook for small operands; Karatsuba once both reach
    /// `KARATSUBA_THRESHOLD` limbs, where its `O(n^1.585)` recurrence beats the
    /// schoolbook `O(n^2)`.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        if self.len() < KARATSUBA_THRESHOLD || other.len() < KARATSUBA_THRESHOLD {
            self.mul_schoolbook(other)
        } else {
            self.mul_karatsuba(other)
        }
    }

    /// `self * other` by the schoolbook product. The base case of [`mul`].
    #[must_use]
    fn mul_schoolbook(&self, other: &Self) -> Self {
        let mut out = alloc::vec![0u32; self.len() + other.len()];
        for i in 0..self.len() {
            let a = u64::from(self.limbs[i]);
            let mut carry = 0u64;
            for j in 0..other.len() {
                let cur = u64::from(out[i + j]) + a * u64::from(other.limbs[j]) + carry;
                out[i + j] = (cur % B) as u32;
                carry = cur / B;
            }
            // Ripple the leftover carry through the higher limbs.
            let mut k = i + other.len();
            while carry > 0 {
                let cur = u64::from(out[k]) + carry;
                out[k] = (cur % B) as u32;
                carry = cur / B;
                k += 1;
            }
        }
        Self::from_limbs(out)
    }

    /// `self * other` by one Karatsuba step, recursing through [`mul`] so the
    /// sub-products fall back to schoolbook once small. Splitting both operands
    /// at limb `m` (`x = x_hi*B^m + x_lo`), the product is
    /// `z2*B^(2m) + z1*B^m + z0` with `z0 = x_lo*y_lo`, `z2 = x_hi*y_hi`, and
    /// `z1 = (x_lo+x_hi)(y_lo+y_hi) - z0 - z2`. That middle term is
    /// `x_lo*y_hi + x_hi*y_lo`, never negative, so the two subtractions never
    /// borrow past zero (the `DecBig::sub` precondition holds).
    #[must_use]
    fn mul_karatsuba(&self, other: &Self) -> Self {
        let m = self.len().max(other.len()) / 2;
        let (x_lo, x_hi) = self.split_at_limb(m);
        let (y_lo, y_hi) = other.split_at_limb(m);
        let z0 = x_lo.mul(&y_lo);
        let z2 = x_hi.mul(&y_hi);
        let z1 = x_lo.add(&x_hi).mul(&y_lo.add(&y_hi)).sub(&z0).sub(&z2);
        z2.shift_limbs(2 * m).add(&z1.shift_limbs(m)).add(&z0)
    }

    /// Split into the low `m` limbs and the rest, as `(low, high)` with
    /// `self == high*B^m + low`. Either part may be zero.
    #[must_use]
    fn split_at_limb(&self, m: usize) -> (Self, Self) {
        if m >= self.len() {
            (self.clone(), Self::zero())
        } else {
            (
                Self::from_limbs(self.limbs[..m].to_vec()),
                Self::from_limbs(self.limbs[m..].to_vec()),
            )
        }
    }

    /// `self * B^count`: prepend `count` zero limbs (a `10^(9*count)` shift).
    #[must_use]
    fn shift_limbs(&self, count: usize) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let mut limbs = alloc::vec![0u32; count];
        limbs.extend_from_slice(&self.limbs);
        Self::from_limbs(limbs)
    }

    /// `self * small` for `small < 10^9`, normalizing.
    #[must_use]
    fn mul_small(&self, small: u32) -> Self {
        debug_assert!(u64::from(small) < B);
        if small == 0 || self.is_zero() {
            return Self::zero();
        }
        let s = u64::from(small);
        let mut out = Vec::with_capacity(self.len() + 1);
        let mut carry = 0u64;
        for &limb in &self.limbs {
            let cur = u64::from(limb) * s + carry;
            out.push((cur % B) as u32);
            carry = cur / B;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        Self::from_limbs(out)
    }

    /// `self / 10`, returning the quotient and the dropped least-significant
    /// decimal digit.
    #[must_use]
    pub fn div_rem10(&self) -> (Self, u32) {
        self.div_rem_small(10)
    }

    /// `self / divisor` and `self % divisor` for a single-limb divisor
    /// `divisor < 10^9`. Precondition: `divisor != 0`.
    #[must_use]
    fn div_rem_small(&self, divisor: u32) -> (Self, u32) {
        debug_assert!(divisor != 0 && u64::from(divisor) < B);
        let d = u64::from(divisor);
        let mut quotient = alloc::vec![0u32; self.len()];
        let mut rem = 0u64;
        for i in (0..self.len()).rev() {
            // `rem < divisor <= 10^9 - 1`, so `rem * B + limb < 10^9 * 10^9`,
            // comfortably inside `u64`.
            let cur = rem * B + u64::from(self.limbs[i]);
            quotient[i] = (cur / d) as u32;
            rem = cur % d;
        }
        (Self::from_limbs(quotient), rem as u32)
    }

    /// `self / divisor` and `self % divisor` (Euclidean), returning
    /// `(quotient, remainder)`. Precondition: `divisor != 0`.
    ///
    /// Knuth Algorithm D (TAOCP Vol 2 §4.3.1) at radix `B = 10^9`. See the
    /// module-level provenance note.
    #[must_use]
    pub fn div_rem(&self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero(), "DecBig::div_rem by zero");

        // u < v: quotient 0, remainder u.
        if self.cmp_ref(divisor) == Ordering::Less {
            return (Self::zero(), self.clone());
        }
        // Single-limb divisor: the linear-time path.
        if divisor.len() == 1 {
            let (q, r) = self.div_rem_small(divisor.limbs[0]);
            return (q, Self::from_u32(r));
        }

        let n = divisor.len();
        let m = self.len() - n;

        // D1. Normalize so the divisor's leading limb is large. `d` scales
        // both operands; `1 <= d < B` and `d * v[n-1] < B`, so the scaled
        // divisor keeps exactly `n` limbs.
        let d = (B / (u64::from(divisor.limbs[n - 1]) + 1)) as u32;
        let vn = scalar_mul_exact(&divisor.limbs, d, n);
        // The scaled dividend gets one extra high limb (possibly zero).
        let mut un = scalar_mul_exact(&self.limbs, d, m + n + 1);

        let vn1 = vn[n - 1];
        let vn2 = vn[n - 2];
        let mut quotient = alloc::vec![0u32; m + 1];

        // D2..D7. One quotient limb per iteration, most significant first.
        for j in (0..=m).rev() {
            // D3. Estimate the quotient limb from the top two dividend limbs.
            let num = un[j + n] * B + un[j + n - 1];
            let mut qhat = num / vn1;
            let mut rhat = num % vn1;
            // The estimate is at most two too large; this corrects it.
            loop {
                if qhat >= B || qhat * vn2 > rhat * B + un[j + n - 2] {
                    qhat -= 1;
                    rhat += vn1;
                    if rhat < B {
                        continue;
                    }
                }
                break;
            }

            // D4. Multiply the divisor by `qhat` and subtract from the
            // current dividend window `un[j..=j+n]`.
            let mut borrow = 0i64;
            let mut carry = 0u64;
            for i in 0..n {
                let prod = qhat * vn[i] + carry;
                carry = prod / B;
                let sub = un[j + i] as i64 - (prod % B) as i64 - borrow;
                if sub < 0 {
                    un[j + i] = (sub + B as i64) as u64;
                    borrow = 1;
                } else {
                    un[j + i] = sub as u64;
                    borrow = 0;
                }
            }
            let top = un[j + n] as i64 - carry as i64 - borrow;

            if top < 0 {
                // D5/D6. `qhat` was one too large: decrement and add the
                // divisor back, discarding the carry out of the window (it
                // cancels the borrow that just occurred).
                un[j + n] = (top + B as i64) as u64;
                qhat -= 1;
                let mut carry2 = 0u64;
                for i in 0..n {
                    let s = un[j + i] + vn[i] + carry2;
                    un[j + i] = s % B;
                    carry2 = s / B;
                }
                un[j + n] = (un[j + n] + carry2) % B;
            } else {
                un[j + n] = top as u64;
            }

            quotient[j] = qhat as u32;
        }

        // D8. The remainder is the low `n` limbs of the scaled dividend,
        // divided back by the normalization factor `d`.
        let rem_limbs: Vec<u32> = un[0..n].iter().map(|&l| l as u32).collect();
        let (rem, rem_check) = Self::from_limbs(rem_limbs).div_rem_small(d);
        debug_assert!(rem_check == 0, "DecBig::div_rem unnormalization residue");

        (Self::from_limbs(quotient), rem)
    }

    /// `self * 10^k`.
    #[must_use]
    pub fn mul_pow10(&self, k: u32) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let whole = (k / LIMB_DIGITS) as usize;
        let rem = k % LIMB_DIGITS;
        let mut limbs = Vec::with_capacity(whole + self.len());
        limbs.resize(whole, 0);
        limbs.extend_from_slice(&self.limbs);
        let shifted = Self::from_limbs(limbs);
        if rem == 0 {
            shifted
        } else {
            shifted.mul_small(10u32.pow(rem))
        }
    }

    /// `10^k`.
    #[must_use]
    pub fn pow10(k: u32) -> Self {
        let whole = (k / LIMB_DIGITS) as usize;
        let rem = k % LIMB_DIGITS;
        let mut limbs = alloc::vec![0u32; whole];
        limbs.push(10u32.pow(rem));
        Self::from_limbs(limbs)
    }

    /// `self / 10^k` and `self % 10^k`, returning `(quotient, remainder)`.
    /// The remainder carries the `k` least-significant decimal digits.
    #[must_use]
    pub fn div_rem_pow10(&self, k: u32) -> (Self, Self) {
        if k == 0 {
            return (self.clone(), Self::zero());
        }
        let whole = (k / LIMB_DIGITS) as usize;
        let rd = k % LIMB_DIGITS;

        // Split at the limb boundary `10^(9*whole)`.
        let (low, high): (&[u32], &[u32]) = if whole >= self.len() {
            (&self.limbs[..], &[])
        } else {
            self.limbs.split_at(whole)
        };
        let q_coarse = Self::from_limbs(high.to_vec());
        let r_coarse = Self::from_limbs(low.to_vec());

        if rd == 0 {
            return (q_coarse, r_coarse);
        }

        // Further divide the coarse quotient by the intra-limb power `10^rd`.
        let p = 10u32.pow(rd);
        let (quotient, r_small) = q_coarse.div_rem_small(p);
        // remainder = r_small * 10^(9*whole) + r_coarse
        let remainder = Self::from_u32(r_small)
            .mul_pow10(whole as u32 * LIMB_DIGITS)
            .add(&r_coarse);
        (quotient, remainder)
    }

    /// Floor integer square root with the unsquared remainder: returns
    /// `(s, r)` where `s = floor(sqrt(self))` and `r = self - s*s`.
    ///
    /// Newton's method from an upper-bound seed, monotonically decreasing to
    /// the floor. Standard; written fresh.
    #[must_use]
    pub fn isqrt(&self) -> (Self, Self) {
        if self.is_zero() {
            return (Self::zero(), Self::zero());
        }
        // Seed `10^ceil(digits/2) > sqrt(self)`: an upper bound, since
        // `self < 10^digits` gives `sqrt(self) < 10^(digits/2)`.
        let digits = self.decimal_digit_count();
        let seed_pow = u32::try_from(digits.div_ceil(2)).expect("digit count fits u32");
        let mut x = Self::pow10(seed_pow);

        loop {
            // next = floor((x + floor(self/x)) / 2)
            let (q, _) = self.div_rem(&x);
            let (next, _) = x.add(&q).div_rem_small(2);
            if next.cmp_ref(&x) != Ordering::Less {
                break;
            }
            x = next;
        }

        let rem = self.sub(&x.mul(&x));
        (x, rem)
    }
}

impl PartialOrd for DecBig {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DecBig {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_ref(other)
    }
}

impl core::fmt::Display for DecBig {
    /// Writes the unsigned decimal digits with no leading zeros (`0` for
    /// zero). The most significant limb prints bare; every lower limb prints
    /// zero-padded to its nine-digit group.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.limbs.last() {
            None => f.write_str("0"),
            Some(&top) => {
                write!(f, "{top}")?;
                for &limb in self.limbs.iter().rev().skip(1) {
                    write!(f, "{limb:09}")?;
                }
                Ok(())
            }
        }
    }
}

/// Decimal digit count of a single limb value, `1` for zero.
#[inline]
fn digits_u32(x: u32) -> u32 {
    if x == 0 {
        1
    } else {
        x.ilog10() + 1
    }
}

/// `a * d` as exactly `out_len` base-`10^9` limbs (little-endian), where the
/// caller guarantees the true product fits in `out_len` limbs. Used only by
/// the normalization step of [`DecBig::div_rem`], where the limb counts are
/// known precisely. Limbs are returned as `u64` so the division kernel can
/// do in-place signed subtraction without re-widening.
fn scalar_mul_exact(a: &[u32], d: u32, out_len: usize) -> Vec<u64> {
    let mut out = alloc::vec![0u64; out_len];
    let dd = u64::from(d);
    let mut carry = 0u64;
    for (i, &limb) in a.iter().enumerate() {
        let cur = u64::from(limb) * dd + carry;
        out[i] = cur % B;
        carry = cur / B;
    }
    if a.len() < out_len {
        out[a.len()] = carry;
    } else {
        debug_assert!(carry == 0, "scalar_mul_exact overflow");
    }
    out
}

// No Kani harnesses here. `DecBig` is a heap (`Vec`) type, and Kani does not
// model the allocator tractably: a one-limb `(a + b) - b == a` harness alone
// expands to a ~66-million-variable SAT instance (the `Vec` equality lowers to
// an un-unwound `memcmp`) that does not discharge in minutes. This matches the
// precedent that the fixed-width `U256` / `U384` / `U512` primitives in this
// crate carry no Kani proofs. Verification rests on the `u128`-oracle property
// tests in `tests/decbig.rs`, which check every operation against ground-truth
// `u128` arithmetic over the full `u128` range (up to five limbs) and against
// reconstruction identities for wider operands. See ADR-0038.

#[cfg(test)]
mod tests {
    use super::*;

    fn db(x: u128) -> DecBig {
        DecBig::from_u128(x)
    }

    #[test]
    fn zero_and_constructors() {
        assert!(DecBig::zero().is_zero());
        assert!(DecBig::from_u32(0).is_zero());
        assert!(DecBig::from_u64(0).is_zero());
        assert!(DecBig::from_u128(0).is_zero());
        assert_eq!(
            DecBig::from_u32(u32::MAX).to_u128(),
            Some(u128::from(u32::MAX))
        );
        assert_eq!(
            DecBig::from_u64(u64::MAX).to_u128(),
            Some(u128::from(u64::MAX))
        );
    }

    #[test]
    fn digit_count_basics() {
        assert_eq!(DecBig::zero().decimal_digit_count(), 1);
        assert_eq!(db(1).decimal_digit_count(), 1);
        assert_eq!(db(9).decimal_digit_count(), 1);
        assert_eq!(db(10).decimal_digit_count(), 2);
        assert_eq!(db(999_999_999).decimal_digit_count(), 9);
        assert_eq!(db(1_000_000_000).decimal_digit_count(), 10);
        assert_eq!(DecBig::pow10(30).decimal_digit_count(), 31);
        assert_eq!(DecBig::pow10(50).decimal_digit_count(), 51);
    }

    #[test]
    fn cmp_orders_by_magnitude() {
        assert_eq!(db(0).cmp_ref(&db(0)), Ordering::Equal);
        assert_eq!(db(5).cmp_ref(&db(7)), Ordering::Less);
        assert_eq!(
            db(1_000_000_000).cmp_ref(&db(999_999_999)),
            Ordering::Greater
        );
        assert_eq!(
            DecBig::pow10(40).cmp_ref(&DecBig::pow10(39)),
            Ordering::Greater
        );
    }

    #[test]
    fn pow10_matches_mul_pow10() {
        for k in 0u32..40 {
            assert_eq!(DecBig::pow10(k), db(1).mul_pow10(k), "10^{k}");
        }
    }

    #[test]
    fn from_ascii_digits_known() {
        assert!(DecBig::from_ascii_digits(b"").is_zero());
        assert!(DecBig::from_ascii_digits(b"0").is_zero());
        assert!(DecBig::from_ascii_digits(b"0000").is_zero());
        assert_eq!(DecBig::from_ascii_digits(b"000123").to_u128(), Some(123));
        let big = DecBig::from_ascii_digits(b"123456789012345678901234567890");
        assert_eq!(big.decimal_digit_count(), 30);
        assert_eq!(
            big,
            DecBig::from_u128(123_456_789_012_345_678_901_234_567_890)
        );
    }

    #[test]
    #[should_panic(expected = "ASCII digits")]
    fn from_ascii_digits_rejects_non_digit() {
        // fd-aqs.14: validation is unconditional now, so a release build
        // cannot wrap a non-digit byte into a garbage limb.
        let _ = DecBig::from_ascii_digits(b"12x45");
    }

    #[test]
    fn div_rem10_inverts_known() {
        let (q, r) = db(12_345).div_rem10();
        assert_eq!(q, db(1234));
        assert_eq!(r, 5);
        let (q0, r0) = DecBig::zero().div_rem10();
        assert!(q0.is_zero());
        assert_eq!(r0, 0);
    }

    #[test]
    fn div_rem_fires_algorithm_d_add_back() {
        // Directed witness for the Algorithm D D5/D6 add-back branch:
        // `qhat` estimated one too large, so the divisor is added back.
        // The branch fires with probability ~2e-9 per quotient limb on
        // random operands, so it had zero coverage before fd-aqs.14. In
        // base-10^9 little-endian limbs the operands are dividend
        // [0, 0, 500000000, 499999999] and divisor [1, 0, 500000000];
        // as decimals:
        let dividend = DecBig::from_ascii_digits(b"499999999500000000000000000000000000");
        let divisor = DecBig::from_ascii_digits(b"500000000000000000000000001");
        let (q, r) = dividend.div_rem(&divisor);
        // Ground truth (exact integer divmod).
        assert_eq!(q, DecBig::from_ascii_digits(b"999999998"));
        assert_eq!(r, DecBig::from_ascii_digits(b"499999999999999999000000002"));
        // Division invariant `q·d + r == dividend`, `0 <= r < d` — a
        // complete correctness check independent of the ground truth.
        assert_eq!(q.mul(&divisor).add(&r), dividend);
        assert_eq!(r.cmp_ref(&divisor), Ordering::Less);
    }

    #[test]
    fn isqrt_perfect_and_off_by_one() {
        for x in [
            0u128,
            1,
            2,
            3,
            4,
            8,
            9,
            15,
            16,
            99,
            100,
            101,
            10_000,
            123_456_789,
        ] {
            let (s, r) = db(x).isqrt();
            let s = s.to_u128().unwrap();
            let r = r.to_u128().unwrap();
            assert_eq!(s * s + r, x, "isqrt({x}) reconstruction");
            assert!(r <= 2 * s, "isqrt({x}) remainder too large");
            assert!((s + 1) * (s + 1) > x, "isqrt({x}) not floor");
        }
    }

    #[test]
    fn isqrt_large_perfect_square() {
        // (10^25)^2 = 10^50.
        let root = DecBig::pow10(25);
        let n = root.mul(&root);
        let (s, r) = n.isqrt();
        assert_eq!(s, root);
        assert!(r.is_zero());
    }

    #[test]
    fn div_rem_long_division_known() {
        // 10^40 / (10^20 - 1) — a many-limb divisor exercising Algorithm D.
        let num = DecBig::pow10(40);
        let den = DecBig::pow10(20).sub(&db(1));
        let (q, r) = num.div_rem(&den);
        // Reconstruct: q*den + r == num, r < den.
        assert_eq!(q.mul(&den).add(&r), num);
        assert_eq!(r.cmp_ref(&den), Ordering::Less);
    }

    #[test]
    fn karatsuba_matches_schoolbook() {
        // `n` ascii digit bytes from a repeating, nonzero-leading pattern.
        fn digits(pat: &[u8], n: usize) -> Vec<u8> {
            (0..n).map(|i| pat[i % pat.len()]).collect()
        }
        // 900 digits = 100 limbs forces two-level Karatsuba; 300 digits = ~34
        // limbs is one level just past the threshold; the small operand stays on
        // schoolbook even inside `mul`.
        let big_a = DecBig::from_ascii_digits(&digits(b"1234567890", 900));
        let big_b = DecBig::from_ascii_digits(&digits(b"9876543219", 870));
        let near_a = DecBig::from_ascii_digits(&digits(b"31415926535", 300));
        let near_b = DecBig::from_ascii_digits(&digits(b"27182818284", 295));
        let small = DecBig::from_ascii_digits(b"12345678901234567890");
        for (a, b) in [
            (&big_a, &big_b),   // balanced, recursive
            (&big_a, &near_b),  // unbalanced
            (&near_a, &near_b), // one level past the threshold
            (&big_a, &small),   // mixed sizes
        ] {
            // `mul` takes the Karatsuba path; cross-check the proven schoolbook.
            assert_eq!(a.mul(b), a.mul_schoolbook(b), "product mismatch");
            assert_eq!(b.mul(a), a.mul_schoolbook(b), "not commutative");
        }
    }
}
