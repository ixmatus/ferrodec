//! Moved from `ferrodec/src/math/extended.rs` @ commit 82a7fe1 (P0a.2 c3). Behaviour-neutral: genericized over [`DecimalFormat`]; the `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! Extended-precision intermediate for transcendentals.
//!
//! ## Why
//!
//! `Decimal128`'s 34-digit envelope is not wide enough to deliver
//! correctly rounded transcendentals (ADR-0032). Each Taylor / Newton
//! / argument-reduction step accumulates ~0.5 ULP of error; a 30-term
//! series evaluated at 34 digits drifts ~15 ULP relative to the true
//! result, far beyond even the weaker faithful (≤ 1 ULP) bound.
//!
//! [`Extended`] gives 50-digit working precision: 16 extra digits over
//! `Decimal128`, 34 over `Decimal64`, 43 over `Decimal32`. The
//! resulting cumulative kernel error budget exceeds the smallest
//! empirical Arb worst-case half-ULP margin (`4.167e-8` for `cosh` at
//! Decimal32, the binding case across the family) by more than thirty
//! orders of magnitude on every format, which is the proof envelope
//! for the correctly rounded §9.2 contract; ADR-0032 §Decision records
//! the per function derivation.
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
//! filter them at the format boundary.
//!
//! ## Operations
//!
//! All binary ops produce a normalised result (≤ 50-digit `coef`)
//! using round-half-even on the discarded digits. There is no
//! tracking of `INEXACT` here — the only `Status` we emit is at the
//! boundary (`to_format`), where the prior intermediate rounding
//! already reflects the precision loss.
//!
//! ## Status
//!
//! The Extended intermediate is the working representation for every
//! §9.2 transcendental in this crate; the kernel modules (`exp`,
//! `ln`, `cbrt`, `sincos`, `inverse_trig`, `hyperbolic`, `pow`) all
//! evaluate at this width before rounding back to the format. The
//! migrations off the pre-fd-r0l f64 / `libm` detour landed in
//! fd-r0l; ADR-0032 (Phase D) tightened the resulting kernel's
//! contract from faithful to correctly rounded. The
//! `#[allow(dead_code)]` retained at the crate root reflects that
//! some Extended helpers are exercised only by unit tests, not by
//! every kernel path.

#![allow(dead_code)]

use crate::format::DecimalFormat;
use core::cmp::Ordering;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::{u256::widening_mul_u128, U256, U384};

/// Working precision in decimal digits.
pub(crate) const EXT_PRECISION: u32 = 50;

#[derive(Clone, Copy, Debug)]
pub struct Extended {
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

    /// `0.5`. Used by `exp`'s argument-reduction half-shift and by the
    /// `|x| < 0.5` Taylor-vs-cancellation switch in `sinh` / `cosh`.
    pub const HALF: Self = Self {
        coef: U256::from_u128(5),
        exp: -1,
        sign: false,
    };

    /// An `Extended` whose magnitude exceeds `Decimal128::MAX` (`10^6144`)
    /// by enough that the boundary `to_format` round produces the IEEE
    /// 754-2019 §7.4 overflow disposition for the rounding direction
    /// (`±∞` at the nearest modes and toward the overflowing side, the
    /// largest finite magnitude toward zero and the opposite side),
    /// with `OVERFLOW` raised by the format rounder. Used by `sinh` /
    /// `cosh` and the `exp` family gate to signal saturation when `|x|`
    /// is past the `exp` convergence window.
    ///
    /// The exponent (`7000`) is chosen comfortably above `E_MAX = 6144`;
    /// any value past `MAX` rounds the same way at the boundary, so the
    /// exact figure is just a documentation-friendly margin.
    #[inline]
    pub const fn saturate_overflow(sign: bool) -> Self {
        Self {
            coef: U256::from_u128(1),
            exp: 7000,
            sign,
        }
    }

    /// The underflow counterpart of [`Self::saturate_overflow`]: a
    /// positive magnitude (`10^-7000`) below every format's smallest
    /// subnormal (`Decimal128`'s floor is `1 × 10^-6176`). Routed
    /// through `round_and_pack_finite` with `pre_sticky = true`, the
    /// format rounder delivers the §7.4 underflow disposition for the
    /// rounding direction (`+0` at the nearest modes and toward zero
    /// or `-∞`, the smallest subnormal toward `+∞`) and raises
    /// `UNDERFLOW`. Callers must only saturate when the true result is
    /// already below half the smallest subnormal, so the nearest-mode
    /// answer is genuinely zero; the gate thresholds encode that.
    #[inline]
    pub const fn saturate_underflow() -> Self {
        Self {
            coef: U256::from_u128(1),
            exp: -7000,
            sign: false,
        }
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.coef.is_zero()
    }

    /// Negate. Zero stays positive (canonical representation).
    #[inline]
    #[must_use]
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
    #[must_use]
    pub fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }

    /// Build from a finite or zero format datum. Panics on NaN / Inf —
    /// callers must dispatch those at the public-API boundary.
    pub fn from_format<F: DecimalFormat>(d: F) -> Self {
        let (coef, exp, sign) = d.to_extended_parts().expect(
            "from_format requires a finite or zero datum; NaN / Inf are \
             dispatched at the public-API boundary",
        );
        Self { coef, exp, sign }
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
        // Slice E.4 guard: the kernels downstream of this parser assume a
        // working precision of `EXT_PRECISION = 50` digits. Hand-curated
        // literals usually overshoot slightly (the `*_EXT_STR` constants
        // in `src/math/consts.rs` are 55 digits) but a literal much wider
        // than that would silently bypass the precision invariant and
        // could surprise a caller that assumes the parsed value fits the
        // envelope. Tolerate up to 5 extra digits before debug-asserting;
        // production callers (`Extended::round_to_precision`) re-narrow
        // before any kernel consumes the value.
        debug_assert!(
            coef.decimal_digit_count() <= EXT_PRECISION + 5,
            "Extended::parse_str: literal exceeds EXT_PRECISION + 5; \
             round it through Extended::round_to_precision or trim the source"
        );
        Self {
            coef,
            exp: exp_explicit - digits_after_point,
            sign,
        }
    }

    /// Multiply by `10^k` (k may be negative). This is a pure
    /// exponent shift — no rounding, no coefficient change.
    #[must_use]
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

    /// Convert to a format datum. `q_preferred` is the IEEE 754 §6.3
    /// preferred quantum exponent for the operation that built this
    /// value (callers typically pass `0` for transcendentals or pass
    /// through the source operand's quantum for identity-like ops).
    pub fn to_format<F: DecimalFormat>(self, q_preferred: i32, rm: RoundingMode) -> (F, Status) {
        F::round_and_pack_finite(
            self.coef,
            self.exp,
            q_preferred,
            self.sign,
            false,
            rm,
            Status::OK,
        )
    }

    /// The ADR-0051 grid-stuck snap test: `true` when `self` lies
    /// within ~10^-47 relative of `anchor` (a signed comparison; the
    /// caller passes the anchor with the result's sign).
    ///
    /// The threshold separates two regimes by a wide margin on each
    /// side. Composition noise — the few units in the 50th
    /// significant digit that a multi-step derivation (division,
    /// series, halving) can leave around an anchor it mathematically
    /// hugs — is at most ~10 such units (~10^-49 relative), so any
    /// result whose *side* relative to the anchor is unreliable is
    /// snapped. A genuinely separated result sits at least one format
    /// half-ULP from the nearest grid point by the ADR-0033 empirical
    /// worst-case margins (≥ ~10^-9 ULP, i.e. ≥ ~10^-42 relative at
    /// the widest format), so it is never snapped and the bare value
    /// decides every mode itself. In between, both treatments agree:
    /// a snapped value within the threshold rounds identically to the
    /// true result at every direction and format precision, because
    /// both lie strictly between the same format grid points on the
    /// theorem side of the anchor.
    #[must_use]
    pub fn sticks_to(self, anchor: Extended) -> bool {
        let d = self.sub(anchor);
        if d.is_zero() {
            return true;
        }
        let d_adj = d.exp + d.coef.decimal_digit_count() as i32 - 1;
        let a_adj = anchor.exp + anchor.coef.decimal_digit_count() as i32 - 1;
        d_adj <= a_adj - 47
    }

    /// Convert to a format datum an *anchor* value (the kernel's
    /// grid point: the input `x` for the `f(x) ≈ x` family, ±1 for
    /// the families anchored there) whose true result lies strictly
    /// on the `magnitude_grows` side, within the [`Self::sticks_to`]
    /// snap band (ADR-0051).
    ///
    /// The bare [`Self::to_format`] cannot express that knowledge:
    /// the anchor sits exactly on a format grid point, so the four
    /// directed rounding directions need the residual's side to pick
    /// between the grid point and its neighbour. The encoding reuses
    /// the rounding kernel's own enclosure channel: the coefficient
    /// is widened to the full `EXT_PRECISION` digits (exact), and
    /// `pre_sticky = true` then denotes the open interval one
    /// unit-in-the-last-place wide on the chosen side
    /// (`magnitude_grows`: above `self`; otherwise the widened
    /// coefficient is first decremented, denoting the interval
    /// below). The denoted interval and the true result round
    /// identically at every direction and format precision: both lie
    /// strictly between the same format grid points on the same side
    /// of the anchor, since the snap band (~10^-47 relative) is at
    /// least thirteen orders of magnitude narrower than any format's
    /// ULP.
    ///
    /// Caller guarantees `self` is nonzero, on-grid-or-anchor, and
    /// the side theorem (`|sin x| < |x|`, `cosh x > 1`, ...).
    pub fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        debug_assert!(!self.is_zero(), "residual rounding needs a nonzero value");
        let dig = self.coef.decimal_digit_count();
        let scale = EXT_PRECISION - dig;
        let coef50 = self.coef.mul_pow10(scale);
        let exp50 = self.exp - scale as i32;
        let coef = if magnitude_grows {
            coef50
        } else {
            coef50.sub(U256::from_u128(1))
        };
        F::round_and_pack_finite(coef, exp50, 0, self.sign, true, rm, Status::OK)
    }

    /// Mode-independent escalation predicate: `true` when the format
    /// rounding of `self` is not decided by a bracket of half-width
    /// `budget` units in the last place of the widened working value —
    /// i.e. when some value within that bracket rounds differently
    /// from `self` (result bits or status) in at least one of the five
    /// rounding modes.
    ///
    /// ## Contract
    ///
    /// * **Units.** The coefficient is first widened to exactly
    ///   `EXT_PRECISION` digits (the same normalisation as
    ///   [`Self::to_format_with_residual`]), so one budget unit is one
    ///   unit in the 50th significant digit of `self` regardless of
    ///   the stored digit count. A rung's per-function error budget is
    ///   a bound in these units.
    /// * **Both boundary families, every mode.** The rounder's
    ///   decision boundaries at the drop position are the format grid
    ///   points (tail ≡ 0, deciding the directed modes and `INEXACT`)
    ///   and the midpoints between them (tail ≡ 5·10^(drop−1),
    ///   deciding the nearest modes). Both families are tested
    ///   unconditionally, so escalation is a deterministic function of
    ///   the input alone — never of the caller's rounding mode.
    /// * **Drop-position fidelity.** The drop position mirrors
    ///   `round_and_pack_finite`'s single-rounding rule (fd-42l): the
    ///   wider of the precision excess and the subnormal quantum
    ///   excess `qmin − exp`. Mirroring the subnormal branch is what
    ///   closes the subnormal-edge tininess hazard: a value within
    ///   `budget` of the `10^E_MIN` decade point sits within `budget`
    ///   of a grid point (the decade point is representable), so any
    ///   input whose `UNDERFLOW` flag the bracket cannot decide is
    ///   escalated by the grid family.
    /// * **Sign is ignored.** The boundary set is symmetric in
    ///   magnitude; sign selects *which* directed mode moves at a
    ///   crossing, never *whether* one does.
    /// * **Zero escalates.** A zero working value has no exponent to
    ///   define the unit, and sits on the grid point where even the
    ///   result's sign is undecidable from a bracket; only upstream
    ///   classification can certify an exact zero.
    /// * Models the `pre_sticky = false` boundary ([`Self::to_format`]).
    ///   The anchor-residual path ([`Self::to_format_with_residual`],
    ///   ADR-0051) runs *before* any predicate call and never consults
    ///   it.
    ///
    /// A caller that receives `false` may deliver rung 1's rounding
    /// unconditionally, provided the rung's true-error bound is at
    /// most `budget` units. `true` does not assert a genuine boundary
    /// case — only that this rung's bracket cannot exclude one.
    #[must_use]
    pub fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        if self.is_zero() {
            return true;
        }
        // Normalize to the rung width first. Kernel arithmetic keeps
        // coefficients ≤ EXT_PRECISION digits, but a value delivered
        // straight from a hand-curated constant carries up to
        // `EXT_PRECISION + 5` (the `parse_str` envelope — `atan(1)`
        // returns the 55-digit π/4 literal verbatim). Rounding those
        // extra digits away perturbs the measured distance by ≤ 1
        // unit, absorbed by every real budget (all ≥ 10^4); *not*
        // normalizing would silently shrink the budget unit by up to
        // 10^5 and under-escalate (M8).
        let (coef, exp) = if self.coef.decimal_digit_count() > EXT_PRECISION {
            let mut wide = U384::from_u256(self.coef);
            let (c, shift) = round_u384_to_ext(&mut wide);
            (c, self.exp + shift as i32)
        } else {
            (self.coef, self.exp)
        };
        let dig = coef.decimal_digit_count();

        // Widen to exactly EXT_PRECISION digits so the budget unit is
        // uniform across inputs (cf. to_format_with_residual).
        let scale = EXT_PRECISION.saturating_sub(dig);
        let coef_w = coef.mul_pow10(scale);
        let exp_w = exp - scale as i32;
        let digits = dig + scale;

        // The fd-42l single-rounding drop position: the wider of the
        // precision excess and the subnormal quantum excess. Mirrors
        // round_and_pack_finite Step 1 verbatim.
        let qmin = -F::BIAS;
        let precision_excess = digits.saturating_sub(F::PRECISION);
        let subnormal_excess = u32::try_from((qmin - exp_w).max(0)).unwrap_or(u32::MAX);
        let excess = precision_excess.max(subnormal_excess);

        if excess == 0 {
            // Every working digit survives the format rounding: the
            // value sits exactly on a format grid point (distance 0).
            // Unreachable for the real formats (their precision excess
            // is ≥ 16); kept total for hypothetical wide formats.
            return true;
        }
        if excess > digits {
            // Full drop, strictly: the kept value is zero and the
            // nearest boundary is the zero grid point, a full widened
            // coefficient away — at least 10^(EXT_PRECISION − 1) units.
            // `budget: u128` cannot express that distance (10^49 >
            // u128::MAX ≈ 3.4·10^38), so the answer is `false` by the
            // budget's type alone. The `excess == digits` full drop
            // (round digit at the MSD) stays on the general path below.
            return false;
        }

        // tail = coef_w mod 10^excess, extracted by the same div_rem10
        // walk the production rounder uses to drop digits.
        let mut kept = coef_w;
        let mut i = 0u32;
        while i < excess {
            kept = kept.div_rem10().0;
            i += 1;
        }
        let tail = coef_w.sub(kept.mul_pow10(excess));
        let field = U256::from_u128(1).mul_pow10(excess); // 10^excess ≤ 10^50: fits U256
        let half = U256::from_u128(5).mul_pow10(excess - 1);

        let bound = U256::from_u128(budget);
        let within = |d: U256| d.cmp(bound) != Ordering::Greater;
        let dist_mid = if tail.cmp(half) == Ordering::Less {
            half.sub(tail)
        } else {
            tail.sub(half)
        };
        within(tail) || within(field.sub(tail)) || within(dist_mid)
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

    #[must_use]
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

    #[must_use]
    pub fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }

    #[must_use]
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
    #[must_use]
    pub fn square(self) -> Self {
        self.mul(self)
    }

    /// Reciprocal (`1 / self`) via Newton-Raphson refinement.
    ///
    /// Seed with the format-rounded reciprocal (≥ 33 digits of
    /// initial precision). Each Newton step `x → x · (2 − b · x)`
    /// roughly doubles the precision; two steps take 33 → ~66 → ~132
    /// digits, comfortably past `EXT_PRECISION = 50`.
    ///
    /// Caller must ensure `self` is non-zero.
    #[must_use]
    pub fn recip<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.is_zero(), "Extended::recip on zero");
        // Seed: 1 / self at format precision.
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (recip_d, _) = self_d.recip_seed(RoundingMode::NearestEven);
        let mut x = Self::from_format::<F>(recip_d);
        let two = Self::from_i32(2);

        for _ in 0..2 {
            let bx = self.mul(x);
            let correction = two.sub(bx);
            x = x.mul(correction);
        }
        x
    }

    /// Divide `self / other` at extended precision.
    #[must_use]
    pub fn div<F: DecimalFormat>(self, other: Self) -> Self {
        if self.is_zero() {
            return Self::ZERO;
        }
        self.mul(other.recip::<F>())
    }

    /// Square root via Newton's method, seeded from the format's
    /// own `sqrt`. Caller must ensure `self` is non-negative
    /// and non-zero.
    ///
    /// One Newton iteration `x → 0.5 · (x + self/x)` doubles precision
    /// from the 33-digit seed to ~66 digits — past `EXT_PRECISION` = 50.
    #[must_use]
    pub fn sqrt<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.sign, "Extended::sqrt of negative");
        if self.is_zero() {
            return self;
        }
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (seed_d, _) = self_d.sqrt_seed(RoundingMode::NearestEven);
        let mut x = Self::from_format::<F>(seed_d);
        let half = Self {
            coef: U256::from_u128(5),
            exp: -1,
            sign: false,
        };
        for _ in 0..2 {
            let q = self.div::<F>(x);
            x = half.mul(x.add(q));
        }
        x
    }

    /// Divide by a small positive `u32` divisor. Used for Taylor
    /// coefficient sequences `term · r² / ((2n)(2n+1))` where the
    /// denominator is an integer.
    #[must_use]
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
// The ExtNum working-precision seam (M3, fd-4zo.10 lane; ADR-0059).

/// The contract between the transcendental kernel bodies and a
/// working-precision number type.
///
/// [`Extended`] (rung 1, 50 digits) implements it today by delegating
/// every operation verbatim to its inherent surface; the escalation
/// rungs (`Extended2` at 110 digits, the feature-gated unbounded rung)
/// implement the same contract at their own widths. Kernel bodies
/// written against this trait are precision polymorphic: the ladder
/// re-runs the *same* body at a wider rung instead of maintaining a
/// per-width copy. This follows the house precedent of the
/// [`DecimalFormat`] genericization, whose `Decimal128` instantiation
/// was proven byte identical to the pre-seam kernel; the M4 gate
/// demands the same property for the `Extended` instantiation of every
/// generic body.
///
/// ## Surface
///
/// The members mirror the operations the eight kernel modules actually
/// perform, nothing more:
///
/// * arithmetic and comparison at working precision (round-half-even
///   after every binary operation, exactly as the inherent surface);
/// * the format boundary crossings (`from_format`, `to_format`, the
///   ADR-0051 anchor seam `sticks_to` / `to_format_with_residual`, and
///   the M2 escalation predicate `near_rounding_boundary`);
/// * component-level constructors and accessors replacing the direct
///   field pokes the concrete kernels used (`from_parts_u128`,
///   `with_sign`, `with_exponent`, `sign`, `exponent`, `digit_count`);
/// * the named transcendental constants, provided per implementation
///   because a wider rung needs *wider* literals, not a widening of
///   the 55-digit ones (M5 delivers the 110-digit set);
/// * the series iteration caps, one method per loop so each rung sizes
///   its Taylor loops to its own precision (the loops terminate early
///   on `next_sum == sum`, so a wider cap is behavior neutral at rung 1
///   — but the cap must grow with the digit count for the wider rungs
///   to converge).
///
/// Implementations are `Copy` value types; the kernels pass them by
/// value exactly as they pass `Extended` today.
///
/// ## The exemplar receiver (M8b)
///
/// The precision query, the series caps, the constants, and the
/// constructors are all *instance* methods whose receiver supplies
/// nothing but the working-precision context: `ex.one()`,
/// `ex.pi_over_two()`, `ex.from_format(x)`. Rust cannot monomorphize
/// unboundedly many precisions from const generics, so a heap-backed
/// rung must carry its precision as a runtime value — and a static
/// surface (`E::ONE`, `E::pi()`) has no slot to carry one into a kernel
/// body. A scoped precision global was rejected: ambient state in a
/// pure kernel, and thumbv6m has no CAS to lock it with. The fixed
/// rungs ignore the receiver entirely and constant fold, so the seam is
/// behavior neutral for rungs 1 and 2 (the M4 byte-identity gate,
/// re-run, is the evidence).
///
/// A generic body obtains its exemplar either from an `E`-typed
/// argument it already carries (a working value is at the rung's
/// precision by construction) or from a leading `ex: E` parameter the
/// public wrapper fills with the concrete rung's zero.
// `wrong_self_convention`: the `from_*` constructors keep the names of
// the inherent constructors they delegate to, and their receiver is the
// precision context rather than the value being converted — the very
// shape the exemplar seam exists to express.
#[allow(clippy::wrong_self_convention)]
pub(crate) trait ExtNum: Copy + core::fmt::Debug {
    // ---- working-precision metadata ------------------------------------

    /// Working precision in decimal digits. An instance method rather
    /// than an associated constant so a heap-backed top rung can report
    /// the dynamic precision its receiver carries, while the fixed
    /// rungs ignore the receiver and constant fold.
    fn precision(&self) -> u32;

    // ---- series iteration caps -----------------------------------------

    /// Cap for `exp`'s Taylor loop (`Σ rⁿ/n!`, `|r| ≤ ln(10)/2`).
    fn exp_series_terms(&self) -> u32;
    /// Cap for the `sin`/`cos` Taylor loops (`|r| ≤ π/4`).
    fn sin_cos_series_terms(&self) -> u32;
    /// Cap for the `sinh`/`cosh` small-argument Taylor loops
    /// (`|x| < 0.5`).
    fn sinh_cosh_series_terms(&self) -> u32;
    /// Cap for the `ln(1 + u)` Taylor loop (`|u| ≤ ~0.6`).
    fn log1p_series_terms(&self) -> u32;
    /// Cap for `atan`'s Taylor loop (`|t| ≤ tan(π/8)`).
    fn atan_series_terms(&self) -> u32;

    // ---- constants -----------------------------------------------------

    /// Canonical zero.
    fn zero(&self) -> Self;
    /// `1`.
    fn one(&self) -> Self;
    /// `0.5`.
    fn half(&self) -> Self;

    /// `π` at this rung's working precision.
    fn pi(&self) -> Self;
    /// Euler's number `e`.
    fn e(&self) -> Self;
    /// `ln(2)`.
    fn ln2(&self) -> Self;
    /// `ln(10)`.
    fn ln10(&self) -> Self;
    /// `1/ln(10)`.
    fn inv_ln10(&self) -> Self;
    /// `1/ln(2)`.
    fn inv_ln2(&self) -> Self;
    /// `π/2`.
    fn pi_over_two(&self) -> Self;
    /// `π/4`.
    fn pi_over_four(&self) -> Self;
    /// `tan(π/8)` — atan's inner reduction threshold.
    fn tan_pi_over_eight(&self) -> Self;

    // ---- constructors --------------------------------------------------
    //
    // Exemplar-relative like the constants above: the receiver names the
    // width to build at, never a value the result depends on.

    fn from_i32(&self, n: i32) -> Self;
    /// Parse a hand-curated decimal literal (panics on invalid input;
    /// see [`Extended::parse_str`] for the accepted grammar). Literals
    /// wider than the rung's working precision are narrowed by the
    /// implementation's own invariant machinery.
    fn parse_str(&self, s: &str) -> Self;
    /// Exact small-component constructor: `(-1)^sign · coef · 10^exp`
    /// with a `u128` coefficient. Replaces the concrete kernels'
    /// `Extended { coef, exp, sign }` literals (thresholds and similar
    /// values whose coefficients fit `u128`).
    fn from_parts_u128(&self, coef: u128, exp: i32, sign: bool) -> Self;
    /// Widening constructor from `U256` components plus a pre-dropped
    /// sticky residue, rounding into the rung's working precision
    /// (argred's residual delivery seam).
    fn from_components_with_sticky(
        &self,
        coef: U256,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self;
    /// Decode a finite or zero format datum (panics on NaN / Inf, which
    /// the kernels dispatch at the public boundary).
    fn from_format<F: DecimalFormat>(&self, d: F) -> Self;
    /// Lossless widening from the rung-1 carrier. The [`DecimalFormat`]
    /// seam delivers its `Extended`-typed values (the `exp` magnitude
    /// gate limits) into whichever rung is running through this;
    /// `Extended`'s own impl is the identity.
    fn from_extended(&self, x: Extended) -> Self;
    /// Overflow saturation proxy (see [`Extended::saturate_overflow`]).
    fn saturate_overflow(&self, sign: bool) -> Self;
    /// Underflow saturation proxy (see [`Extended::saturate_underflow`]).
    fn saturate_underflow(&self) -> Self;

    // ---- accessors and component edits ---------------------------------

    /// `true` for a negative value (zero is canonically positive).
    fn sign(self) -> bool;
    /// The unbiased quantum exponent of the coefficient.
    fn exponent(self) -> i32;
    /// Decimal digit count of the coefficient.
    fn digit_count(self) -> u32;
    fn is_zero(self) -> bool;
    /// Same coefficient and exponent, sign replaced. Caller keeps the
    /// concrete kernels' contract of never setting a sign on zero.
    #[must_use]
    fn with_sign(self, sign: bool) -> Self;
    /// Same coefficient and sign, exponent replaced (ln's decade
    /// decomposition seam).
    #[must_use]
    fn with_exponent(self, exp: i32) -> Self;

    // ---- arithmetic ----------------------------------------------------

    #[must_use]
    fn neg(self) -> Self;
    #[must_use]
    fn abs(self) -> Self;
    #[must_use]
    fn add(self, other: Self) -> Self;
    #[must_use]
    fn sub(self, other: Self) -> Self;
    #[must_use]
    fn mul(self, other: Self) -> Self;
    #[must_use]
    fn square(self) -> Self;
    /// `self / other`, Newton seeded at the format's precision.
    #[must_use]
    fn div<F: DecimalFormat>(self, other: Self) -> Self;
    /// `1 / self`, Newton seeded at the format's precision.
    #[must_use]
    fn recip<F: DecimalFormat>(self) -> Self;
    /// `√self`, Newton seeded at the format's precision.
    #[must_use]
    fn sqrt<F: DecimalFormat>(self) -> Self;
    /// Divide by a small positive integer (Taylor denominators).
    #[must_use]
    fn div_u32(self, divisor: u32) -> Self;
    /// Multiply by `10^k` — a pure exponent shift.
    #[must_use]
    fn mul_pow10_exp(self, k: i32) -> Self;

    // ---- comparison ----------------------------------------------------

    /// Signed total ordering, `+0 == -0`.
    fn cmp(self, other: Self) -> Ordering;

    // ---- conversions ---------------------------------------------------

    /// Truncate toward zero into an `i32`. Caller guarantees the
    /// magnitude is well within `i32::MAX` (the reduction integers
    /// `k` of `exp`'s decade split are ≤ ~6200).
    fn trunc_to_i32(self) -> i32;

    // ---- format boundary -----------------------------------------------

    /// Round into the format (see [`Extended::to_format`]).
    fn to_format<F: DecimalFormat>(self, q_preferred: i32, rm: RoundingMode) -> (F, Status);
    /// The ADR-0051 anchor residual delivery (see
    /// [`Extended::to_format_with_residual`]).
    fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status);
    /// The ADR-0051 grid-stuck snap test (see [`Extended::sticks_to`]).
    #[must_use]
    fn sticks_to(self, anchor: Self) -> bool;
    /// The M2 escalation predicate (see
    /// [`Extended::near_rounding_boundary`]).
    #[must_use]
    fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool;

    // ---- ladder position (ADR-0059 M8) ---------------------------------

    /// `true` when a near-boundary verdict at this rung escalates to
    /// the next one; `false` only for the top fixed rung, whose
    /// delivery is unconditional (the Tier 2 model; `ladder_audit`
    /// builds panic there instead). Under the `unbounded-ladder`
    /// feature there is no top *fixed* rung: rung 2 escalates too, and
    /// the dynamic rung above it widens instead of ever delivering an
    /// ambiguous value.
    const ESCALATES: bool;
    /// Ladder position: 1 (`Extended`), 2 (`Extended2`), 3 (the
    /// dynamic rung). Read only by the test-lane cfg skips in
    /// `round_guarded` — `force_escalate` forces rung 1 alone,
    /// `force_rung3` forces rungs 1 and 2 — so each lane keeps its
    /// meaning regardless of which rungs escalate in a given build.
    const RUNG: u8;
    /// This rung's side of a per-function [`crate::ladder::Budget`],
    /// in this rung's own predicate units. Takes the receiver so the
    /// dynamic rung can evaluate its precision-dependent formula; the
    /// fixed rungs ignore it and constant fold.
    fn rung_budget(&self, budget: &crate::ladder::Budget) -> u128;
    /// This rung's Payne–Hanek reduction: `(k mod 4, |x| reduced into
    /// `[0, π/4]`, status)`. Rung 1 reads the 76-fractional-digit
    /// window and the 38-digit `π/2` (empirically discharged
    /// truncation, fd-aqs.10); rung 2 reads `reduce_wide`'s
    /// 143-digit window and the 115-digit `π/2` (analytic
    /// `< 10^-114` truncation, M6). Dispatching per rung is the
    /// whole point of trig escalation: re-running the narrow
    /// reduction at wide arithmetic would inherit the very
    /// truncation the escalation is trying to outrun.
    #[cfg(feature = "trig")]
    fn reduce_trig<F: DecimalFormat>(&self, x: F) -> (u32, Self, Status);
}

// The receiver of every exemplar-relative member below is unused: rung
// 1's width is fixed at `EXT_PRECISION`, so each one delegates verbatim
// to the inherent surface and constant folds away.
impl ExtNum for Extended {
    fn precision(&self) -> u32 {
        EXT_PRECISION
    }

    // The caps reproduce the concrete kernels' loop bounds exactly;
    // `series_caps_pin_the_concrete_loop_bounds` (tests below) is the
    // drift guard the M4 genericization relies on.
    fn exp_series_terms(&self) -> u32 {
        60
    }
    fn sin_cos_series_terms(&self) -> u32 {
        120
    }
    fn sinh_cosh_series_terms(&self) -> u32 {
        120
    }
    fn log1p_series_terms(&self) -> u32 {
        250
    }
    fn atan_series_terms(&self) -> u32 {
        200
    }

    fn zero(&self) -> Self {
        Extended::ZERO
    }
    fn one(&self) -> Self {
        Extended::ONE
    }
    fn half(&self) -> Self {
        Extended::HALF
    }

    fn pi(&self) -> Self {
        crate::consts::pi_ext()
    }
    fn e(&self) -> Self {
        crate::consts::e_ext()
    }
    fn ln2(&self) -> Self {
        crate::consts::ln2_ext()
    }
    fn ln10(&self) -> Self {
        crate::consts::ln10_ext()
    }
    fn inv_ln10(&self) -> Self {
        crate::consts::inv_ln10_ext()
    }
    fn inv_ln2(&self) -> Self {
        crate::consts::inv_ln2_ext()
    }
    fn pi_over_two(&self) -> Self {
        crate::consts::pi_over_two_ext()
    }
    fn pi_over_four(&self) -> Self {
        crate::consts::pi_over_four_ext()
    }
    fn tan_pi_over_eight(&self) -> Self {
        crate::consts::tan_pi_over_eight_ext()
    }

    fn from_i32(&self, n: i32) -> Self {
        Extended::from_i32(n)
    }
    fn parse_str(&self, s: &str) -> Self {
        Extended::parse_str(s)
    }
    fn from_parts_u128(&self, coef: u128, exp: i32, sign: bool) -> Self {
        Self {
            coef: U256::from_u128(coef),
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
        Extended::from_components_with_sticky(coef, exp, sign, pre_sticky)
    }
    fn from_format<F: DecimalFormat>(&self, d: F) -> Self {
        Extended::from_format(d)
    }
    fn from_extended(&self, x: Extended) -> Self {
        x
    }
    fn saturate_overflow(&self, sign: bool) -> Self {
        Extended::saturate_overflow(sign)
    }
    fn saturate_underflow(&self) -> Self {
        Extended::saturate_underflow()
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
        Extended::is_zero(self)
    }
    fn with_sign(self, sign: bool) -> Self {
        Self { sign, ..self }
    }
    fn with_exponent(self, exp: i32) -> Self {
        Self { exp, ..self }
    }

    fn neg(self) -> Self {
        Extended::neg(self)
    }
    fn abs(self) -> Self {
        Extended::abs(self)
    }
    fn add(self, other: Self) -> Self {
        Extended::add(self, other)
    }
    fn sub(self, other: Self) -> Self {
        Extended::sub(self, other)
    }
    fn mul(self, other: Self) -> Self {
        Extended::mul(self, other)
    }
    fn square(self) -> Self {
        Extended::square(self)
    }
    fn div<F: DecimalFormat>(self, other: Self) -> Self {
        Extended::div::<F>(self, other)
    }
    fn recip<F: DecimalFormat>(self) -> Self {
        Extended::recip::<F>(self)
    }
    fn sqrt<F: DecimalFormat>(self) -> Self {
        Extended::sqrt::<F>(self)
    }
    fn div_u32(self, divisor: u32) -> Self {
        Extended::div_u32(self, divisor)
    }
    fn mul_pow10_exp(self, k: i32) -> Self {
        Extended::mul_pow10_exp(self, k)
    }

    fn cmp(self, other: Self) -> Ordering {
        Extended::cmp(self, other)
    }

    // Mirrors `exp.rs`'s `truncate_to_i32` (which M4 retires in favor
    // of this seam): shift the coefficient by the exponent and read
    // the low limb.
    fn trunc_to_i32(self) -> i32 {
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

    fn to_format<F: DecimalFormat>(self, q_preferred: i32, rm: RoundingMode) -> (F, Status) {
        Extended::to_format::<F>(self, q_preferred, rm)
    }
    fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        Extended::to_format_with_residual::<F>(self, magnitude_grows, rm)
    }
    fn sticks_to(self, anchor: Self) -> bool {
        Extended::sticks_to(self, anchor)
    }
    fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        Extended::near_rounding_boundary::<F>(self, budget)
    }

    const ESCALATES: bool = true;
    const RUNG: u8 = 1;
    fn rung_budget(&self, budget: &crate::ladder::Budget) -> u128 {
        budget.rung1
    }
    #[cfg(feature = "trig")]
    fn reduce_trig<F: DecimalFormat>(&self, x: F) -> (u32, Self, Status) {
        crate::argred::reduce_body::<F, Extended>(*self, x)
    }
}

// ----------------------------------------------------------------------------
// Multi-word helpers.

/// `a × b` for two `U256`s whose combined decimal-digit count is ≤ 115
/// (the `U384` capacity). Inputs must each be ≤ 50 digits, which is
/// the invariant Extended maintains after every round.
#[inline]
pub(crate) fn u256_mul_u256(a: U256, b: U256) -> U384 {
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

    fn ext(s: &str) -> Extended {
        Extended::parse_str(s)
    }

    #[test]
    fn add_basic() {
        let a = ext("1.5");
        let b = ext("2.25");
        let c = a.add(b);
        assert_eq!(c.cmp(ext("3.75")), core::cmp::Ordering::Equal);
    }

    #[test]
    fn sub_basic() {
        let a = ext("3.75");
        let b = ext("1.25");
        let c = a.sub(b);
        assert_eq!(c.cmp(ext("2.5")), core::cmp::Ordering::Equal);
    }

    #[test]
    fn mul_basic() {
        let a = ext("3.5");
        let b = ext("4.0");
        let c = a.mul(b);
        assert_eq!(c.cmp(ext("14.0")), core::cmp::Ordering::Equal);
    }

    #[test]
    fn mul_high_precision_carries() {
        // (10^25)² should give 10^50, exactly at EXT_PRECISION boundary.
        let a = ext("1e25");
        let b = ext("1e25");
        let c = a.mul(b);
        assert_eq!(c.cmp(ext("1e50")), core::cmp::Ordering::Equal);
    }

    #[test]
    fn div_u32_basic() {
        let a = ext("10");
        let c = a.div_u32(3);
        // 10/3 = 3.333…3 to 50 digits.
        assert_eq!(
            c.cmp(ext("3.3333333333333333333333333333333333333333333333333")),
            core::cmp::Ordering::Equal
        );
    }

    #[test]
    fn div_u32_terminates_clean() {
        let a = ext("100");
        let c = a.div_u32(4);
        assert_eq!(c.cmp(ext("25")), core::cmp::Ordering::Equal);
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
        assert_eq!(
            restored.cmp(ext("1e-40")),
            core::cmp::Ordering::Equal,
            "expected 1e-40"
        );
    }

    // -----------------------------------------------------------------
    // Oracle cross-check: at extended precision the basic ops should
    // match astro-float to within 1 ULP_50 (i.e. 10^{-50} relative).

    /// Render an `Extended` directly as a full-precision decimal string
    /// of the form `[-]<digits>e<exp>` — no format round-trip, so
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
        assert_eq!(d.cmp(ext("1e-36")), core::cmp::Ordering::Equal);
    }

    // -----------------------------------------------------------------
    // near_rounding_boundary (M2, fd-4zo.10).

    mod boundary_predicate {
        extern crate std;

        use super::*;
        use crate::format::DecimalFormat;
        use alloc::vec;
        use alloc::vec::Vec;
        use ferrodec_ieee::should_round_up;
        use proptest::prelude::*;

        use crate::mock_format::MockFmt;

        /// The three real format shapes (precision, bias).
        type D128Shape = MockFmt<34, 6176>;
        type D64Shape = MockFmt<16, 398>;
        type D32Shape = MockFmt<7, 101>;

        const MODES: [RoundingMode; 5] = [
            RoundingMode::NearestEven,
            RoundingMode::NearestAway,
            RoundingMode::TowardZero,
            RoundingMode::TowardPositive,
            RoundingMode::TowardNegative,
        ];

        /// Build an `Extended` whose widened coefficient is
        /// `prefix · 10^excess + base + offset`; a negative `offset`
        /// borrows from `prefix`, an overflowing one carries into it.
        fn with_tail(
            prefix: u128,
            excess: u32,
            base: U256,
            offset: i128,
            exp: i32,
            sign: bool,
        ) -> Extended {
            let stem = U256::from_u128(prefix).mul_pow10(excess).add(base);
            let coef = if offset >= 0 {
                stem.add(U256::from_u128(offset as u128))
            } else {
                stem.sub(U256::from_u128(offset.unsigned_abs()))
            };
            Extended { coef, exp, sign }
        }

        /// d128 shape, normal range, excess 16: sweep every tail within
        /// ±300 of each boundary (lower grid point, midpoint, upper grid
        /// point) and pin the predicate against an independently computed
        /// `u128` distance for budgets spanning the bands.
        #[test]
        fn exhaustive_tail_bands_d128_shape() {
            const FIELD: u128 = 10u128.pow(16);
            const HALF: u128 = 5 * 10u128.pow(15);
            // 34-digit prefixes with an even and an odd last kept digit:
            // the distance semantics must not see parity (parity only
            // picks the half-even tie direction, which is the rounder's
            // business, not the predicate's).
            let prefixes: [u128; 2] = [
                1_234_567_890_123_456_789_012_345_678_901_234,
                9_876_543_210_987_654_321_098_765_432_109_877,
            ];
            let budgets: [u128; 6] = [0, 1, 2, 299, 300, 301];
            for prefix in prefixes {
                for center in [0u128, HALF, FIELD] {
                    for off in -300i128..=300 {
                        let v = with_tail(prefix, 16, U256::from_u128(center), off, -20, false);
                        // Independent distance in u128 space: the actual
                        // tail after borrow / carry, then the minimum over
                        // both families.
                        let t = (center as i128 + off).rem_euclid(FIELD as i128) as u128;
                        let dist = t.min(FIELD - t).min(t.abs_diff(HALF));
                        for b in budgets {
                            assert_eq!(
                                v.near_rounding_boundary::<D128Shape>(b),
                                dist <= b,
                                "prefix={prefix} center={center} off={off} b={b}"
                            );
                        }
                    }
                }
            }
        }

        /// d64 / d32 shapes: the drop fields (10^34, 10^43) exceed
        /// `u128`, so pin the boundary bands where the distance equals
        /// the offset magnitude by construction.
        #[test]
        fn tail_bands_d64_d32_shapes() {
            fn run<F: DecimalFormat>(excess: u32, prefix: u128) {
                let half = U256::from_u128(5).mul_pow10(excess - 1);
                let field = U256::from_u128(1).mul_pow10(excess);
                for base in [U256::ZERO, half, field] {
                    for off in -200i128..=200 {
                        let v = with_tail(prefix, excess, base, off, 0, false);
                        for b in [0u128, 1, 199, 200, 201] {
                            assert_eq!(
                                v.near_rounding_boundary::<F>(b),
                                off.unsigned_abs() <= b,
                                "excess={excess} off={off} b={b}"
                            );
                        }
                    }
                }
            }
            run::<D64Shape>(34, 1_234_567_890_123_456);
            run::<D32Shape>(43, 1_234_567);
        }

        /// The fd-42l subnormal drop: when `qmin − exp` exceeds the
        /// precision excess, the boundary field widens with the drop.
        #[test]
        fn subnormal_drop_positions_d128_shape() {
            // General path: E ∈ {17, 25, 49}, prefix keeps 50 total digits.
            for (e, prefix) in [
                (17u32, 123_456_789_012_345_678_901_234_567_890_123u128), // 33 digits
                (25, 1_234_567_890_123_456_789_012_345),                  // 25 digits
                (49, 7),                                                  // 1 digit
            ] {
                let exp = -6176 - e as i32;
                let half = U256::from_u128(5).mul_pow10(e - 1);
                for base in [U256::ZERO, half] {
                    for off in [-3i128, -1, 0, 1, 3] {
                        let v = with_tail(prefix, e, base, off, exp, false);
                        assert_eq!(
                            v.near_rounding_boundary::<D128Shape>(3),
                            off.unsigned_abs() <= 3,
                            "E={e} off={off}"
                        );
                        if off != 0 {
                            assert!(
                                !v.near_rounding_boundary::<D128Shape>(off.unsigned_abs() - 1),
                                "E={e} off={off} budget below distance"
                            );
                        }
                    }
                }
            }

            // E = 50: the round digit sits at the MSD; the midpoint of
            // the zero-to-MIN_SUBNORMAL step is 5·10^49.
            let mid50 = Extended {
                coef: U256::from_u128(5).mul_pow10(49),
                exp: -6226,
                sign: false,
            };
            assert!(mid50.near_rounding_boundary::<D128Shape>(0));
            let nines50 = Extended {
                coef: U256::from_u128(1).mul_pow10(50).sub(U256::from_u128(1)),
                exp: -6226,
                sign: false,
            };
            // 10^50 − 1: one unit below the MIN_SUBNORMAL grid point.
            assert!(nines50.near_rounding_boundary::<D128Shape>(1));
            assert!(!nines50.near_rounding_boundary::<D128Shape>(0));
            let interior50 = Extended {
                coef: U256::from_u128(1).mul_pow10(49),
                exp: -6226,
                sign: false,
            };
            // 10^49 sits 4·10^49 units from the nearest boundary — out
            // of reach of ANY u128 budget.
            assert!(!interior50.near_rounding_boundary::<D128Shape>(u128::MAX));

            // E ≥ 51: full drop, strictly. False by the budget type.
            let deep = Extended {
                coef: U256::from_u128(1).mul_pow10(50).sub(U256::from_u128(1)),
                exp: -6227,
                sign: false,
            };
            assert!(!deep.near_rounding_boundary::<D128Shape>(u128::MAX));
        }

        /// The subnormal-edge tininess hazard (the flag side of fd-42l):
        /// straddling `10^E_MIN` flips the pre-rounding tininess
        /// decision, and both straddle shapes sit one unit from a grid
        /// point, so the grid family escalates them.
        #[test]
        fn tininess_edge_escalates_d128_shape() {
            // 0.999…9 × 10^E_MIN (E_MIN = −6143): 50 nines, one unit
            // below the decade grid point; drop is the subnormal 17.
            let below = Extended {
                coef: U256::from_u128(1).mul_pow10(50).sub(U256::from_u128(1)),
                exp: -6193,
                sign: false,
            };
            assert!(below.near_rounding_boundary::<D128Shape>(1));
            // 1.000…01 × 10^E_MIN: one unit above it; drop is the
            // precision 16.
            let above = Extended {
                coef: U256::from_u128(1).mul_pow10(49).add(U256::from_u128(1)),
                exp: -6192,
                sign: false,
            };
            assert!(above.near_rounding_boundary::<D128Shape>(1));
        }

        /// Zero and exactly representable values sit on the grid and
        /// escalate at any budget; only upstream classification can
        /// certify them.
        #[test]
        fn zero_and_exact_values_escalate() {
            assert!(Extended::ZERO.near_rounding_boundary::<D128Shape>(0));
            assert!(ext("1.5").near_rounding_boundary::<D128Shape>(0));
            assert!(ext("-2.25e100").near_rounding_boundary::<D128Shape>(0));
            assert!(ext("1e10").near_rounding_boundary::<D64Shape>(1));
            // 37 significant digits: the widened tail is 5.67e15, whose
            // nearest boundary (the midpoint) is 6.7e14 units away.
            let long = ext("1.234567890123456789012345678901234567");
            assert!(!long.near_rounding_boundary::<D128Shape>(1_000_000_000));
            assert!(long.near_rounding_boundary::<D128Shape>(670_000_000_000_000));
        }

        /// The predicate is a function of the widened coefficient and
        /// the drop position alone: sign never matters, and the
        /// exponent matters only through the subnormal excess.
        #[test]
        fn sign_and_exponent_invariance() {
            let prefix: u128 = 3_141_592_653_589_793_238_462_643_383_279_502;
            for off in [0i128, 4, 7, 12] {
                let mut verdicts: Vec<bool> = Vec::new();
                for sign in [false, true] {
                    for exp in [-100i32, 0, 3000] {
                        let v = with_tail(prefix, 16, U256::ZERO, off, exp, sign);
                        verdicts.push(v.near_rounding_boundary::<D128Shape>(6));
                    }
                }
                assert!(
                    verdicts.iter().all(|&x| x == (off.unsigned_abs() <= 6)),
                    "off={off}: {verdicts:?}"
                );
            }
        }

        // -------------------------------------------------------------
        // Property test vs a widened reference rounder.

        /// Reference outcome of rounding `(coef · 10^exp, sign)` into a
        /// format shape: the packed pair plus the flag-relevant facts.
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        struct RefOutcome {
            coef: U256,
            exp: i32,
            inexact: bool,
            underflow: bool,
        }

        /// Independent widened reference rounder. Digit-walk drop with
        /// the production `should_round_up` decision (itself proven
        /// exhaustively against IEEE 754-2019 §4.3.3), tininess decided
        /// on the pre-rounding value. Deliberately structured as an
        /// *outcome* computation so the predicate's distance arithmetic
        /// is checked against what rounding actually does, not against
        /// a re-derivation of the same distances.
        fn ref_round<F: DecimalFormat>(
            coef: U256,
            exp: i32,
            sign: bool,
            rm: RoundingMode,
        ) -> RefOutcome {
            let mut digits = 0u32;
            let mut n = coef;
            while !n.is_zero() {
                n = n.div_rem10().0;
                digits += 1;
            }
            assert!(digits > 0, "reference rounder needs a nonzero coefficient");
            let qmin = -F::BIAS;
            let e_min = qmin + F::PRECISION as i32 - 1;
            let p_excess = digits.saturating_sub(F::PRECISION);
            let s_excess = u32::try_from((qmin - exp).max(0)).unwrap_or(u32::MAX);
            let drop = p_excess.max(s_excess);
            assert!(drop <= 200, "test domain keeps the drop loop bounded");

            let mut kept = coef;
            let mut sticky = false;
            let mut round_digit = 0u32;
            let mut i = 0u32;
            while i < drop {
                let (q, r) = kept.div_rem10();
                if i + 1 < drop {
                    if r != 0 {
                        sticky = true;
                    }
                } else {
                    round_digit = r;
                }
                kept = q;
                i += 1;
            }

            let up = should_round_up(rm, sign, kept.div_rem10().1, round_digit, sticky);
            let mut rc = kept;
            let mut re = exp + drop as i32;
            if up {
                rc = rc.add(U256::from_u128(1));
                let mut rd = 0u32;
                let mut m = rc;
                while !m.is_zero() {
                    m = m.div_rem10().0;
                    rd += 1;
                }
                if rd > F::PRECISION {
                    rc = rc.div_rem10().0;
                    re += 1;
                }
            }
            let inexact = round_digit != 0 || sticky;
            let tiny_pre = digits as i32 + exp - 1 < e_min;
            RefOutcome {
                coef: rc,
                exp: re,
                inexact,
                underflow: tiny_pre && inexact,
            }
        }

        proptest! {
            /// Soundness: predicate `false` means every value within the
            /// closed ±budget bracket reaches the same reference outcome
            /// in all five modes. Completeness (with one unit of slack
            /// for the closed-bracket knife edge): predicate `true` at
            /// `budget − 1` means a differing witness exists within
            /// ±budget.
            #[test]
            fn predicate_sound_and_complete_vs_reference(
                prefix in 10u128.pow(33)..10u128.pow(34),
                tail in 0..10u128.pow(16),
                exp in -6250i32..=100,
                sign in proptest::bool::ANY,
                budget in 1u128..=1_000_000_000_000u128,
            ) {
                let coef = U256::from_u128(prefix)
                    .mul_pow10(16)
                    .add(U256::from_u128(tail));
                let v = Extended { coef, exp, sign };
                let near = v.near_rounding_boundary::<D128Shape>(budget);

                // Candidate offsets within the closed bracket, always
                // including the endpoints and the unit sidesteps.
                let mut offs: Vec<i128> = vec![0, 1, -1, budget as i128, -(budget as i128)];
                // Boundary landings (and their unit sidesteps) when the
                // boundary is inside the bracket. drop > 50 needs no
                // candidates: every boundary is beyond any u128 budget.
                let s_excess = (-6176 - exp).max(0) as u32;
                let drop = 16u32.max(s_excess);
                if drop <= 50 {
                    let field = U256::from_u128(1).mul_pow10(drop);
                    let half = U256::from_u128(5).mul_pow10(drop - 1);
                    let mut kept = coef;
                    for _ in 0..drop {
                        kept = kept.div_rem10().0;
                    }
                    let t = coef.sub(kept.mul_pow10(drop));
                    let b256 = U256::from_u128(budget);
                    let mut push_if_small = |dist: U256, negative: bool| {
                        if dist.cmp(b256) != Ordering::Greater {
                            // A distance that passes the filter fits
                            // i128 (budget ≤ 10^12), so the cast below
                            // is exact.
                            let mag = dist.lo as i128;
                            let signed_off = if negative { -mag } else { mag };
                            for cand in [signed_off, signed_off - 1, signed_off + 1] {
                                if cand.unsigned_abs() <= budget {
                                    offs.push(cand);
                                }
                            }
                        }
                    };
                    push_if_small(t, true);
                    push_if_small(field.sub(t), false);
                    if t.cmp(half) == Ordering::Less {
                        push_if_small(half.sub(t), false);
                    } else {
                        push_if_small(t.sub(half), true);
                    }
                }

                let apply = |d: i128| -> U256 {
                    if d >= 0 {
                        coef.add(U256::from_u128(d as u128))
                    } else {
                        coef.sub(U256::from_u128(d.unsigned_abs()))
                    }
                };

                let mut any_diff = false;
                for &d in &offs {
                    let w = apply(d);
                    for rm in MODES {
                        let base = ref_round::<D128Shape>(coef, exp, sign, rm);
                        let out = ref_round::<D128Shape>(w, exp, sign, rm);
                        if out != base {
                            any_diff = true;
                            prop_assert!(
                                near,
                                "unsound: predicate false but offset {d} changes {rm:?}: {base:?} -> {out:?}"
                            );
                        }
                    }
                }
                if budget > 1 && v.near_rounding_boundary::<D128Shape>(budget - 1) {
                    prop_assert!(
                        any_diff,
                        "incomplete: predicate true at budget-1 with no witness within the bracket"
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // ExtNum seam (M3, fd-4zo.11).

    mod extnum_seam {
        use super::*;
        use crate::extended::ExtNum;

        /// The M4 genericization rewrites the kernels' literal loop
        /// bounds to these caps; exact pins per cap, so a drifted
        /// value fails here before it silently changes a Taylor loop.
        /// The exemplar (M8b) is `ZERO` throughout: the fixed rung
        /// reads nothing but the width off it.
        #[test]
        fn series_caps_pin_the_concrete_loop_bounds() {
            let ex = Extended::ZERO;
            assert_eq!(ex.exp_series_terms(), 60);
            assert_eq!(ex.sin_cos_series_terms(), 120);
            assert_eq!(ex.sinh_cosh_series_terms(), 120);
            assert_eq!(ex.log1p_series_terms(), 250);
            assert_eq!(ex.atan_series_terms(), 200);
            assert_eq!(ex.precision(), EXT_PRECISION);
        }

        /// Every named constant delegates to the same literal the
        /// concrete kernels parse today.
        #[test]
        fn named_constants_delegate_to_consts() {
            let ex = Extended::ZERO;
            let pairs = [
                (ex.pi(), crate::consts::pi_ext()),
                (ex.e(), crate::consts::e_ext()),
                (ex.ln2(), crate::consts::ln2_ext()),
                (ex.ln10(), crate::consts::ln10_ext()),
                (ex.inv_ln10(), crate::consts::inv_ln10_ext()),
                (ex.inv_ln2(), crate::consts::inv_ln2_ext()),
                (ex.pi_over_two(), crate::consts::pi_over_two_ext()),
                (ex.pi_over_four(), crate::consts::pi_over_four_ext()),
                (
                    ex.tan_pi_over_eight(),
                    crate::consts::tan_pi_over_eight_ext(),
                ),
            ];
            for (got, want) in pairs {
                assert_eq!(got.coef, want.coef);
                assert_eq!(got.exp, want.exp);
                assert_eq!(got.sign, want.sign);
            }
        }

        /// `from_parts_u128` reproduces the representation of the
        /// concrete kernels' struct literals bit for bit (the
        /// `LOG1P_THRESHOLD` shapes in `hyperbolic.rs`).
        #[test]
        fn from_parts_u128_matches_struct_literals() {
            let t = Extended::ZERO.from_parts_u128(15, -2, false);
            assert_eq!(t.coef, U256::from_u128(15));
            assert_eq!(t.exp, -2);
            assert!(!t.sign);
        }

        /// `with_sign` / `with_exponent` reproduce the concrete
        /// kernels' field-edit struct literals.
        #[test]
        fn component_edits_match_field_pokes() {
            let one_neg = Extended::ZERO.one().with_sign(true);
            let literal = Extended {
                sign: true,
                ..Extended::ONE
            };
            assert_eq!(one_neg.coef, literal.coef);
            assert_eq!(one_neg.exp, literal.exp);
            assert_eq!(one_neg.sign, literal.sign);

            let x = ext("12345e-3");
            let m = x.with_exponent(-4);
            assert_eq!(m.coef, x.coef);
            assert_eq!(m.exp, -4);
            assert_eq!(m.sign, x.sign);
        }

        /// `trunc_to_i32` mirrors `exp.rs`'s `truncate_to_i32` (which
        /// M4 retires for this seam): truncation toward zero on both
        /// exponent signs, both value signs, and zero.
        #[test]
        fn trunc_to_i32_truncates_toward_zero() {
            let cases = [
                ("0", 0),
                ("1", 1),
                ("-1", -1),
                ("6144.999999999999999999999999", 6144),
                ("-6144.999999999999999999999999", -6144),
                ("0.99999999999999", 0),
                ("-0.5", 0),
                ("123.456", 123),
                ("1e3", 1000),
                ("-2.5e2", -250),
            ];
            for (s, want) in cases {
                assert_eq!(ext(s).trunc_to_i32(), want, "input {s}");
            }
        }

        /// Spot delegation identity: a trait-dispatched compound
        /// expression equals the inherent-dispatched one on the same
        /// inputs (the impl is delegation, not re-derivation; this
        /// guards against a future edit decoupling the two).
        #[test]
        fn trait_dispatch_equals_inherent_dispatch() {
            fn via_trait<E: ExtNum>(a: E, b: E) -> E {
                a.mul(b).add(a.one()).sub(b.square()).div_u32(3).neg().abs()
            }
            let a = ext("3.14159265358979323846264338327950288419716939937510");
            let b = ext("-2.71828182845904523536028747135266249775724709369996");
            let got = via_trait(a, b);
            let want = a
                .mul(b)
                .add(Extended::ONE)
                .sub(b.square())
                .div_u32(3)
                .neg()
                .abs();
            assert_eq!(got.coef, want.coef);
            assert_eq!(got.exp, want.exp);
            assert_eq!(got.sign, want.sign);
        }
    }
}
