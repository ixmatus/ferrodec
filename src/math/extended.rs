//! Extended-precision intermediate for transcendentals.
//!
//! ## Why
//!
//! `Decimal128`'s 34-digit envelope isn't wide enough to deliver
//! faithfully-rounded transcendentals. Each Taylor / Newton / argument-
//! reduction step accumulates ~0.5 ULP of error; a 30-term series
//! evaluated at 34 digits drifts ~15 ULP relative to the true result.
//!
//! [`Extended`] gives 50-digit working precision — 16 extra digits
//! over `Decimal128` — which is comfortably enough for a 30-term
//! series with a sub-ULP final error after rounding back to 34 digits.
//!
//! ## Representation
//!
//! `value = (-1)^sign · coef · 10^exp`
//!
//! * `coef: U256`, kept ≤ `EXT_PRECISION` (50) decimal digits after
//!   every rounded operation. 50 digits ≈ 166 bits, so `coef.hi` fits
//!   in 38 bits and `U256 × U256` products fit in `U384`.
//! * `exp: i32`, the unbiased exponent (no `BIAS` offset).
//! * `sign: bool`, true for negative.
//!
//! Special values (NaN / Inf) are NOT representable. Callers must
//! filter them at the [`Decimal128`] boundary.
//!
//! ## Operations
//!
//! All binary ops produce a normalised result (≤ 50-digit `coef`)
//! using round-half-even on the discarded digits. There is no
//! tracking of `INEXACT` here — the only `Status` we emit is at the
//! boundary (`to_decimal128`), where the prior intermediate rounding
//! already reflects the precision loss.
//!
//! ## Status
//!
//! This module is the foundation for migrating `exp`, `ln`, `sincos`,
//! and `pow` to faithful rounding. Until those migrations land, most
//! of the surface here is exercised only by unit tests — hence the
//! crate-level `#[allow(dead_code)]`.

#![allow(dead_code)]

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal128;
use crate::multiword::{u256::widening_mul_u128, U256, U384};
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};
use core::cmp::Ordering;

/// Working precision in decimal digits.
pub(crate) const EXT_PRECISION: u32 = 50;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Extended {
    pub coef: U256,
    pub exp: i32,
    pub sign: bool,
}

impl Extended {
    /// Canonical zero. Sign is positive; exponent is 0 (callers that
    /// care about quantum should set `exp` explicitly).
    pub const ZERO: Self = Self {
        coef: U256::ZERO,
        exp: 0,
        sign: false,
    };

    /// `1`.
    pub const ONE: Self = Self {
        coef: U256 { lo: 1, hi: 0 },
        exp: 0,
        sign: false,
    };

    #[inline]
    pub fn is_zero(self) -> bool {
        self.coef.is_zero()
    }

    /// Negate. Zero stays positive (canonical representation).
    #[inline]
    pub fn neg(self) -> Self {
        if self.is_zero() {
            self
        } else {
            Self {
                sign: !self.sign,
                ..self
            }
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }

    /// Build from a finite or zero `Decimal128`. Panics on NaN / Inf —
    /// callers must dispatch those at the public-API boundary.
    pub fn from_decimal128(d: Decimal128) -> Self {
        match classify_bits(d.to_bits()) {
            Class::Zero { sign, biased_exp } => Self {
                coef: U256::ZERO,
                exp: biased_exp as i32 - BIAS as i32,
                sign,
            },
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => Self {
                coef: U256::from_u128(coefficient),
                exp: biased_exp as i32 - BIAS as i32,
                sign,
            },
            _ => panic!("Extended::from_decimal128: NaN / Inf not representable"),
        }
    }

    pub fn from_i32(n: i32) -> Self {
        if n == 0 {
            return Self::ZERO;
        }
        Self {
            coef: U256::from_u128(n.unsigned_abs() as u128),
            exp: 0,
            sign: n < 0,
        }
    }

    pub fn from_u128(n: u128) -> Self {
        if n == 0 {
            return Self::ZERO;
        }
        Self {
            coef: U256::from_u128(n),
            exp: 0,
            sign: false,
        }
    }

    /// Parse a decimal string. Accepts optional sign, integer / fractional
    /// digits, and an optional `eN` / `e+N` / `e-N` exponent. The string
    /// is assumed to be a hand-curated constant — invalid input panics.
    /// No rounding: the full digit sequence (up to ~75 digits, the U256
    /// capacity) is preserved exactly. Caller is responsible for keeping
    /// the literal within `EXT_PRECISION + small` if they want
    /// invariant preservation.
    pub fn parse_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut sign = false;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            if bytes[i] == b'-' {
                sign = true;
            }
            i += 1;
        }

        let mut coef = U256::ZERO;
        let mut decimal_seen = false;
        let mut digits_after_point: i32 = 0;
        while i < bytes.len() && bytes[i] != b'e' && bytes[i] != b'E' {
            match bytes[i] {
                b'0'..=b'9' => {
                    let d = (bytes[i] - b'0') as u128;
                    coef = coef.mul10().add(U256::from_u128(d));
                    if decimal_seen {
                        digits_after_point += 1;
                    }
                    i += 1;
                }
                b'.' => {
                    assert!(!decimal_seen, "Extended::parse_str: duplicate '.'");
                    decimal_seen = true;
                    i += 1;
                }
                _ => panic!("Extended::parse_str: invalid character"),
            }
        }

        let mut exp_explicit: i32 = 0;
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            let mut exp_sign = false;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                if bytes[i] == b'-' {
                    exp_sign = true;
                }
                i += 1;
            }
            let mut digits = 0i32;
            while i < bytes.len() {
                match bytes[i] {
                    b'0'..=b'9' => {
                        digits = digits * 10 + (bytes[i] - b'0') as i32;
                        i += 1;
                    }
                    _ => panic!("Extended::parse_str: invalid char in exponent"),
                }
            }
            exp_explicit = if exp_sign { -digits } else { digits };
        }

        if coef.is_zero() {
            return Self::ZERO;
        }
        Self {
            coef,
            exp: exp_explicit - digits_after_point,
            sign,
        }
    }

    /// Multiply by `10^k` (k may be negative). This is a pure
    /// exponent shift — no rounding, no coefficient change.
    pub fn mul_pow10_exp(self, k: i32) -> Self {
        if self.is_zero() {
            return self;
        }
        Self {
            coef: self.coef,
            exp: self.exp + k,
            sign: self.sign,
        }
    }

    /// Build an `Extended` from raw components, rounding the
    /// coefficient down to ≤ `EXT_PRECISION` digits via round-half-
    /// even. The resulting value is `(-1)^sign · coef_rounded ·
    /// 10^(exp + drop_count)` — i.e. rounding shifts `exp` upward
    /// when digits are dropped.
    pub fn from_components(coef: U256, exp: i32, sign: bool) -> Self {
        Self::from_components_with_sticky(coef, exp, sign, false)
    }

    /// Variant of [`Self::from_components`] that accepts a `sticky`
    /// flag for digits already dropped before this call (e.g. by a
    /// `U384 → U256` shift). Round-half-even uses both this sticky and
    /// any further-dropped digits.
    pub fn from_components_with_sticky(coef: U256, exp: i32, sign: bool, pre_sticky: bool) -> Self {
        if coef.is_zero() {
            return Self::ZERO;
        }
        let (rounded, exp_shift) = round_u256_to_ext(coef, pre_sticky);
        Self {
            coef: rounded,
            exp: exp + exp_shift as i32,
            sign,
        }
    }

    /// Convert to a `Decimal128`. `q_preferred` is the IEEE 754 §6.3
    /// preferred quantum exponent for the operation that built this
    /// value (callers typically pass `0` for transcendentals or pass
    /// through the source operand's quantum for identity-like ops).
    pub fn to_decimal128(self, q_preferred: i32, rm: RoundingMode) -> (Decimal128, Status) {
        round_and_pack_finite(
            self.coef,
            self.exp,
            q_preferred,
            self.sign,
            false,
            rm,
            Status::OK,
        )
    }

    /// Magnitude comparison (ignoring sign). Useful for branching in
    /// add/sub.
    fn cmp_abs(self, other: Self) -> Ordering {
        if self.is_zero() && other.is_zero() {
            return Ordering::Equal;
        }
        if self.is_zero() {
            return Ordering::Less;
        }
        if other.is_zero() {
            return Ordering::Greater;
        }
        // Compare by decade first.
        let dig_a = self.coef.decimal_digit_count() as i32;
        let dig_b = other.coef.decimal_digit_count() as i32;
        let decade_a = self.exp + dig_a - 1;
        let decade_b = other.exp + dig_b - 1;
        match decade_a.cmp(&decade_b) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => {
                // Same decade — align coefs to the same exponent and compare.
                let a_shift = (dig_b - dig_a).max(0) as u32;
                let b_shift = (dig_a - dig_b).max(0) as u32;
                let a_aligned = U384::from_u256(self.coef).mul_pow10(a_shift);
                let b_aligned = U384::from_u256(other.coef).mul_pow10(b_shift);
                a_aligned.cmp(b_aligned)
            }
        }
    }

    /// Signed total ordering. Treats `+0 == -0`.
    pub fn cmp(self, other: Self) -> Ordering {
        if self.is_zero() && other.is_zero() {
            return Ordering::Equal;
        }
        match (self.sign, other.sign) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.cmp_abs(other),
            (true, true) => other.cmp_abs(self),
        }
    }

    pub fn add(self, other: Self) -> Self {
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }

        // Sort so `lo_op` has the smaller exp (its coef stays put;
        // `hi_op` gets shifted up to match).
        let (lo_op, hi_op) = if self.exp <= other.exp {
            (self, other)
        } else {
            (other, self)
        };
        let delta = (hi_op.exp - lo_op.exp) as u32;

        // Short-circuit only when shifting `hi_op` up by `delta` would
        // overflow `U384`'s ~115-digit envelope. By construction, in
        // those cases `lo_op`'s MSD is far below the sum's LSB at
        // EXT_PRECISION, so the omission is below the rounding
        // boundary. The naive "delta > EXT_PRECISION" check is wrong
        // because it ignores the actual digit-count of `hi_op` —
        // when `hi_op.coef` has only a few digits, the sum can carry
        // information from `lo_op` even at large `delta`.
        let dig_hi = hi_op.coef.decimal_digit_count();
        let max_delta_for_u384: u32 = 115u32.saturating_sub(dig_hi);
        if delta > max_delta_for_u384 {
            return hi_op;
        }

        let hi_shifted = U384::from_u256(hi_op.coef).mul_pow10(delta);
        let lo_extended = U384::from_u256(lo_op.coef);

        let same_sign = hi_op.sign == lo_op.sign;
        let (mut result_coef, mut result_sign) = if same_sign {
            (hi_shifted.add(lo_extended), hi_op.sign)
        } else {
            match hi_shifted.cmp(lo_extended) {
                Ordering::Greater | Ordering::Equal => (hi_shifted.sub(lo_extended), hi_op.sign),
                Ordering::Less => (lo_extended.sub(hi_shifted), lo_op.sign),
            }
        };

        if result_coef.is_zero() {
            result_sign = false;
            return Self {
                coef: U256::ZERO,
                exp: lo_op.exp,
                sign: result_sign,
            };
        }

        let (rounded_coef, exp_shift) = round_u384_to_ext(&mut result_coef);
        Self {
            coef: rounded_coef,
            exp: lo_op.exp + exp_shift as i32,
            sign: result_sign,
        }
    }

    pub fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }

    pub fn mul(self, other: Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::ZERO;
        }
        let mut prod = u256_mul_u256(self.coef, other.coef);
        let result_exp = self.exp + other.exp;
        let result_sign = self.sign ^ other.sign;
        let (rounded_coef, exp_shift) = round_u384_to_ext(&mut prod);
        Self {
            coef: rounded_coef,
            exp: result_exp + exp_shift as i32,
            sign: result_sign,
        }
    }

    /// Square (slightly faster than `mul(self, self)` because it skips
    /// the cross-term symmetry, though here we just call `mul` —
    /// kept as a named entry point for readability).
    pub fn square(self) -> Self {
        self.mul(self)
    }

    /// Reciprocal (`1 / self`) via Newton-Raphson refinement.
    ///
    /// Seed with the Decimal128-rounded reciprocal (≥ 33 digits of
    /// initial precision). Each Newton step `x → x · (2 − b · x)`
    /// roughly doubles the precision; two steps take 33 → ~66 → ~132
    /// digits, comfortably past `EXT_PRECISION = 50`.
    ///
    /// Caller must ensure `self` is non-zero.
    pub fn recip(self) -> Self {
        debug_assert!(!self.is_zero(), "Extended::recip on zero");
        // Seed: 1 / self at Decimal128 precision.
        let (self_d, _) = self.to_decimal128(0, RoundingMode::NearestEven);
        let (recip_d, _) = Decimal128::ONE.div(self_d, RoundingMode::NearestEven);
        let mut x = Self::from_decimal128(recip_d);
        let two = Self::from_i32(2);

        for _ in 0..2 {
            let bx = self.mul(x);
            let correction = two.sub(bx);
            x = x.mul(correction);
        }
        x
    }

    /// Divide `self / other` at extended precision.
    pub fn div(self, other: Self) -> Self {
        if self.is_zero() {
            return Self::ZERO;
        }
        self.mul(other.recip())
    }

    /// Square root via Newton's method, seeded from
    /// [`Decimal128::sqrt`]. Caller must ensure `self` is non-negative
    /// and non-zero.
    ///
    /// One Newton iteration `x → 0.5 · (x + self/x)` doubles precision
    /// from the 33-digit seed to ~66 digits — past `EXT_PRECISION` = 50.
    pub fn sqrt(self) -> Self {
        debug_assert!(!self.sign, "Extended::sqrt of negative");
        if self.is_zero() {
            return self;
        }
        let (self_d, _) = self.to_decimal128(0, RoundingMode::NearestEven);
        let (seed_d, _) = self_d.sqrt(RoundingMode::NearestEven);
        let mut x = Self::from_decimal128(seed_d);
        let half = Self {
            coef: U256::from_u128(5),
            exp: -1,
            sign: false,
        };
        for _ in 0..2 {
            let q = self.div(x);
            x = half.mul(x.add(q));
        }
        x
    }

    /// Divide by a small positive `u32` divisor. Used for Taylor
    /// coefficient sequences `term · r² / ((2n)(2n+1))` where the
    /// denominator is an integer.
    pub fn div_u32(self, divisor: u32) -> Self {
        debug_assert!(divisor != 0, "div_u32: zero divisor");
        if self.is_zero() {
            return self;
        }

        // Scale `coef` up to `EXT_PRECISION + 2` digits before
        // dividing, so the integer-quotient result still has
        // `EXT_PRECISION + 1` digits even after losing one to the
        // division. The +1 gives the round-half-even step a digit to
        // inspect.
        let dig = self.coef.decimal_digit_count();
        let target = EXT_PRECISION + 2;
        let scale_up = target.saturating_sub(dig);

        let scaled = self.coef.mul_pow10(scale_up);
        let (q, r) = scaled.div_rem_u128(u128::from(divisor));
        let pre_sticky = r != 0;
        let new_exp = self.exp - scale_up as i32;

        let (rounded_coef, exp_shift) = round_u256_to_ext(q, pre_sticky);
        Self {
            coef: rounded_coef,
            exp: new_exp + exp_shift as i32,
            sign: self.sign,
        }
    }
}

// ----------------------------------------------------------------------------
// Multi-word helpers.

/// `a × b` for two `U256`s whose combined decimal-digit count is ≤ 115
/// (the `U384` capacity). Inputs must each be ≤ 50 digits, which is
/// the invariant Extended maintains after every round.
#[inline]
fn u256_mul_u256(a: U256, b: U256) -> U384 {
    let (ll_hi, ll_lo) = widening_mul_u128(a.lo, b.lo);
    let (lh_hi, lh_lo) = widening_mul_u128(a.lo, b.hi);
    let (hl_hi, hl_lo) = widening_mul_u128(a.hi, b.lo);
    let (hh_hi, hh_lo) = widening_mul_u128(a.hi, b.hi);

    // U384 layout (little-endian limbs of width 128):
    //   lo  bits 0..127:    ll_lo
    //   mid bits 128..255:  ll_hi + lh_lo + hl_lo  (with carries up)
    //   hi  bits 256..383:  lh_hi + hl_hi + hh_lo + carries_from_mid
    //   overflow (≥ 384):   hh_hi + carries_from_hi   — must be zero
    let lo = ll_lo;
    let (mid_a, c1) = ll_hi.overflowing_add(lh_lo);
    let (mid, c2) = mid_a.overflowing_add(hl_lo);
    let mid_carry: u128 = u128::from(c1) + u128::from(c2);

    let (hi_a, c3) = lh_hi.overflowing_add(hl_hi);
    let (hi_b, c4) = hi_a.overflowing_add(hh_lo);
    let (hi, c5) = hi_b.overflowing_add(mid_carry);
    let final_overflow = u128::from(c3) + u128::from(c4) + u128::from(c5);
    debug_assert!(
        final_overflow == 0 && hh_hi == 0,
        "u256_mul_u256: inputs exceed U384 product capacity"
    );

    U384 { lo, mid, hi }
}

/// Convert a `U384` whose top limb is zero to `U256`.
#[inline]
fn u384_to_u256(c: U384) -> U256 {
    debug_assert!(c.hi == 0, "u384_to_u256: top limb must be zero");
    U256 {
        lo: c.lo,
        hi: c.mid,
    }
}

/// Round a `U384` coefficient down to ≤ `EXT_PRECISION` digits using
/// round-half-even. Returns the rounded `U256` and the number of
/// decimal digits the exponent must be incremented by.
fn round_u384_to_ext(coef: &mut U384) -> (U256, u32) {
    let dig = coef.decimal_digit_count();
    if dig <= EXT_PRECISION {
        // Result already fits. EXT_PRECISION (50) digits ≤ 166 bits,
        // safely within U256.
        return (u384_to_u256(*coef), 0);
    }
    let total_drop = dig - EXT_PRECISION;
    let mut sticky = false;
    let mut round_digit = 0u32;
    for i in 0..total_drop {
        let (q, d) = coef.div_rem10();
        *coef = q;
        if i + 1 < total_drop {
            if d != 0 {
                sticky = true;
            }
        } else {
            round_digit = d;
        }
    }

    let mut c = u384_to_u256(*coef);
    let lsb = (c.lo & 1) as u32;
    let round_up = round_digit > 5 || (round_digit == 5 && (sticky || lsb == 1));
    if round_up {
        c = c.add(U256::from_u128(1));
        if c.decimal_digit_count() > EXT_PRECISION {
            c = c.div_rem10().0;
            return (c, total_drop + 1);
        }
    }
    (c, total_drop)
}

/// Same as `round_u384_to_ext` but starting from a `U256` (e.g. the
/// quotient of an integer division). Caller passes `pre_sticky = true`
/// when there was a non-zero remainder.
fn round_u256_to_ext(mut coef: U256, pre_sticky: bool) -> (U256, u32) {
    let dig = coef.decimal_digit_count();
    if dig <= EXT_PRECISION {
        // No more digits to drop, but `pre_sticky` may still need to
        // bump the LSB on a half-even tie. With dig ≤ EXT_PRECISION
        // we have no actual round digit (truncation already happened
        // outside us), so pre_sticky alone never causes a round-up
        // here — it just means "result is inexact below the LSB".
        let _ = pre_sticky;
        return (coef, 0);
    }
    let total_drop = dig - EXT_PRECISION;
    let mut sticky = pre_sticky;
    let mut round_digit = 0u32;
    for i in 0..total_drop {
        let (q, d) = coef.div_rem10();
        coef = q;
        if i + 1 < total_drop {
            if d != 0 {
                sticky = true;
            }
        } else {
            round_digit = d;
        }
    }

    let lsb = (coef.lo & 1) as u32;
    let round_up = round_digit > 5 || (round_digit == 5 && (sticky || lsb == 1));
    if round_up {
        coef = coef.add(U256::from_u128(1));
        if coef.decimal_digit_count() > EXT_PRECISION {
            coef = coef.div_rem10().0;
            return (coef, total_drop + 1);
        }
    }
    (coef, total_drop)
}

// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn ext(s: &str) -> Extended {
        Extended::from_decimal128(parse(s))
    }

    #[test]
    fn round_trip_decimal128() {
        for s in &[
            "0",
            "1",
            "-1",
            "1.5",
            "12345.6789",
            "-0.000001",
            "1e30",
            "-1e-30",
        ] {
            let d = parse(s);
            let e = Extended::from_decimal128(d);
            let (back, _) = e.to_decimal128(0, RoundingMode::NearestEven);
            let (cmp, _) = back.partial_cmp(d);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "roundtrip failed for {s}"
            );
        }
    }

    #[test]
    fn add_basic() {
        let a = ext("1.5");
        let b = ext("2.25");
        let c = a.add(b);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("3.75");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn sub_basic() {
        let a = ext("3.75");
        let b = ext("1.25");
        let c = a.sub(b);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("2.5");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn mul_basic() {
        let a = ext("3.5");
        let b = ext("4.0");
        let c = a.mul(b);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("14.0");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn mul_high_precision_carries() {
        // (10^25)² should give 10^50, exactly at EXT_PRECISION boundary.
        let a = ext("1e25");
        let b = ext("1e25");
        let c = a.mul(b);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("1e50");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn div_u32_basic() {
        let a = ext("10");
        let c = a.div_u32(3);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        // 10/3 = 3.333…3 to 34 digits.
        let want = parse("3.333333333333333333333333333333333");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn div_u32_terminates_clean() {
        let a = ext("100");
        let c = a.div_u32(4);
        let (back, _) = c.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("25");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn cmp_signs() {
        assert_eq!(ext("1").cmp(ext("2")), Ordering::Less);
        assert_eq!(ext("-1").cmp(ext("2")), Ordering::Less);
        assert_eq!(ext("-1").cmp(ext("-2")), Ordering::Greater);
        assert_eq!(ext("0").cmp(ext("0")), Ordering::Equal);
        assert_eq!(ext("0").cmp(ext("0").neg()), Ordering::Equal);
    }

    #[test]
    fn add_cancellation_preserves_extended_precision() {
        // 1 - (1 - 1e-40) should give 1e-40 *exactly* — the extra
        // working precision means the small bit doesn't get lost.
        let one = ext("1");
        let tiny = ext("1e-40");
        let sub_result = one.sub(tiny); // 0.999…9 with 40 trailing 9s in extended
        let restored = one.sub(sub_result); // should be tiny
        let (back, _) = restored.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("1e-40");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "expected {want:?}, got {back:?}"
        );
    }

    // -----------------------------------------------------------------
    // Oracle cross-check: at extended precision the basic ops should
    // match astro-float to within 1 ULP_50 (i.e. 10^{-50} relative).

    /// Render an `Extended` directly as a full-precision decimal string
    /// of the form `[-]<digits>e<exp>` — no Decimal128 round-trip, so
    /// all 50 working digits make it into the comparison.
    fn ext_to_string(e: Extended) -> alloc::string::String {
        use alloc::string::String;
        if e.is_zero() {
            return String::from("0");
        }
        let mut digits = String::new();
        let mut c = e.coef;
        while !c.is_zero() {
            let (q, d) = c.div_rem10();
            digits.insert(0, char::from(b'0' + d as u8));
            c = q;
        }
        let sign = if e.sign { "-" } else { "" };
        alloc::format!("{sign}{digits}e{}", e.exp)
    }

    fn ext_to_astro(e: Extended) -> astro_float::BigFloat {
        let s = ext_to_string(e);
        let mut cc = astro_float::Consts::new().unwrap();
        astro_float::BigFloat::parse(
            &s,
            astro_float::Radix::Dec,
            300, // 300 bits ≈ 90 decimal digits — well above EXT_PRECISION
            astro_float::RoundingMode::None,
            &mut cc,
        )
    }

    fn astro_diff_below_ulp_50(a: &astro_float::BigFloat, b: &astro_float::BigFloat) -> bool {
        use astro_float::{BigFloat, RoundingMode as AfRm};
        let p = 300;
        let rm = AfRm::None;
        let mut cc = astro_float::Consts::new().unwrap();
        let diff = a.sub(b, p, rm).abs();
        let abs_b = b.abs();
        if abs_b.cmp(&BigFloat::from(0)) == Some(0) {
            // Compare diff against 10^{-49} absolute (one ULP at scale ~1).
            let bound = BigFloat::parse("1e-49", astro_float::Radix::Dec, p, rm, &mut cc);
            return matches!(diff.cmp(&bound), Some(o) if o <= 0);
        }
        let rel = diff.div(&abs_b, p, rm);
        let bound = BigFloat::parse("1e-49", astro_float::Radix::Dec, p, rm, &mut cc);
        matches!(rel.cmp(&bound), Some(o) if o <= 0)
    }

    #[test]
    fn oracle_add_small_random() {
        let pairs = [
            ("1.5", "2.25"),
            ("0.1", "0.2"),
            ("1e30", "1e-30"),
            ("999.9999999999999", "0.0000000000000001"),
            ("-3.5", "5.25"),
            ("1.234567890123456789012345678901234", "1e-50"),
        ];
        for (a_s, b_s) in pairs {
            let a_e = ext(a_s);
            let b_e = ext(b_s);
            let got = a_e.add(b_e);
            let got_af = ext_to_astro(got);
            let mut cc = astro_float::Consts::new().unwrap();
            let a_af = astro_float::BigFloat::parse(
                a_s,
                astro_float::Radix::Dec,
                300,
                astro_float::RoundingMode::None,
                &mut cc,
            );
            let b_af = astro_float::BigFloat::parse(
                b_s,
                astro_float::Radix::Dec,
                300,
                astro_float::RoundingMode::None,
                &mut cc,
            );
            let want_af = a_af.add(&b_af, 300, astro_float::RoundingMode::None);
            assert!(
                astro_diff_below_ulp_50(&got_af, &want_af),
                "add({a_s}, {b_s}) — diff exceeds 1 ULP at 50-digit precision"
            );
        }
    }

    #[test]
    fn oracle_mul_small_random() {
        let pairs = [
            ("3.5", "4.0"),
            ("1.1", "1.1"),
            ("0.9999999999999", "1.0000000000001"),
            ("3.14159265358979323846", "2.71828182845904523536"),
            ("1e25", "1e-25"),
            ("-1.5", "1.5"),
        ];
        for (a_s, b_s) in pairs {
            let a_e = ext(a_s);
            let b_e = ext(b_s);
            let got = a_e.mul(b_e);
            let got_af = ext_to_astro(got);
            let mut cc = astro_float::Consts::new().unwrap();
            let a_af = astro_float::BigFloat::parse(
                a_s,
                astro_float::Radix::Dec,
                300,
                astro_float::RoundingMode::None,
                &mut cc,
            );
            let b_af = astro_float::BigFloat::parse(
                b_s,
                astro_float::Radix::Dec,
                300,
                astro_float::RoundingMode::None,
                &mut cc,
            );
            let want_af = a_af.mul(&b_af, 300, astro_float::RoundingMode::None);
            assert!(
                astro_diff_below_ulp_50(&got_af, &want_af),
                "mul({a_s}, {b_s}) — diff exceeds 1 ULP at 50-digit precision"
            );
        }
    }

    #[test]
    fn oracle_div_u32_small() {
        let cases = [
            ("10", 3),
            ("1", 7),
            ("355", 113), // ≈ π
            ("1.234567890123456789012345678901234", 17),
        ];
        for (a_s, d) in cases {
            let a_e = ext(a_s);
            let got = a_e.div_u32(d);
            let got_af = ext_to_astro(got);
            let mut cc = astro_float::Consts::new().unwrap();
            let a_af = astro_float::BigFloat::parse(
                a_s,
                astro_float::Radix::Dec,
                300,
                astro_float::RoundingMode::None,
                &mut cc,
            );
            let d_af = astro_float::BigFloat::from_word(u64::from(d), 300);
            let want_af = a_af.div(&d_af, 300, astro_float::RoundingMode::None);
            assert!(
                astro_diff_below_ulp_50(&got_af, &want_af),
                "div_u32({a_s}, {d}) — diff exceeds 1 ULP at 50-digit precision"
            );
        }
    }

    #[test]
    fn add_50_digit_precision() {
        // Add a 34-digit value to its 1-ULP neighbour and check we
        // resolve them at extended precision.
        let a = ext("1.234567890123456789012345678901234");
        let b = ext("0.000000000000000000000000000000000001"); // 1e-36
        let c = a.add(b);
        // Subtract a back; should give exactly b (at extended precision).
        let d = c.sub(a);
        let (back, _) = d.to_decimal128(0, RoundingMode::NearestEven);
        let want = parse("1e-36");
        let (cmp, _) = back.partial_cmp(want);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }
}
