//! Rung 2 of the escalation ladder (ADR-0059, M5): the 110-digit
//! working type behind [`Extended`]'s 50-digit rung 1.
//!
//! ## Why
//!
//! The S1 falsification probe produced Arb-certified misroundings of
//! the shipped 50-digit Decimal128 trig kernel: near a rounding
//! boundary, rung 1's error bracket cannot decide the format rounding.
//! The M2 predicate ([`Extended::near_rounding_boundary`]) detects
//! those calls; this type is what they escalate to. 110 working digits
//! more than doubles the guard-digit count over every format
//! (76 over `Decimal128`), pushing the expected undecided rate down to
//! the ADR-0059 Tier 2 model figure (~10^-36 per call).
//!
//! ## Mirror discipline
//!
//! This file mirrors `extended.rs` clause for clause at the wider
//! width: `U384` coefficient (110 digits ≈ 366 bits, inside `U384`'s
//! 115-digit envelope), `U768` products (220 digits ≤ 231), and
//! **three** Newton steps where rung 1 takes two (a format seed of
//! ≥ 7 digits doubles to ≥ 56, and the `Decimal128`/`Decimal64` seeds
//! reach ≥ 128 ≥ 110; the per-function error budgets at M8 account
//! for the seed-precision dependence exactly as ADR-0032 does for
//! rung 1). Any behavioral divergence between the two files beyond
//! width parameters is a defect.
//!
//! Not yet wired: the kernels reach this type only at M8, when the
//! per-function budgets and predicate guards land. Until then the
//! crate-root `allow(dead_code)` mirror below keeps the build clean.

#![allow(dead_code)]

use crate::extended::{ExtNum, Extended};
use crate::format::DecimalFormat;
use core::cmp::Ordering;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::{u768::u384_mul_u384_to_u768, U256, U384, U768};

/// Rung 2 working precision in decimal digits.
pub(crate) const EXT2_PRECISION: u32 = 110;

/// Decimal-digit capacity of `U768` (`10^231 < 2^768 < 10^232`), the
/// alignment buffer bound mirroring `extended.rs`'s 115 for `U384`.
const U768_DIGIT_CAPACITY: u32 = 231;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Extended2 {
    pub(crate) coef: U384,
    pub(crate) exp: i32,
    pub(crate) sign: bool,
}

impl Extended2 {
    /// Canonical zero. Sign is positive; exponent is 0.
    pub(crate) const ZERO: Self = Self {
        coef: U384::ZERO,
        exp: 0,
        sign: false,
    };

    /// `1`.
    pub(crate) const ONE: Self = Self {
        coef: U384 {
            lo: 1,
            mid: 0,
            hi: 0,
        },
        exp: 0,
        sign: false,
    };

    /// `0.5`.
    pub(crate) const HALF: Self = Self {
        coef: U384 {
            lo: 5,
            mid: 0,
            hi: 0,
        },
        exp: -1,
        sign: false,
    };

    /// Overflow saturation proxy; see [`Extended::saturate_overflow`]
    /// for the full disposition argument (the exponent 7000 clears
    /// every format's `E_MAX` with the same documentation margin).
    #[inline]
    pub(crate) const fn saturate_overflow(sign: bool) -> Self {
        Self {
            coef: U384 {
                lo: 1,
                mid: 0,
                hi: 0,
            },
            exp: 7000,
            sign,
        }
    }

    /// Underflow saturation proxy; see [`Extended::saturate_underflow`].
    #[inline]
    pub(crate) const fn saturate_underflow() -> Self {
        Self {
            coef: U384 {
                lo: 1,
                mid: 0,
                hi: 0,
            },
            exp: -7000,
            sign: false,
        }
    }

    #[inline]
    pub(crate) fn is_zero(self) -> bool {
        self.coef.is_zero()
    }

    /// Negate. Zero stays positive (canonical representation).
    #[inline]
    #[must_use]
    pub(crate) fn neg(self) -> Self {
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
    #[must_use]
    pub(crate) fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }

    /// Build from a finite or zero format datum. Panics on NaN / Inf —
    /// callers dispatch those at the public-API boundary.
    pub(crate) fn from_format<F: DecimalFormat>(d: F) -> Self {
        let (coef, exp, sign) = d.to_extended_parts().expect(
            "from_format requires a finite or zero datum; NaN / Inf are \
             dispatched at the public-API boundary",
        );
        Self {
            coef: U384::from_u256(coef),
            exp,
            sign,
        }
    }

    /// Lossless widening from the rung-1 carrier: same digits, same
    /// exponent, same sign, wider limbs.
    #[inline]
    pub(crate) fn from_extended(x: Extended) -> Self {
        Self {
            coef: U384::from_u256(x.coef),
            exp: x.exp,
            sign: x.sign,
        }
    }

    pub(crate) fn from_i32(n: i32) -> Self {
        if n == 0 {
            return Self::ZERO;
        }
        Self {
            coef: U384::from_u128(n.unsigned_abs() as u128),
            exp: 0,
            sign: n < 0,
        }
    }

    /// Parse a decimal string; the grammar and panics mirror
    /// [`Extended::parse_str`]. The full digit sequence (up to 115
    /// digits, the `U384` capacity) is preserved exactly.
    pub(crate) fn parse_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut sign = false;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            if bytes[i] == b'-' {
                sign = true;
            }
            i += 1;
        }

        let mut coef = U384::ZERO;
        let mut decimal_seen = false;
        let mut digits_after_point: i32 = 0;
        while i < bytes.len() && bytes[i] != b'e' && bytes[i] != b'E' {
            match bytes[i] {
                b'0'..=b'9' => {
                    let d = (bytes[i] - b'0') as u128;
                    coef = coef.mul10().add(U384::from_u128(d));
                    if decimal_seen {
                        digits_after_point += 1;
                    }
                    i += 1;
                }
                b'.' => {
                    assert!(!decimal_seen, "Extended2::parse_str: duplicate '.'");
                    decimal_seen = true;
                    i += 1;
                }
                _ => panic!("Extended2::parse_str: invalid character"),
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
                    _ => panic!("Extended2::parse_str: invalid char in exponent"),
                }
            }
            exp_explicit = if exp_sign { -digits } else { digits };
        }

        if coef.is_zero() {
            return Self::ZERO;
        }
        // Same guard shape as rung 1: hand-curated literals overshoot
        // by up to 5 digits (the `*_EXT2_STR` constants are 115
        // digits); anything wider bypasses the precision invariant.
        debug_assert!(
            coef.decimal_digit_count() <= EXT2_PRECISION + 5,
            "Extended2::parse_str: literal exceeds EXT2_PRECISION + 5; \
             round it through the invariant machinery or trim the source"
        );
        Self {
            coef,
            exp: exp_explicit - digits_after_point,
            sign,
        }
    }

    /// Multiply by `10^k` (k may be negative). Pure exponent shift.
    #[must_use]
    pub(crate) fn mul_pow10_exp(self, k: i32) -> Self {
        if self.is_zero() {
            return self;
        }
        Self {
            coef: self.coef,
            exp: self.exp + k,
            sign: self.sign,
        }
    }

    /// Build from raw `U384` components, rounding to ≤ `EXT2_PRECISION`
    /// digits via round-half-even.
    pub(crate) fn from_components(coef: U384, exp: i32, sign: bool) -> Self {
        Self::from_components_with_sticky(coef, exp, sign, false)
    }

    /// Variant of [`Self::from_components`] with a pre-dropped sticky.
    pub(crate) fn from_components_with_sticky(
        coef: U384,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self {
        if coef.is_zero() {
            return Self::ZERO;
        }
        let (rounded, exp_shift) = round_u384_to_ext2(coef, pre_sticky);
        Self {
            coef: rounded,
            exp: exp + exp_shift as i32,
            sign,
        }
    }

    /// Convert to a format datum. The 110-digit coefficient exceeds the
    /// rounder's `U256` intake, so the low digits collapse into a
    /// sticky residue first (`shift_right_to_u256`). The collapse is
    /// exact for the rounding decision: the format's round digit sits
    /// at or above the 34th significant digit while the collapse floor
    /// is the 77-digit `U256` envelope, so the round digit is kept
    /// verbatim and every dropped digit lands in the sticky term the
    /// rounder already consumes. The adjusted exponent is preserved
    /// (`digits + exp` is collapse-invariant), so the pre-rounding
    /// tininess decision is untouched.
    pub(crate) fn to_format<F: DecimalFormat>(
        self,
        q_preferred: i32,
        rm: RoundingMode,
    ) -> (F, Status) {
        let (coef, shift, sticky) = self.coef.shift_right_to_u256(false);
        F::round_and_pack_finite(
            coef,
            self.exp + shift as i32,
            q_preferred,
            self.sign,
            sticky,
            rm,
            Status::OK,
        )
    }

    /// The ADR-0051 grid-stuck snap test at rung 2 width: `true` when
    /// `self` lies within ~10^-107 relative of `anchor`. The threshold
    /// is `EXT2_PRECISION − 3`, the same three-digit standoff as
    /// rung 1's 47 = 50 − 3: composition noise is a few units in the
    /// 110th significant digit (~10^-109 relative), while a genuinely
    /// separated result sits at least ~10^-42 relative away (the
    /// ADR-0033 empirical worst-case margins, format-side), so the two
    /// regimes stay separated by more than sixty orders of magnitude.
    #[must_use]
    pub(crate) fn sticks_to(self, anchor: Extended2) -> bool {
        let d = self.sub(anchor);
        if d.is_zero() {
            return true;
        }
        let d_adj = d.exp + d.coef.decimal_digit_count() as i32 - 1;
        let a_adj = anchor.exp + anchor.coef.decimal_digit_count() as i32 - 1;
        d_adj <= a_adj - (EXT2_PRECISION as i32 - 3)
    }

    /// The ADR-0051 anchor residual delivery, mirroring
    /// [`Extended::to_format_with_residual`]: widen to the full
    /// `EXT2_PRECISION` digits, take the open one-ULP interval on the
    /// chosen side, then collapse to the rounder's `U256` intake with
    /// the interval encoded as a forced sticky residue. The collapse
    /// keeps the denoted side: after the optional decrement the
    /// dropped low digits fold into the sticky term, and the denoted
    /// interval (width ≤ 10^-77 relative) stays strictly between the
    /// same format grid points as the true result.
    pub(crate) fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        debug_assert!(!self.is_zero(), "residual rounding needs a nonzero value");
        let dig = self.coef.decimal_digit_count();
        let scale = EXT2_PRECISION - dig;
        let coef_w = self.coef.mul_pow10(scale);
        let exp_w = self.exp - scale as i32;
        let coef_adj = if magnitude_grows {
            coef_w
        } else {
            coef_w.sub(U384::from_u128(1))
        };
        let (coef, shift, _) = coef_adj.shift_right_to_u256(false);
        F::round_and_pack_finite(
            coef,
            exp_w + shift as i32,
            0,
            self.sign,
            true,
            rm,
            Status::OK,
        )
    }

    /// Mode-independent escalation predicate at rung 2 width; the
    /// contract mirrors [`Extended::near_rounding_boundary`] with the
    /// budget unit now one ULP of the 110-digit widened working value.
    /// The strict full drop is `false` by the budget type with even
    /// more room than rung 1: every boundary sits at least
    /// `10^(EXT2_PRECISION − 1) = 10^109` units away.
    ///
    /// The bool view of [`Self::candidate_boundary`]; one computation,
    /// so the adjudicator decides the boundary this predicate flagged.
    #[must_use]
    pub(crate) fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        self.candidate_boundary::<F>(budget).is_near()
    }

    /// The escalation predicate with the boundary identity kept
    /// (ADR-0060's adjudication seam), mirroring
    /// [`Extended::candidate_boundary`] at rung 2 width. This rung is
    /// the one whose identity the exact integer adjudicator consumes:
    /// a `Near` here carries the single candidate boundary of
    /// ADR-0060's semantics (every adjudicating budget sits sixty
    /// decimal orders below a quarter of the drop field, so the hit is
    /// unique).
    #[must_use]
    pub(crate) fn candidate_boundary<F: DecimalFormat>(
        self,
        budget: u128,
    ) -> crate::ladder::BoundaryVerdict {
        self.boundary_verdict::<F>(Some(budget))
    }

    /// The `force_adjudicate` lane's unbudgeted locate (contract on
    /// [`Extended::nearest_boundary`]): this rung's is the one the
    /// lane actually consumes, since adjudication is rung 2
    /// semantics.
    #[must_use]
    pub(crate) fn nearest_boundary<F: DecimalFormat>(self) -> crate::ladder::BoundaryVerdict {
        self.boundary_verdict::<F>(None)
    }

    /// The one computation behind the two views above (the rung 1
    /// mirror's contract).
    fn boundary_verdict<F: DecimalFormat>(
        self,
        budget: Option<u128>,
    ) -> crate::ladder::BoundaryVerdict {
        use crate::ladder::{Boundary, BoundaryVerdict};
        if self.is_zero() {
            return BoundaryVerdict::NearIndeterminate;
        }
        // Normalize to the rung width first: values delivered straight
        // from a hand-curated constant carry up to
        // `EXT2_PRECISION + 5` digits (the `parse_str` envelope); see
        // the rung 1 mirror for why not normalizing would silently
        // under-escalate (M8).
        let (coef, exp) = if self.coef.decimal_digit_count() > EXT2_PRECISION {
            let (c, shift) = round_u384_to_ext2(self.coef, false);
            (c, self.exp + shift as i32)
        } else {
            (self.coef, self.exp)
        };
        let dig = coef.decimal_digit_count();

        let scale = EXT2_PRECISION.saturating_sub(dig);
        let coef_w = coef.mul_pow10(scale);
        let exp_w = exp - scale as i32;
        let digits = dig + scale;

        let qmin = -F::BIAS;
        let precision_excess = digits.saturating_sub(F::PRECISION);
        let subnormal_excess = u32::try_from((qmin - exp_w).max(0)).unwrap_or(u32::MAX);
        let excess = precision_excess.max(subnormal_excess);

        if excess == 0 {
            return BoundaryVerdict::NearIndeterminate;
        }
        if excess > digits {
            // Strict full drop: Clear by the budget type, no nameable
            // identity unbudgeted (the rung 1 mirror's derivation).
            return match budget {
                Some(_) => BoundaryVerdict::Clear,
                None => BoundaryVerdict::NearIndeterminate,
            };
        }

        let mut kept = coef_w;
        let mut i = 0u32;
        while i < excess {
            kept = kept.div_rem10().0;
            i += 1;
        }
        let tail = coef_w.sub(kept.mul_pow10(excess));
        let field = U384::from_u128(1).mul_pow10(excess); // 10^excess ≤ 10^110: fits U384
        let half = U384::from_u128(5).mul_pow10(excess - 1);

        let dist_lower = tail;
        let dist_upper = field.sub(tail);
        let dist_mid = if tail.cmp(half) == Ordering::Less {
            half.sub(tail)
        } else {
            tail.sub(half)
        };

        // Nearest boundary wins; strict comparisons implement the
        // documented tie order (grid before midpoint, lower before
        // upper), and the hit is unique for every adjudicating budget
        // (rung 1's mirror carries the argument).
        let mut best_dist = dist_lower;
        let mut best = 0u8;
        if dist_upper.cmp(best_dist) == Ordering::Less {
            best_dist = dist_upper;
            best = 1;
        }
        if dist_mid.cmp(best_dist) == Ordering::Less {
            best_dist = dist_mid;
            best = 2;
        }
        if let Some(bound) = budget {
            if best_dist.cmp(U384::from_u128(bound)) == Ordering::Greater {
                return BoundaryVerdict::Clear;
            }
        }
        if best == 0 && kept.is_zero() {
            // Full-drop edge, zero grid point nearest: no nameable
            // identity (unbudgeted only; the rung 1 mirror derives
            // why a budget cannot select it).
            return BoundaryVerdict::NearIndeterminate;
        }

        // At most `F::PRECISION ≤ 34` kept digits: fits the low limb
        // (`ladder::Boundary`'s type doc carries the derivation).
        debug_assert!(
            kept.mid == 0 && kept.hi == 0,
            "kept coefficient exceeds u128"
        );
        let kept_c = kept.lo;
        let exp_grid = exp_w + excess as i32;
        BoundaryVerdict::Near(match best {
            0 => Boundary::lower_grid(kept_c, exp_grid),
            1 => Boundary::upper_grid(kept_c, exp_grid),
            _ => Boundary::midpoint(kept_c, exp_grid),
        })
    }

    /// Magnitude comparison (ignoring sign).
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
                let a_aligned = U768::from_u384(self.coef).mul_pow10(a_shift);
                let b_aligned = U768::from_u384(other.coef).mul_pow10(b_shift);
                a_aligned.cmp(b_aligned)
            }
        }
    }

    /// Signed total ordering. Treats `+0 == -0`.
    pub(crate) fn cmp(self, other: Self) -> Ordering {
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

    #[must_use]
    pub(crate) fn add(self, other: Self) -> Self {
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }

        let (lo_op, hi_op) = if self.exp <= other.exp {
            (self, other)
        } else {
            (other, self)
        };
        let delta = (hi_op.exp - lo_op.exp) as u32;

        // Short-circuit only when shifting `hi_op` up by `delta` would
        // overflow the U768 alignment buffer; the digit-count-aware
        // bound mirrors rung 1's `115 − dig_hi` at the U768 capacity.
        let dig_hi = hi_op.coef.decimal_digit_count();
        let max_delta_for_u768: u32 = U768_DIGIT_CAPACITY.saturating_sub(dig_hi);
        if delta > max_delta_for_u768 {
            return hi_op;
        }

        let hi_shifted = U768::from_u384(hi_op.coef).mul_pow10(delta);
        let lo_extended = U768::from_u384(lo_op.coef);

        let same_sign = hi_op.sign == lo_op.sign;
        let (result_coef, mut result_sign) = if same_sign {
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
                coef: U384::ZERO,
                exp: lo_op.exp,
                sign: result_sign,
            };
        }

        let (rounded_coef, exp_shift) = round_u768_to_ext2(result_coef);
        Self {
            coef: rounded_coef,
            exp: lo_op.exp + exp_shift as i32,
            sign: result_sign,
        }
    }

    #[must_use]
    pub(crate) fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }

    #[must_use]
    pub(crate) fn mul(self, other: Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::ZERO;
        }
        let prod = u384_mul_u384_to_u768(self.coef, other.coef);
        let result_exp = self.exp + other.exp;
        let result_sign = self.sign ^ other.sign;
        let (rounded_coef, exp_shift) = round_u768_to_ext2(prod);
        Self {
            coef: rounded_coef,
            exp: result_exp + exp_shift as i32,
            sign: result_sign,
        }
    }

    #[must_use]
    pub(crate) fn square(self) -> Self {
        self.mul(self)
    }

    /// Reciprocal via Newton-Raphson, seeded at the format's precision.
    ///
    /// Three steps where rung 1 takes two: precision doubles per step,
    /// so a `Decimal128` seed runs 34 → 68 → 136 → 272, comfortably
    /// past `EXT2_PRECISION = 110` (the narrower formats' seeds carry
    /// proportionally less, exactly as at rung 1; the M8 budgets own
    /// that accounting).
    #[must_use]
    pub(crate) fn recip<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.is_zero(), "Extended2::recip on zero");
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (recip_d, _) = self_d.recip_seed(RoundingMode::NearestEven);
        let mut x = Self::from_format::<F>(recip_d);
        let two = Self::from_i32(2);

        for _ in 0..3 {
            let bx = self.mul(x);
            let correction = two.sub(bx);
            x = x.mul(correction);
        }
        x
    }

    /// Divide `self / other` at rung 2 precision.
    #[must_use]
    pub(crate) fn div<F: DecimalFormat>(self, other: Self) -> Self {
        if self.is_zero() {
            return Self::ZERO;
        }
        self.mul(other.recip::<F>())
    }

    /// Square root via Newton's method, seeded from the format's own
    /// `sqrt`; three halving steps (34 → 68 → 136 → 272 digit doubling
    /// through the division, past `EXT2_PRECISION`).
    #[must_use]
    pub(crate) fn sqrt<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.sign, "Extended2::sqrt of negative");
        if self.is_zero() {
            return self;
        }
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (seed_d, _) = self_d.sqrt_seed(RoundingMode::NearestEven);
        let mut x = Self::from_format::<F>(seed_d);
        for _ in 0..3 {
            let q = self.div::<F>(x);
            x = Self::HALF.mul(x.add(q));
        }
        x
    }

    /// Divide by a small positive `u32` divisor (Taylor denominators).
    #[must_use]
    pub(crate) fn div_u32(self, divisor: u32) -> Self {
        debug_assert!(divisor != 0, "div_u32: zero divisor");
        if self.is_zero() {
            return self;
        }

        // Scale up to EXT2_PRECISION + 2 digits before dividing, so the
        // quotient keeps EXT2_PRECISION + 1 digits and the round step
        // has a digit to inspect (112 ≤ the 115-digit U384 envelope).
        let dig = self.coef.decimal_digit_count();
        let target = EXT2_PRECISION + 2;
        let scale_up = target.saturating_sub(dig);

        let scaled = self.coef.mul_pow10(scale_up);
        let (q, r) = scaled.div_rem_u128(u128::from(divisor));
        let pre_sticky = r != 0;
        let new_exp = self.exp - scale_up as i32;

        let (rounded_coef, exp_shift) = round_u384_to_ext2(q, pre_sticky);
        Self {
            coef: rounded_coef,
            exp: new_exp + exp_shift as i32,
            sign: self.sign,
        }
    }

    /// Truncate toward zero into an `i32`; mirrors the rung-1 seam.
    pub(crate) fn trunc_to_i32(self) -> i32 {
        if self.is_zero() {
            return 0;
        }
        if self.exp >= 0 {
            let mut c = self.coef;
            for _ in 0..(self.exp as u32) {
                c = c.mul10();
            }
            let val = c.lo as i64;
            return if self.sign { -(val as i32) } else { val as i32 };
        }
        let mut c = self.coef;
        for _ in 0..((-self.exp) as u32) {
            let (q, _) = c.div_rem10();
            c = q;
        }
        let val = c.lo as i64;
        if self.sign {
            -(val as i32)
        } else {
            val as i32
        }
    }
}

// ----------------------------------------------------------------------------
// Width helpers, mirroring `round_u384_to_ext` / `round_u256_to_ext`.

/// Convert a `U768` whose upper limbs are zero to `U384`.
#[inline]
fn u768_to_u384(c: U768) -> U384 {
    debug_assert!(
        c.limbs[3] == 0 && c.limbs[4] == 0 && c.limbs[5] == 0,
        "u768_to_u384: upper limbs must be zero"
    );
    U384 {
        lo: c.limbs[0],
        mid: c.limbs[1],
        hi: c.limbs[2],
    }
}

/// Round a `U768` coefficient down to ≤ `EXT2_PRECISION` digits using
/// round-half-even. Returns the rounded `U384` and the exponent bump.
fn round_u768_to_ext2(mut coef: U768) -> (U384, u32) {
    let dig = coef.decimal_digit_count();
    if dig <= EXT2_PRECISION {
        // 110 digits ≈ 366 bits — inside U384.
        return (u768_to_u384(coef), 0);
    }
    let total_drop = dig - EXT2_PRECISION;
    let mut sticky = false;
    let mut round_digit = 0u32;
    let mut i = 0u32;
    while i < total_drop {
        let (q, d) = coef.div_rem10();
        coef = q;
        if i + 1 < total_drop {
            if d != 0 {
                sticky = true;
            }
        } else {
            round_digit = d;
        }
        i += 1;
    }

    let mut c = u768_to_u384(coef);
    let lsb = (c.lo & 1) as u32;
    let round_up = round_digit > 5 || (round_digit == 5 && (sticky || lsb == 1));
    if round_up {
        c = c.add(U384::from_u128(1));
        if c.decimal_digit_count() > EXT2_PRECISION {
            c = c.div_rem10().0;
            return (c, total_drop + 1);
        }
    }
    (c, total_drop)
}

/// Same as [`round_u768_to_ext2`] but starting from a `U384` (e.g. the
/// quotient of an integer division), with a caller-supplied sticky.
fn round_u384_to_ext2(mut coef: U384, pre_sticky: bool) -> (U384, u32) {
    let dig = coef.decimal_digit_count();
    if dig <= EXT2_PRECISION {
        // No round digit of our own; `pre_sticky` alone never rounds
        // up here (same contract as rung 1's `round_u256_to_ext`).
        let _ = pre_sticky;
        return (coef, 0);
    }
    let total_drop = dig - EXT2_PRECISION;
    let mut sticky = pre_sticky;
    let mut round_digit = 0u32;
    let mut i = 0u32;
    while i < total_drop {
        let (q, d) = coef.div_rem10();
        coef = q;
        if i + 1 < total_drop {
            if d != 0 {
                sticky = true;
            }
        } else {
            round_digit = d;
        }
        i += 1;
    }

    let lsb = (coef.lo & 1) as u32;
    let round_up = round_digit > 5 || (round_digit == 5 && (sticky || lsb == 1));
    if round_up {
        coef = coef.add(U384::from_u128(1));
        if coef.decimal_digit_count() > EXT2_PRECISION {
            coef = coef.div_rem10().0;
            return (coef, total_drop + 1);
        }
    }
    (coef, total_drop)
}

// ----------------------------------------------------------------------------
// The ExtNum seam: rung 2 speaks the same contract as rung 1.

// Rung 2's width is fixed at `EXT2_PRECISION`, so every
// exemplar-relative member ignores its receiver and delegates verbatim
// to the inherent surface, exactly as rung 1 does.
impl ExtNum for Extended2 {
    fn precision(&self) -> u32 {
        EXT2_PRECISION
    }

    // Series caps sized for 110-digit convergence with the same safety
    // ratios the rung-1 caps carry over their needed term counts:
    //
    // * exp, |r| ≤ ln(10)/2: the term drops below 10^-115 near n ≈ 85
    //   (rung 1 needs ~36 of its 60).
    // * sin/cos and sinh/cosh, |r| ≤ π/4 resp. |x| < 0.5: below
    //   10^-115 near n ≈ 40 (rung 1 needs ~20 of its 120).
    // * log1p, |u| ≤ 0.5: n ≳ 115 · log2(10) ≈ 382 (rung 1's comment
    //   derives 166 of its 250).
    // * atan, |t| ≤ tan(π/8): (2n+1) · log10(1/tan(π/8)) ≥ 115 gives
    //   n ≈ 150 (rung 1 needs ~65 of its 200).
    //
    // Every loop still exits early on `next_sum == sum`, so the caps
    // are convergence backstops, not iteration counts.
    fn exp_series_terms(&self) -> u32 {
        120
    }
    fn sin_cos_series_terms(&self) -> u32 {
        240
    }
    fn sinh_cosh_series_terms(&self) -> u32 {
        240
    }
    fn log1p_series_terms(&self) -> u32 {
        550
    }
    fn atan_series_terms(&self) -> u32 {
        450
    }

    fn zero(&self) -> Self {
        Extended2::ZERO
    }
    fn one(&self) -> Self {
        Extended2::ONE
    }
    fn half(&self) -> Self {
        Extended2::HALF
    }

    fn pi(&self) -> Self {
        crate::consts::pi_ext2()
    }
    fn e(&self) -> Self {
        crate::consts::e_ext2()
    }
    fn ln2(&self) -> Self {
        crate::consts::ln2_ext2()
    }
    fn ln10(&self) -> Self {
        crate::consts::ln10_ext2()
    }
    fn inv_ln10(&self) -> Self {
        crate::consts::inv_ln10_ext2()
    }
    fn inv_ln2(&self) -> Self {
        crate::consts::inv_ln2_ext2()
    }
    fn inv_pi(&self) -> Self {
        crate::consts::inv_pi_ext2()
    }
    fn pi_over_two(&self) -> Self {
        crate::consts::pi_over_two_ext2()
    }
    fn pi_over_four(&self) -> Self {
        crate::consts::pi_over_four_ext2()
    }
    fn tan_pi_over_eight(&self) -> Self {
        crate::consts::tan_pi_over_eight_ext2()
    }

    fn from_i32(&self, n: i32) -> Self {
        Extended2::from_i32(n)
    }
    fn parse_str(&self, s: &str) -> Self {
        Extended2::parse_str(s)
    }
    fn from_parts_u128(&self, coef: u128, exp: i32, sign: bool) -> Self {
        Self {
            coef: U384::from_u128(coef),
            exp,
            sign,
        }
    }
    fn from_components_with_sticky(
        &self,
        coef: U256,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self {
        Extended2::from_components_with_sticky(U384::from_u256(coef), exp, sign, pre_sticky)
    }
    fn from_format<F: DecimalFormat>(&self, d: F) -> Self {
        Extended2::from_format(d)
    }
    fn from_extended(&self, x: Extended) -> Self {
        Extended2::from_extended(x)
    }
    fn saturate_overflow(&self, sign: bool) -> Self {
        Extended2::saturate_overflow(sign)
    }
    fn saturate_underflow(&self) -> Self {
        Extended2::saturate_underflow()
    }

    fn sign(self) -> bool {
        self.sign
    }
    fn exponent(self) -> i32 {
        self.exp
    }
    fn digit_count(self) -> u32 {
        self.coef.decimal_digit_count()
    }
    fn is_zero(self) -> bool {
        Extended2::is_zero(self)
    }
    fn with_sign(self, sign: bool) -> Self {
        Self { sign, ..self }
    }
    fn with_exponent(self, exp: i32) -> Self {
        Self { exp, ..self }
    }

    fn neg(self) -> Self {
        Extended2::neg(self)
    }
    fn abs(self) -> Self {
        Extended2::abs(self)
    }
    fn add(self, other: Self) -> Self {
        Extended2::add(self, other)
    }
    fn sub(self, other: Self) -> Self {
        Extended2::sub(self, other)
    }
    fn mul(self, other: Self) -> Self {
        Extended2::mul(self, other)
    }
    fn square(self) -> Self {
        Extended2::square(self)
    }
    fn div<F: DecimalFormat>(self, other: Self) -> Self {
        Extended2::div::<F>(self, other)
    }
    fn recip<F: DecimalFormat>(self) -> Self {
        Extended2::recip::<F>(self)
    }
    fn sqrt<F: DecimalFormat>(self) -> Self {
        Extended2::sqrt::<F>(self)
    }
    fn div_u32(self, divisor: u32) -> Self {
        Extended2::div_u32(self, divisor)
    }
    fn mul_pow10_exp(self, k: i32) -> Self {
        Extended2::mul_pow10_exp(self, k)
    }

    fn cmp(self, other: Self) -> Ordering {
        Extended2::cmp(self, other)
    }

    fn trunc_to_i32(self) -> i32 {
        Extended2::trunc_to_i32(self)
    }

    fn to_format<F: DecimalFormat>(self, q_preferred: i32, rm: RoundingMode) -> (F, Status) {
        Extended2::to_format::<F>(self, q_preferred, rm)
    }
    fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        Extended2::to_format_with_residual::<F>(self, magnitude_grows, rm)
    }
    fn sticks_to(self, anchor: Self) -> bool {
        Extended2::sticks_to(self, anchor)
    }
    fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        Extended2::near_rounding_boundary::<F>(self, budget)
    }
    fn candidate_boundary<F: DecimalFormat>(self, budget: u128) -> crate::ladder::BoundaryVerdict {
        Extended2::candidate_boundary::<F>(self, budget)
    }
    fn nearest_boundary<F: DecimalFormat>(self) -> crate::ladder::BoundaryVerdict {
        Extended2::nearest_boundary::<F>(self)
    }

    // Top fixed rung. Without the `unbounded-ladder` feature its
    // delivery is unconditional (the Tier 2 model) and the rung-2
    // budget feeds only the `ladder_audit` ambiguity check. With the
    // feature there is a wider rung to escalate to, so a near-boundary
    // verdict here escalates exactly as rung 1's does — that is the
    // whole meaning of "no exception set".
    const ESCALATES: bool = cfg!(feature = "unbounded-ladder");
    const RUNG: u8 = 2;
    fn rung_budget(&self, budget: &crate::ladder::Budget) -> u128 {
        budget.rung2
    }
    #[cfg(feature = "trig")]
    fn reduce_trig<F: DecimalFormat>(&self, x: F) -> (u32, Self, Status) {
        crate::argred::reduce_wide::<F>(x)
    }
}

// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use crate::mock_format::MockFmt;
    use alloc::string::String;

    fn ext2(s: &str) -> Extended2 {
        Extended2::parse_str(s)
    }

    fn assert_ext2_eq(got: Extended2, want: Extended2, label: &str) {
        assert_eq!(
            got.cmp(want),
            Ordering::Equal,
            "{label}: got {got:?}, want {want:?}"
        );
    }

    #[test]
    fn add_basic() {
        assert_ext2_eq(ext2("1.5").add(ext2("2.25")), ext2("3.75"), "1.5 + 2.25");
    }

    #[test]
    fn sub_basic() {
        assert_ext2_eq(ext2("3.75").sub(ext2("1.25")), ext2("2.5"), "3.75 - 1.25");
    }

    #[test]
    fn mul_basic() {
        assert_ext2_eq(ext2("3.5").mul(ext2("4.0")), ext2("14.0"), "3.5 * 4");
    }

    #[test]
    fn mul_high_precision_carries() {
        // (10^55)² = 10^110, exactly at the EXT2_PRECISION boundary.
        assert_ext2_eq(
            ext2("1e55").mul(ext2("1e55")),
            ext2("1e110"),
            "10^55 squared",
        );
    }

    #[test]
    fn div_u32_basic() {
        // 10/3 to 110 digits: "3." followed by 109 more 3s.
        let mut want = String::from("3.");
        for _ in 0..109 {
            want.push('3');
        }
        assert_ext2_eq(ext2("10").div_u32(3), ext2(&want), "10 / 3");
    }

    #[test]
    fn div_u32_terminates_clean() {
        assert_ext2_eq(ext2("100").div_u32(4), ext2("25"), "100 / 4");
    }

    #[test]
    fn cmp_signs() {
        assert_eq!(ext2("1").cmp(ext2("2")), Ordering::Less);
        assert_eq!(ext2("-1").cmp(ext2("2")), Ordering::Less);
        assert_eq!(ext2("-1").cmp(ext2("-2")), Ordering::Greater);
        assert_eq!(ext2("0").cmp(ext2("0")), Ordering::Equal);
        assert_eq!(ext2("0").cmp(ext2("0").neg()), Ordering::Equal);
    }

    #[test]
    fn add_cancellation_preserves_working_precision() {
        // 1 − (1 − 1e-100) recovers 1e-100 exactly: the wider envelope
        // holds every digit of the cancellation (the rung-1 mirror test
        // uses 1e-40 against 50 digits).
        let one = ext2("1");
        let tiny = ext2("1e-100");
        let restored = one.sub(one.sub(tiny));
        assert_ext2_eq(restored, tiny, "1 - (1 - 1e-100)");
    }

    #[test]
    fn from_extended_is_value_exact() {
        for s in ["3.14159265358979323846", "-0.5", "1e-30", "9.99e6000"] {
            let e1 = Extended::parse_str(s);
            let e2 = Extended2::from_extended(e1);
            assert_ext2_eq(e2, ext2(s), s);
        }
        assert!(Extended2::from_extended(Extended::ZERO).is_zero());
    }

    #[test]
    fn trunc_to_i32_truncates_toward_zero() {
        let cases = [
            ("0", 0),
            ("1", 1),
            ("-1", -1),
            ("6144.999999999999999999999999", 6144),
            ("-0.5", 0),
            ("123.456", 123),
            ("1e3", 1000),
            ("-2.5e2", -250),
        ];
        for (s, want) in cases {
            assert_eq!(ext2(s).trunc_to_i32(), want, "input {s}");
        }
    }

    #[test]
    fn sticks_to_threshold_is_ext2_scaled() {
        let one = Extended2::ONE;
        // 1 + 1e-108: inside the ~10^-107 snap band.
        let mut close = String::from("1.");
        for _ in 0..107 {
            close.push('0');
        }
        close.push('1');
        assert!(ext2(&close).sticks_to(one));
        // 1 + 1e-50: separated at rung 2 width (this is exactly what
        // escalation buys: rung 1 would have snapped it).
        assert!(!ext2("1.00000000000000000000000000000000000000000000000001").sticks_to(one));
    }

    #[test]
    fn series_caps_pin_rung2_values() {
        let ex = Extended2::ZERO;
        assert_eq!(ex.exp_series_terms(), 120);
        assert_eq!(ex.sin_cos_series_terms(), 240);
        assert_eq!(ex.sinh_cosh_series_terms(), 240);
        assert_eq!(ex.log1p_series_terms(), 550);
        assert_eq!(ex.atan_series_terms(), 450);
        assert_eq!(ex.precision(), EXT2_PRECISION);
    }

    /// The candidate boundary's identity at the consuming rung (the
    /// ADR-0060 adjudicator reads exactly this payload): each family
    /// pinned at the 76-digit d128 drop, the borrow and carry
    /// spellings converging on the same `Boundary` value.
    #[test]
    fn candidate_boundary_identities_d128_drop() {
        use crate::ladder::{Boundary, BoundaryKind, BoundaryVerdict};
        type Shape = MockFmt<34, 6176>;

        const PREFIX: u128 = 1_234_567_890_123_456_789_012_345_678_901_234;
        let half = U384::from_u128(5).mul_pow10(75);
        let field = U384::from_u128(1).mul_pow10(76);
        let exp = -50;
        let exp_grid = exp + 76;

        let near = |base: U384, off: i128| {
            let stem = U384::from_u128(PREFIX).mul_pow10(76).add(base);
            let coef = if off >= 0 {
                stem.add(U384::from_u128(off as u128))
            } else {
                stem.sub(U384::from_u128(off.unsigned_abs()))
            };
            let v = Extended2 {
                coef,
                exp,
                sign: false,
            };
            match v.candidate_boundary::<Shape>(3) {
                BoundaryVerdict::Near(b) => b,
                v => panic!("expected Near, got {v:?}"),
            }
        };

        let lower = Boundary {
            coef: PREFIX,
            exp: exp_grid,
            kind: BoundaryKind::Grid,
        };
        for off in [0i128, 1, 3, -1, -3] {
            assert_eq!(near(U384::ZERO, off), lower, "off={off}");
        }
        let mid = Boundary {
            coef: 10 * PREFIX + 5,
            exp: exp_grid - 1,
            kind: BoundaryKind::Midpoint,
        };
        for off in [-3i128, 0, 3] {
            assert_eq!(near(half, off), mid, "off={off}");
        }
        let upper = Boundary {
            coef: PREFIX + 1,
            exp: exp_grid,
            kind: BoundaryKind::Grid,
        };
        for off in [-3i128, -1, 0, 1] {
            assert_eq!(near(field, off), upper, "off={off}");
        }

        // Far and degenerate arms.
        let interior = Extended2 {
            coef: U384::from_u128(PREFIX)
                .mul_pow10(76)
                .add(U384::from_u128(4)),
            exp,
            sign: false,
        };
        assert_eq!(
            interior.candidate_boundary::<Shape>(3),
            BoundaryVerdict::Clear
        );
        assert_eq!(
            Extended2::ZERO.candidate_boundary::<Shape>(u128::MAX),
            BoundaryVerdict::NearIndeterminate
        );
    }

    #[test]
    fn near_rounding_boundary_bands_d128_drop() {
        // At EXT2 width the d128 drop is 110 − 34 = 76 digits;
        // construct prefix · 10^76 + base ± off and pin the band
        // distances, mirroring the rung-1 subnormal test's shape.
        type Shape = MockFmt<34, 6176>;

        let prefix: u128 = 1_234_567_890_123_456_789_012_345_678_901_234;
        let half = U384::from_u128(5).mul_pow10(75);
        let field = U384::from_u128(1).mul_pow10(76);
        for base in [U384::ZERO, half, field] {
            for off in [-3i128, -1, 0, 1, 3] {
                let stem = U384::from_u128(prefix).mul_pow10(76).add(base);
                let coef = if off >= 0 {
                    stem.add(U384::from_u128(off as u128))
                } else {
                    stem.sub(U384::from_u128(off.unsigned_abs()))
                };
                let v = Extended2 {
                    coef,
                    exp: -50,
                    sign: false,
                };
                assert_eq!(
                    v.near_rounding_boundary::<Shape>(3),
                    off.unsigned_abs() <= 3,
                    "off={off}"
                );
                if off != 0 {
                    assert!(!v.near_rounding_boundary::<Shape>(off.unsigned_abs() - 1));
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Oracle cross-check vs astro-float at 450 bits (~135 decimal
    // digits): basic ops agree to 1 ULP at 110-digit precision,
    // mirroring the rung-1 suite's 300-bit / 1e-49 discipline.

    const ORACLE_P: usize = 450;

    fn ext2_to_string(e: Extended2) -> String {
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

    fn parse_af(s: &str, cc: &mut astro_float::Consts) -> astro_float::BigFloat {
        astro_float::BigFloat::parse(
            s,
            astro_float::Radix::Dec,
            ORACLE_P,
            astro_float::RoundingMode::None,
            cc,
        )
    }

    fn af_diff_below_ulp_110(a: &astro_float::BigFloat, b: &astro_float::BigFloat) -> bool {
        use astro_float::{BigFloat, RoundingMode as AfRm};
        let rm = AfRm::None;
        let mut cc = astro_float::Consts::new().unwrap();
        let diff = a.sub(b, ORACLE_P, rm).abs();
        let abs_b = b.abs();
        if abs_b.cmp(&BigFloat::from(0)) == Some(0) {
            let bound = parse_af("1e-109", &mut cc);
            return matches!(diff.cmp(&bound), Some(o) if o <= 0);
        }
        let rel = diff.div(&abs_b, ORACLE_P, rm);
        let bound = parse_af("1e-109", &mut cc);
        matches!(rel.cmp(&bound), Some(o) if o <= 0)
    }

    #[test]
    fn oracle_add_small_random() {
        let pairs = [
            ("1.5", "2.25"),
            ("0.1", "0.2"),
            ("1e60", "1e-60"),
            ("999.9999999999999", "0.0000000000000001"),
            ("-3.5", "5.25"),
            ("1.234567890123456789012345678901234", "1e-110"),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, b_s) in pairs {
            let got = ext2(a_s).add(ext2(b_s));
            let got_af = parse_af(&ext2_to_string(got), &mut cc);
            let a_af = parse_af(a_s, &mut cc);
            let b_af = parse_af(b_s, &mut cc);
            let want_af = a_af.add(&b_af, ORACLE_P, astro_float::RoundingMode::None);
            assert!(
                af_diff_below_ulp_110(&got_af, &want_af),
                "add({a_s}, {b_s}) exceeds 1 ULP at 110 digits"
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
            ("1e55", "1e-55"),
            ("-1.5", "1.5"),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, b_s) in pairs {
            let got = ext2(a_s).mul(ext2(b_s));
            let got_af = parse_af(&ext2_to_string(got), &mut cc);
            let a_af = parse_af(a_s, &mut cc);
            let b_af = parse_af(b_s, &mut cc);
            let want_af = a_af.mul(&b_af, ORACLE_P, astro_float::RoundingMode::None);
            assert!(
                af_diff_below_ulp_110(&got_af, &want_af),
                "mul({a_s}, {b_s}) exceeds 1 ULP at 110 digits"
            );
        }
    }

    #[test]
    fn oracle_div_u32_small() {
        let cases = [
            ("10", 3u32),
            ("1", 7),
            ("355", 113),
            ("1.234567890123456789012345678901234", 17),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, d) in cases {
            let got = ext2(a_s).div_u32(d);
            let got_af = parse_af(&ext2_to_string(got), &mut cc);
            let a_af = parse_af(a_s, &mut cc);
            let d_af = astro_float::BigFloat::from_word(u64::from(d), ORACLE_P);
            let want_af = a_af.div(&d_af, ORACLE_P, astro_float::RoundingMode::None);
            assert!(
                af_diff_below_ulp_110(&got_af, &want_af),
                "div_u32({a_s}, {d}) exceeds 1 ULP at 110 digits"
            );
        }
    }

    #[test]
    fn oracle_wide_constant_product() {
        // π_ext2 · e_ext2 against the oracle's independently derived
        // π · e: one 110-digit rounding on top of two 115-digit
        // literals stays within 1 ULP at 110 digits. Exercises the
        // wide constants and the U768 product path together without
        // needing a format seed.
        let mut cc = astro_float::Consts::new().unwrap();
        let pi2 = crate::consts::pi_ext2();
        let e2 = crate::consts::e_ext2();
        let prod = pi2.mul(e2);
        let got_af = parse_af(&ext2_to_string(prod), &mut cc);
        let pi_af = cc.pi(ORACLE_P, astro_float::RoundingMode::None);
        let one = parse_af("1", &mut cc);
        let e_af = one.exp(ORACLE_P, astro_float::RoundingMode::None, &mut cc);
        let want_af = pi_af.mul(&e_af, ORACLE_P, astro_float::RoundingMode::None);
        assert!(
            af_diff_below_ulp_110(&got_af, &want_af),
            "pi * e exceeds 1 ULP at 110 digits"
        );
    }
}
