//! The unbounded rung of the ADR-0059 escalation ladder (M8b): a
//! working type whose precision is chosen at run time.
//!
//! ## Why
//!
//! Rungs 1 and 2 ([`Extended`] at 50 digits and `Extended2` at 110)
//! are fixed-width, so the ladder they form is finite: a call whose
//! true result sits closer to a format rounding boundary than rung 2's
//! own error bracket cannot be decided at all. The Ziv driver (M8b
//! step 5) closes that hole by re-running the kernel at ever wider
//! precision until the bracket separates. This type is what it runs
//! at, and the width lives in a value rather than in the type.
//!
//! ## The `Copy` constraint and its resolution
//!
//! [`ExtNum`] requires `Copy`: the kernel bodies consume a working
//! value more than once (`let r_sq = r.square(); taylor_sin_ext(r,
//! r_sq)`). `DecBig` is `Vec`-backed and cannot be `Copy`. The working
//! value is therefore a `Copy` **handle into an arena the driver
//! owns**: [`DynArena`] holds the coefficients, [`ExtendedDyn`] holds
//! an index, an exponent and a sign. The exemplar seam (the
//! `&self` receiver on every constant and constructor of [`ExtNum`])
//! exists precisely so a receiver can carry that runtime context;
//! here it carries the arena reference and the precision.
//!
//! Arena values are immutable once pushed, so a handle stays valid for
//! the arena's lifetime and copying one is free.
//!
//! ## Borrow discipline
//!
//! The arena's `Vec` sits behind a `RefCell`. Every operation reads
//! the coefficients it needs under a short-lived shared borrow (which
//! ends at the end of the reading statement), computes on owned
//! `DecBig` values, and only then takes the mutable borrow that
//! appends the result. **No operation may call another `ExtendedDyn`
//! operation while a borrow is live**: the two borrows would overlap
//! and `RefCell` would panic. Keeping the reads inside
//! [`ExtendedDyn::coef`] (which clones and drops the borrow in one
//! statement) is what makes that rule mechanical rather than a
//! convention a reviewer has to re-derive.
//!
//! ## Mirror discipline
//!
//! `extended2.rs` is the template and this file mirrors it clause for
//! clause at the runtime width `prec`. Any behavioral divergence
//! beyond the width parameterization is a defect; the cross-substrate
//! differential in this file's test module (`ExtendedDyn` at
//! `prec = 110` against `Extended2`) is the standing guard.
//!
//! The full [`ExtNum`] surface is live: `reduce_trig` delegates to
//! `argred::reduce_dyn`, the runtime Payne-Hanek reduction (M8b step
//! 4), and `rung_budget` evaluates the per-function `budget.dynamic`
//! formula at the arena's precision (step 5). The Ziv driver in
//! `ladder::run3` is what walks this rung.

#![allow(dead_code)]

use crate::extended::{ExtNum, Extended};
use crate::format::DecimalFormat;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::cmp::Ordering;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::{bigconst, DecBig, U256};

/// Lowest working precision the dynamic rung accepts. The Ziv driver
/// enters this rung only after rung 2 (110 digits) fails to separate,
/// so a narrower arena would be a caller bug rather than a supported
/// configuration.
pub(crate) const MIN_DYN_PRECISION: u32 = 110;

/// Guard digits every runtime constant carries over the working
/// precision. Mirrors the fixed rungs, whose hand-curated literals
/// overshoot by the same five digits (55 over 50, 115 over 110).
const CONST_GUARD_DIGITS: u32 = 5;

/// Slack term of the virtual alignment capacity `2 · prec + 11`.
///
/// The fixed rungs short-circuit an addition when aligning the larger
/// operand would overflow their alignment buffer: rung 2's `U768`
/// holds 231 digits, which is exactly `2 · 110 + 11`, and rung 1's
/// `U384` holds 115 against `2 · 50 + 15`. `DecBig` grows, so no
/// buffer forces the cut here; the capacity is kept anyway (at the
/// tighter rung 2 shape, `2 · prec + 11`) so that a dynamic rung and
/// a fixed rung of the same width return the same value for the same
/// addition. Choosing rung 2's constant over rung 1's makes the
/// dynamic rung agree with the rung it actually escalates from.
const CAPACITY_SLACK: u32 = 11;

/// Digit floor the coefficient collapses to before entering the
/// format rounder, which takes a `U256` (77 digits). 70 digits keeps
/// the format's round digit — at or above the 34th significant digit
/// on every supported format — far above the collapse floor, so the
/// round digit survives verbatim and every dropped digit lands in the
/// sticky term the rounder already consumes. This is the `DecBig`
/// restatement of rung 2's `shift_right_to_u256` collapse argument.
const FORMAT_COLLAPSE_DIGITS: u32 = 70;

// ----------------------------------------------------------------------------
// The arena and its handles.

/// Backing store for the dynamic rung's coefficients.
///
/// The Ziv driver owns one of these per escalation attempt and hands
/// out [`ExtendedDyn`] handles into it; dropping the arena reclaims
/// every intermediate at once. Values are append-only and immutable,
/// which is what lets a handle be `Copy`.
pub(crate) struct DynArena {
    /// Coefficients, indexed by an [`ExtendedDyn::idx`]. Slot 0 is
    /// always zero, so the canonical zero needs no allocation.
    vals: RefCell<Vec<DecBig>>,
    /// Working precision in decimal digits.
    prec: u32,
}

impl DynArena {
    /// Build an arena at `prec` working decimal digits.
    ///
    /// # Panics
    ///
    /// Panics when `prec` is below [`MIN_DYN_PRECISION`] (the dynamic
    /// rung never runs below rung 2's width) or so wide that the
    /// constant generators would reject it. The upper check counts the
    /// deepest generator call this type makes — `π/4` reads
    /// `pi_digits(prec + 7)` — so a width that would panic deep inside
    /// a kernel is rejected at construction instead.
    pub(crate) fn new(prec: u32) -> Self {
        assert!(
            prec >= MIN_DYN_PRECISION,
            "DynArena: {prec} working digits is below the dynamic rung's floor of {MIN_DYN_PRECISION}"
        );
        assert!(
            u64::from(prec) + u64::from(CONST_GUARD_DIGITS) + 2 <= bigconst::MAX_DIGITS,
            "DynArena: {prec} working digits exceeds the constant generators' depth cap"
        );
        Self {
            vals: RefCell::new(alloc::vec![DecBig::zero()]),
            prec,
        }
    }

    /// The width exemplar: a zero handle whose only job is to name the
    /// arena and the precision for [`ExtNum`]'s constant and
    /// constructor surface.
    pub(crate) fn exemplar(&self) -> ExtendedDyn<'_> {
        ExtendedDyn {
            arena: self,
            idx: 0,
            exp: 0,
            sign: false,
        }
    }

    /// Working precision in decimal digits.
    pub(crate) fn precision(&self) -> u32 {
        self.prec
    }

    /// Clone the coefficient at `idx` out from under a short-lived
    /// shared borrow. The borrow ends with this statement, so callers
    /// can freely push afterwards.
    fn coef_at(&self, idx: u32) -> DecBig {
        self.vals.borrow()[idx as usize].clone()
    }

    /// Append a coefficient and return its index. Zero folds onto slot
    /// 0 rather than growing the arena, which also keeps the canonical
    /// zero a single value.
    fn intern(&self, coef: DecBig) -> u32 {
        if coef.is_zero() {
            return 0;
        }
        let mut vals = self.vals.borrow_mut();
        vals.push(coef);
        u32::try_from(vals.len() - 1).expect("DynArena: index space exhausted")
    }
}

/// A working value at the arena's runtime precision: the coefficient
/// lives in the arena, the exponent and sign travel in the handle.
///
/// Numeric value is `(-1)^sign · coef · 10^exp`. Zero is canonical:
/// coefficient slot 0 and a positive sign.
#[derive(Clone, Copy)]
pub(crate) struct ExtendedDyn<'a> {
    /// The arena holding this value's coefficient. Also the carrier of
    /// the working precision, which is why every exemplar-relative
    /// member of [`ExtNum`] can read its width off a receiver.
    arena: &'a DynArena,
    /// Index of the coefficient in `arena`.
    idx: u32,
    /// Unbiased quantum exponent.
    exp: i32,
    /// `true` for a negative value; zero stays canonically positive.
    sign: bool,
}

// Hand-written rather than derived: deriving would demand
// `DynArena: Debug` and then dump every interned coefficient into an
// assertion message. Printing the value's own three components is what
// a failing differential actually needs.
impl core::fmt::Debug for ExtendedDyn<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ExtendedDyn {{ coef: {}, exp: {}, sign: {} }}",
            self.coef(),
            self.exp,
            self.sign
        )
    }
}

// The `from_*` constructors take `&self` on purpose: that receiver is
// the exemplar seam, the only thing that names the arena to build in
// and the width to build at. `ExtNum` declares them that way and the
// fixed rungs implement them that way, so the convention lint is
// answering a question this type cannot take a different answer to.
#[allow(clippy::wrong_self_convention)]
impl ExtendedDyn<'_> {
    // ---- arena plumbing -------------------------------------------------

    /// This value's coefficient, cloned out of the arena.
    fn coef(self) -> DecBig {
        self.arena.coef_at(self.idx)
    }

    /// Build a sibling handle in the same arena. A zero coefficient
    /// canonicalizes the sign but keeps the caller's exponent, which
    /// is what the cancellation branch of [`Self::add`] needs.
    fn make(self, coef: DecBig, exp: i32, sign: bool) -> Self {
        if coef.is_zero() {
            return Self {
                arena: self.arena,
                idx: 0,
                exp,
                sign: false,
            };
        }
        Self {
            arena: self.arena,
            idx: self.arena.intern(coef),
            exp,
            sign,
        }
    }

    /// Debug guard for the binary operators: two handles only compose
    /// when they index the same arena, since the arena is both the
    /// coefficient store and the width.
    fn debug_assert_same_arena(self, other: Self) {
        debug_assert!(
            core::ptr::eq(self.arena, other.arena),
            "ExtendedDyn: operands from different arenas"
        );
    }

    // ---- constants ------------------------------------------------------

    /// Canonical zero at this width.
    #[must_use]
    pub(crate) fn zero(&self) -> Self {
        Self {
            arena: self.arena,
            idx: 0,
            exp: 0,
            sign: false,
        }
    }

    /// `1`.
    #[must_use]
    pub(crate) fn one(&self) -> Self {
        self.from_parts_u128(1, 0, false)
    }

    /// `0.5`.
    #[must_use]
    pub(crate) fn half(&self) -> Self {
        self.from_parts_u128(5, -1, false)
    }

    /// Depth every runtime constant is generated to: the working
    /// precision plus [`CONST_GUARD_DIGITS`].
    fn const_depth(&self) -> u64 {
        u64::from(self.arena.prec) + u64::from(CONST_GUARD_DIGITS)
    }

    /// `π`. `pi_digits(n)` returns `floor(π · 10^(n − 1))` as an
    /// `n`-digit integer, so the exponent that restores the value is
    /// `−(n − 1)`. The generator lands within one unit of its last
    /// digit, i.e. within `10^-(prec + 4)` relative.
    #[must_use]
    pub(crate) fn pi(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::pi_digits(n), 1 - exp_of(n), false)
    }

    /// Euler's number `e`. `e_digits(n)` returns
    /// `floor(e · 10^(n − 1))`, an `n`-digit integer.
    #[must_use]
    pub(crate) fn e(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::e_digits(n), 1 - exp_of(n), false)
    }

    /// `ln(2)`. `ln2_digits(n)` returns `floor(ln 2 · 10^n)` — the
    /// constant sits below 1, so its scale is `10^n`, not `10^(n − 1)`.
    #[must_use]
    pub(crate) fn ln2(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::ln2_digits(n), -exp_of(n), false)
    }

    /// `ln(10)`. Above 1, so `ln10_digits(n)` is
    /// `floor(ln 10 · 10^(n − 1))`.
    #[must_use]
    pub(crate) fn ln10(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::ln10_digits(n), 1 - exp_of(n), false)
    }

    /// `1/ln(10)`. Below 1, scale `10^n`.
    #[must_use]
    pub(crate) fn inv_ln10(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::inv_ln10_digits(n), -exp_of(n), false)
    }

    /// `1/ln(2)`. Above 1, scale `10^(n − 1)`.
    #[must_use]
    pub(crate) fn inv_ln2(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::inv_ln2_digits(n), 1 - exp_of(n), false)
    }

    /// `π/2`, halved out of a one-digit-deeper `π`.
    ///
    /// Scale derivation. `P = pi_digits(n + 1) = floor(π · 10^n)` is an
    /// `(n + 1)`-digit integer. `floor(floor(x) / 2) = floor(x / 2)`
    /// for real `x`, so `P / 2` (integer division) is
    /// `floor((π/2) · 10^n)` up to the error `P` already carries:
    /// `|P − π · 10^n| < 2` gives `|P/2 − (π/2) · 10^n| < 1` and the
    /// floor moves it by at most one more unit, so the result is
    /// within **1** unit of `floor((π/2) · 10^n)`. `π/2 ≈ 1.5708`
    /// keeps `(n + 1)` digits, and the value is restored by `10^-n`.
    #[must_use]
    pub(crate) fn pi_over_two(&self) -> Self {
        let n = self.const_depth();
        let halved = bigconst::pi_digits(n + 1).div_rem(&DecBig::from_u32(2)).0;
        self.make(halved, -exp_of(n), false)
    }

    /// `π/4`, quartered out of a two-digit-deeper `π`.
    ///
    /// Scale derivation, the same shape as [`Self::pi_over_two`].
    /// `Q = pi_digits(n + 2) = floor(π · 10^(n + 1))`, an
    /// `(n + 2)`-digit integer with `|Q − π · 10^(n + 1)| < 2`. Then
    /// `Q / 4 = floor((π/4) · 10^(n + 1))` up to one unit
    /// (`|Q/4 − (π/4) · 10^(n+1)| < 0.5`, and the floor adds at most
    /// one). `π/4 ≈ 0.7854` puts `(π/4) · 10^(n + 1)` at `(n + 1)`
    /// digits, so the exponent restoring the value is `−(n + 1)`.
    #[must_use]
    pub(crate) fn pi_over_four(&self) -> Self {
        let n = self.const_depth();
        let quartered = bigconst::pi_digits(n + 2).div_rem(&DecBig::from_u32(4)).0;
        self.make(quartered, -exp_of(n) - 1, false)
    }

    /// `tan(π/8)`, atan's inner reduction threshold. Below 1, scale
    /// `10^n`; the generator's `√2 − 1` route is exact.
    #[must_use]
    pub(crate) fn tan_pi_over_eight(&self) -> Self {
        let n = self.const_depth();
        self.make(bigconst::tan_pi_over_eight_digits(n), -exp_of(n), false)
    }

    /// Overflow saturation proxy; see [`Extended::saturate_overflow`]
    /// for the disposition argument (the exponent 7000 clears every
    /// format's `E_MAX` with the same documentation margin).
    #[must_use]
    pub(crate) fn saturate_overflow(&self, sign: bool) -> Self {
        self.from_parts_u128(1, 7000, sign)
    }

    /// Underflow saturation proxy; see [`Extended::saturate_underflow`].
    #[must_use]
    pub(crate) fn saturate_underflow(&self) -> Self {
        self.from_parts_u128(1, -7000, false)
    }

    // ---- constructors ---------------------------------------------------

    /// Exact small-component constructor:
    /// `(-1)^sign · coef · 10^exp` with a `u128` coefficient. No
    /// rounding — the caller's coefficient is already inside the
    /// working width (mirrors rung 2).
    #[must_use]
    pub(crate) fn from_parts_u128(&self, coef: u128, exp: i32, sign: bool) -> Self {
        self.make(DecBig::from_u128(coef), exp, sign)
    }

    /// `n` as a working value.
    #[must_use]
    pub(crate) fn from_i32(&self, n: i32) -> Self {
        if n == 0 {
            return self.zero();
        }
        self.from_parts_u128(u128::from(n.unsigned_abs()), 0, n < 0)
    }

    /// The `DecBig`-native rounding constructor, and the seam step 4's
    /// runtime argument reduction delivers its residual through:
    /// round `coef · 10^exp` into the working precision by
    /// round-half-even, folding in a residue already dropped by the
    /// caller.
    ///
    /// A zero coefficient yields the canonical zero regardless of the
    /// residue, exactly as rung 2's `from_components_with_sticky`.
    #[must_use]
    pub(crate) fn from_decbig_with_sticky(
        &self,
        coef: DecBig,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self {
        if coef.is_zero() {
            return self.zero();
        }
        let (rounded, shift) = round_decbig_to_prec(coef, self.arena.prec, pre_sticky);
        self.make(rounded, exp + shift as i32, sign)
    }

    /// Widening constructor from `U256` components plus a pre-dropped
    /// sticky residue.
    #[must_use]
    pub(crate) fn from_components_with_sticky(
        &self,
        coef: U256,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self {
        self.from_decbig_with_sticky(u256_to_decbig(coef), exp, sign, pre_sticky)
    }

    /// Build from a finite or zero format datum. Panics on NaN / Inf —
    /// callers dispatch those at the public-API boundary.
    ///
    /// A format coefficient carries at most 34 digits, comfortably
    /// inside the working width, so nothing rounds here.
    #[must_use]
    pub(crate) fn from_format<F: DecimalFormat>(&self, d: F) -> Self {
        let (coef, exp, sign) = d.to_extended_parts().expect(
            "from_format requires a finite or zero datum; NaN / Inf are \
             dispatched at the public-API boundary",
        );
        debug_assert!(
            coef.hi == 0,
            "from_format: a format coefficient fits u128 (≤ 34 digits)"
        );
        self.make(DecBig::from_u128(coef.lo), exp, sign)
    }

    /// Lossless widening from the rung-1 carrier: same digits, same
    /// exponent, same sign, on the growable substrate.
    #[must_use]
    pub(crate) fn from_extended(&self, x: Extended) -> Self {
        self.make(u256_to_decbig(x.coef), x.exp, x.sign)
    }

    /// Parse a decimal string; the grammar and the panics mirror
    /// [`Extended::parse_str`].
    ///
    /// The digits accumulate as ASCII and convert once through
    /// `DecBig::from_ascii_digits`, which is the same value the fixed
    /// rungs build with their `coef · 10 + d` loop without the
    /// quadratic limb shuffle a growable coefficient would pay for it.
    ///
    /// # Panics
    ///
    /// Panics on a character outside the grammar, on a second `'.'`,
    /// and (in debug builds) on a literal wider than the working
    /// precision plus the constant guard.
    #[must_use]
    pub(crate) fn parse_str(&self, s: &str) -> Self {
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut sign = false;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            if bytes[i] == b'-' {
                sign = true;
            }
            i += 1;
        }

        let mut digits: Vec<u8> = Vec::new();
        let mut decimal_seen = false;
        let mut digits_after_point: i32 = 0;
        while i < bytes.len() && bytes[i] != b'e' && bytes[i] != b'E' {
            match bytes[i] {
                b'0'..=b'9' => {
                    digits.push(bytes[i]);
                    if decimal_seen {
                        digits_after_point += 1;
                    }
                    i += 1;
                }
                b'.' => {
                    assert!(!decimal_seen, "ExtendedDyn::parse_str: duplicate '.'");
                    decimal_seen = true;
                    i += 1;
                }
                _ => panic!("ExtendedDyn::parse_str: invalid character"),
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
            let mut value = 0i32;
            while i < bytes.len() {
                match bytes[i] {
                    b'0'..=b'9' => {
                        value = value * 10 + i32::from(bytes[i] - b'0');
                        i += 1;
                    }
                    _ => panic!("ExtendedDyn::parse_str: invalid char in exponent"),
                }
            }
            exp_explicit = if exp_sign { -value } else { value };
        }

        let coef = DecBig::from_ascii_digits(&digits);
        if coef.is_zero() {
            return self.zero();
        }
        // Same guard shape as the fixed rungs: hand-curated literals
        // overshoot the working precision by up to the constant guard;
        // anything wider bypasses the precision invariant.
        debug_assert!(
            digit_count_of(&coef) <= self.arena.prec + CONST_GUARD_DIGITS,
            "ExtendedDyn::parse_str: literal exceeds the working precision \
             plus the constant guard; round it through the invariant \
             machinery or trim the source"
        );
        self.make(coef, exp_explicit - digits_after_point, sign)
    }

    // ---- accessors and component edits ----------------------------------

    /// `true` for the canonical zero.
    pub(crate) fn is_zero(self) -> bool {
        self.idx == 0
    }

    /// Decimal digit count of the coefficient (`1` for zero, the
    /// General Decimal Arithmetic convention the fixed rungs share).
    pub(crate) fn digit_count(self) -> u32 {
        digit_count_of(&self.coef())
    }

    /// Same coefficient and exponent, sign replaced.
    #[must_use]
    pub(crate) fn with_sign(self, sign: bool) -> Self {
        Self { sign, ..self }
    }

    /// Same coefficient and sign, exponent replaced.
    #[must_use]
    pub(crate) fn with_exponent(self, exp: i32) -> Self {
        Self { exp, ..self }
    }

    /// Negate. Zero stays positive (canonical representation).
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

    /// Absolute value.
    #[must_use]
    pub(crate) fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }

    /// Multiply by `10^k` (k may be negative). Pure exponent shift.
    #[must_use]
    pub(crate) fn mul_pow10_exp(self, k: i32) -> Self {
        if self.is_zero() {
            return self;
        }
        Self {
            exp: self.exp + k,
            ..self
        }
    }

    // ---- comparison -----------------------------------------------------

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
        let a = self.coef();
        let b = other.coef();
        let dig_a = digit_count_of(&a) as i32;
        let dig_b = digit_count_of(&b) as i32;
        let decade_a = self.exp + dig_a - 1;
        let decade_b = other.exp + dig_b - 1;
        match decade_a.cmp(&decade_b) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => {
                // Same decade — align coefficients and compare.
                let a_shift = (dig_b - dig_a).max(0) as u32;
                let b_shift = (dig_a - dig_b).max(0) as u32;
                a.mul_pow10(a_shift).cmp_ref(&b.mul_pow10(b_shift))
            }
        }
    }

    /// Signed total ordering. Treats `+0 == -0`.
    pub(crate) fn cmp(self, other: Self) -> Ordering {
        self.debug_assert_same_arena(other);
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

    // ---- arithmetic -----------------------------------------------------

    /// `self + other`, rounded to the working precision.
    #[must_use]
    pub(crate) fn add(self, other: Self) -> Self {
        self.debug_assert_same_arena(other);
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

        // Mirror of the fixed rungs' capacity cut (see
        // `CAPACITY_SLACK`): once aligning `hi_op` up by `delta` would
        // pass the virtual buffer, `lo_op` cannot touch any digit the
        // rung keeps, so the sum is `hi_op`.
        let hi_coef = hi_op.coef();
        let dig_hi = digit_count_of(&hi_coef);
        let capacity = 2 * self.arena.prec + CAPACITY_SLACK;
        if delta > capacity.saturating_sub(dig_hi) {
            return hi_op;
        }

        let hi_shifted = hi_coef.mul_pow10(delta);
        let lo_coef = lo_op.coef();

        let same_sign = hi_op.sign == lo_op.sign;
        let (result_coef, result_sign) = if same_sign {
            (hi_shifted.add(&lo_coef), hi_op.sign)
        } else {
            match hi_shifted.cmp_ref(&lo_coef) {
                Ordering::Greater | Ordering::Equal => (hi_shifted.sub(&lo_coef), hi_op.sign),
                Ordering::Less => (lo_coef.sub(&hi_shifted), lo_op.sign),
            }
        };

        if result_coef.is_zero() {
            // Total cancellation: canonical zero at the finer of the
            // two exponents (mirror).
            return self.make(DecBig::zero(), lo_op.exp, false);
        }

        let (rounded, shift) = round_decbig_to_prec(result_coef, self.arena.prec, false);
        self.make(rounded, lo_op.exp + shift as i32, result_sign)
    }

    /// `self − other`.
    #[must_use]
    pub(crate) fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }

    /// `self · other`: the exact product, rounded once.
    #[must_use]
    pub(crate) fn mul(self, other: Self) -> Self {
        self.debug_assert_same_arena(other);
        if self.is_zero() || other.is_zero() {
            return self.zero();
        }
        let prod = self.coef().mul(&other.coef());
        let (rounded, shift) = round_decbig_to_prec(prod, self.arena.prec, false);
        self.make(
            rounded,
            self.exp + other.exp + shift as i32,
            self.sign ^ other.sign,
        )
    }

    /// `self²`.
    #[must_use]
    pub(crate) fn square(self) -> Self {
        self.mul(self)
    }

    /// Divide by a small positive `u32` divisor (Taylor denominators).
    #[must_use]
    pub(crate) fn div_u32(self, divisor: u32) -> Self {
        debug_assert!(divisor != 0, "div_u32: zero divisor");
        if self.is_zero() {
            return self;
        }

        // Scale up to `prec + 2` digits before dividing, so the
        // quotient keeps `prec + 1` digits and the round step has a
        // digit of its own to inspect (mirror).
        let coef = self.coef();
        let target = self.arena.prec + 2;
        let scale_up = target.saturating_sub(digit_count_of(&coef));

        let scaled = coef.mul_pow10(scale_up);
        let (q, r) = scaled.div_rem(&DecBig::from_u32(divisor));
        let pre_sticky = !r.is_zero();

        let (rounded, shift) = round_decbig_to_prec(q, self.arena.prec, pre_sticky);
        self.make(
            rounded,
            self.exp - scale_up as i32 + shift as i32,
            self.sign,
        )
    }

    /// Reciprocal via Newton-Raphson, seeded at the format's precision.
    ///
    /// The step count is derived rather than fixed: precision doubles
    /// per step from an `F::PRECISION`-digit seed, and the iteration is
    /// run until the reached precision covers `2 · prec`, the same
    /// doubling headroom the fixed rungs carry (rung 1 takes two steps,
    /// `34 · 2² = 136 ≥ 100`; rung 2 takes three, `34 · 2³ = 272 ≥
    /// 220`). The factor of two on `prec` is what leaves the last
    /// iteration's quadratic convergence room to land the full working
    /// width rather than exactly reach it.
    #[must_use]
    pub(crate) fn recip<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.is_zero(), "ExtendedDyn::recip on zero");
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (recip_d, _) = self_d.recip_seed(RoundingMode::NearestEven);
        let mut x = self.from_format::<F>(recip_d);
        let two = self.from_i32(2);

        for _ in 0..newton_steps(F::PRECISION, self.arena.prec) {
            let bx = self.mul(x);
            let correction = two.sub(bx);
            x = x.mul(correction);
        }
        x
    }

    /// `self / other` at the working precision.
    #[must_use]
    pub(crate) fn div<F: DecimalFormat>(self, other: Self) -> Self {
        if self.is_zero() {
            return self.zero();
        }
        self.mul(other.recip::<F>())
    }

    /// Square root via Newton's method, seeded from the format's own
    /// `sqrt`; the step count is derived exactly as in [`Self::recip`].
    #[must_use]
    pub(crate) fn sqrt<F: DecimalFormat>(self) -> Self {
        debug_assert!(!self.sign, "ExtendedDyn::sqrt of negative");
        if self.is_zero() {
            return self;
        }
        let (self_d, _) = self.to_format::<F>(0, RoundingMode::NearestEven);
        let (seed_d, _) = self_d.sqrt_seed(RoundingMode::NearestEven);
        let mut x = self.from_format::<F>(seed_d);
        let half = self.half();
        for _ in 0..newton_steps(F::PRECISION, self.arena.prec) {
            let q = self.div::<F>(x);
            x = half.mul(x.add(q));
        }
        x
    }

    /// Truncate toward zero into an `i32`.
    ///
    /// # Panics
    ///
    /// Panics when the truncated magnitude does not fit an `i32`. The
    /// contract mirrors the fixed rungs': the caller guarantees a small
    /// magnitude (the reduction integers `k` of `exp`'s decade split
    /// are ≤ ~6200). Where the fixed-width rungs silently keep the low
    /// limb, the growable substrate has no low limb to keep, so the
    /// contract violation is loud instead of silent.
    pub(crate) fn trunc_to_i32(self) -> i32 {
        if self.is_zero() {
            return 0;
        }
        let coef = self.coef();
        let truncated = if self.exp >= 0 {
            coef.mul_pow10(self.exp.unsigned_abs())
        } else {
            coef.div_rem_pow10(self.exp.unsigned_abs()).0
        };
        let magnitude = truncated
            .to_u128()
            .and_then(|v| i32::try_from(v).ok())
            .expect("trunc_to_i32: caller guarantees an i32-sized magnitude");
        if self.sign {
            -magnitude
        } else {
            magnitude
        }
    }

    // ---- format boundary ------------------------------------------------

    /// Convert to a format datum.
    ///
    /// The working coefficient far exceeds the rounder's `U256`
    /// intake, so the low digits collapse into a sticky residue first
    /// (see [`FORMAT_COLLAPSE_DIGITS`] for why the collapse is exact
    /// for the rounding decision). The adjusted exponent is preserved
    /// (`digits + exp` is collapse-invariant), so the pre-rounding
    /// tininess decision is untouched.
    pub(crate) fn to_format<F: DecimalFormat>(
        self,
        q_preferred: i32,
        rm: RoundingMode,
    ) -> (F, Status) {
        let coef = self.coef();
        let shift = digit_count_of(&coef).saturating_sub(FORMAT_COLLAPSE_DIGITS);
        let (kept, dropped) = coef.div_rem_pow10(shift);
        F::round_and_pack_finite(
            decbig_to_u256(&kept),
            self.exp + shift as i32,
            q_preferred,
            self.sign,
            !dropped.is_zero(),
            rm,
            Status::OK,
        )
    }

    /// The ADR-0051 anchor residual delivery: widen to the full
    /// working precision, take the open one-ULP interval on the chosen
    /// side, then collapse to the rounder's intake with the interval
    /// encoded as a forced sticky residue. The collapse keeps the
    /// denoted side — after the optional decrement the dropped low
    /// digits fold into the sticky term, and the denoted interval stays
    /// strictly between the same format grid points as the true result.
    pub(crate) fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        debug_assert!(!self.is_zero(), "residual rounding needs a nonzero value");
        let coef = self.coef();
        let dig = digit_count_of(&coef);
        debug_assert!(
            dig <= self.arena.prec,
            "residual rounding needs a value inside the working precision"
        );
        let scale = self.arena.prec - dig;
        let coef_w = coef.mul_pow10(scale);
        let exp_w = self.exp - scale as i32;
        let coef_adj = if magnitude_grows {
            coef_w
        } else {
            coef_w.sub(&DecBig::from_u32(1))
        };
        let shift = digit_count_of(&coef_adj).saturating_sub(FORMAT_COLLAPSE_DIGITS);
        let (kept, _) = coef_adj.div_rem_pow10(shift);
        F::round_and_pack_finite(
            decbig_to_u256(&kept),
            exp_w + shift as i32,
            0,
            self.sign,
            true,
            rm,
            Status::OK,
        )
    }

    /// The ADR-0051 grid-stuck snap test at the runtime width: `true`
    /// when `self` lies within ~`10^-(prec − 3)` relative of `anchor`.
    /// The three-digit standoff is the fixed rungs' (50 − 3 and
    /// 110 − 3): composition noise is a few units in the last working
    /// digit, while a genuinely separated result sits many orders
    /// further out.
    #[must_use]
    pub(crate) fn sticks_to(self, anchor: Self) -> bool {
        let d = self.sub(anchor);
        if d.is_zero() {
            return true;
        }
        let d_adj = d.exp + d.digit_count() as i32 - 1;
        let a_adj = anchor.exp + anchor.digit_count() as i32 - 1;
        d_adj <= a_adj - (self.arena.prec as i32 - 3)
    }

    /// Mode-independent escalation predicate at the runtime width; the
    /// contract mirrors [`Extended::near_rounding_boundary`] with the
    /// budget unit now one ULP of the value widened to `prec` digits.
    #[must_use]
    pub(crate) fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        if self.is_zero() {
            return true;
        }
        let prec = self.arena.prec;
        // Normalize to the rung width first: values delivered straight
        // from a runtime constant carry up to `prec + 6` digits; not
        // normalizing would silently under-escalate (the rung 1 / rung
        // 2 mirror argument).
        let raw = self.coef();
        let (coef, exp) = if digit_count_of(&raw) > prec {
            let (c, shift) = round_decbig_to_prec(raw, prec, false);
            (c, self.exp + shift as i32)
        } else {
            (raw, self.exp)
        };
        let dig = digit_count_of(&coef);

        let scale = prec.saturating_sub(dig);
        let coef_w = coef.mul_pow10(scale);
        let exp_w = exp - scale as i32;
        let digits = dig + scale;

        let qmin = -F::BIAS;
        let precision_excess = digits.saturating_sub(F::PRECISION);
        let subnormal_excess = u32::try_from((qmin - exp_w).max(0)).unwrap_or(u32::MAX);
        let excess = precision_excess.max(subnormal_excess);

        if excess == 0 {
            return true;
        }
        if excess > digits {
            return false;
        }

        let (_, tail) = coef_w.div_rem_pow10(excess);
        let field = DecBig::pow10(excess);
        let half = DecBig::from_u32(5).mul_pow10(excess - 1);

        let bound = DecBig::from_u128(budget);
        let within = |d: &DecBig| d.cmp_ref(&bound) != Ordering::Greater;
        let dist_mid = if tail.cmp_ref(&half) == Ordering::Less {
            half.sub(&tail)
        } else {
            tail.sub(&half)
        };
        within(&tail) || within(&field.sub(&tail)) || within(&dist_mid)
    }
}

// ----------------------------------------------------------------------------
// Width helpers.

/// Decimal digit count of a coefficient, narrowed for the `u32` digit
/// arithmetic the rest of the file speaks. `DecBig` counts in `u64`;
/// the working precisions this rung supports are five decimal orders
/// below `u32::MAX`, so the narrowing cannot fail.
fn digit_count_of(coef: &DecBig) -> u32 {
    u32::try_from(coef.decimal_digit_count()).expect("digit count fits u32 under the depth cap")
}

/// The generator depth `n` as an `i32` exponent term. The depth caps
/// at `bigconst::MAX_DIGITS`, so the narrowing cannot fail.
fn exp_of(n: u64) -> i32 {
    i32::try_from(n).expect("constant depth fits i32 under the depth cap")
}

/// Round a coefficient down to at most `prec` digits by round-half-even,
/// folding in a residue the caller already dropped. Returns the rounded
/// coefficient and the exponent bump the drop implies.
///
/// This is the single rounding clause the whole file rounds through,
/// mirroring `round_u768_to_ext2` / `round_u384_to_ext2`:
///
/// * when the coefficient already fits, `pre_sticky` alone never rounds
///   up (there is no round digit to decide with — the rung 1 and rung 2
///   contract);
/// * otherwise the round digit is the top digit of the dropped field,
///   the sticky is "anything below it is nonzero, or the caller's
///   residue", and the tie goes to the even kept coefficient (whose
///   parity is the parity of its last decimal digit, since ten is
///   even);
/// * a round-up that overflows to `prec + 1` digits divides by ten and
///   reports one more digit of exponent bump.
fn round_decbig_to_prec(coef: DecBig, prec: u32, pre_sticky: bool) -> (DecBig, u32) {
    let dig = digit_count_of(&coef);
    if dig <= prec {
        let _ = pre_sticky;
        return (coef, 0);
    }
    let total_drop = dig - prec;
    let (kept, dropped) = coef.div_rem_pow10(total_drop);
    let (round_digit, rest) = dropped.div_rem_pow10(total_drop - 1);
    let round_digit = round_digit.to_u128().expect("one decimal digit fits u128");
    let sticky = pre_sticky || !rest.is_zero();

    let lsb_odd = kept.div_rem10().1 % 2 == 1;
    let round_up = round_digit > 5 || (round_digit == 5 && (sticky || lsb_odd));
    if round_up {
        let bumped = kept.add(&DecBig::from_u32(1));
        if digit_count_of(&bumped) > prec {
            return (bumped.div_rem10().0, total_drop + 1);
        }
        return (bumped, total_drop);
    }
    (kept, total_drop)
}

/// `U256` → `DecBig`, exactly. `2^128` is `u128::MAX + 1` on the
/// growable substrate, so the two limbs recombine with one multiply.
fn u256_to_decbig(v: U256) -> DecBig {
    let two_pow_128 = DecBig::from_u128(u128::MAX).add(&DecBig::from_u32(1));
    DecBig::from_u128(v.hi)
        .mul(&two_pow_128)
        .add(&DecBig::from_u128(v.lo))
}

/// `DecBig` → `U256`, exactly, for coefficients of at most 70 digits
/// (the [`FORMAT_COLLAPSE_DIGITS`] contract). Splitting at `10^35`
/// puts both halves inside a `u128` (39 digits), and reassembling in
/// `U256` cannot overflow its 77-digit envelope.
fn decbig_to_u256(v: &DecBig) -> U256 {
    debug_assert!(
        v.decimal_digit_count() <= u64::from(FORMAT_COLLAPSE_DIGITS),
        "decbig_to_u256: caller collapses to the format intake first"
    );
    let (hi, lo) = v.div_rem_pow10(35);
    let hi_u = hi.to_u128().expect("upper half fits u128 under 70 digits");
    let lo_u = lo.to_u128().expect("lower 35 digits fit u128");
    U256::from_u128(hi_u)
        .mul_pow10(35)
        .add(U256::from_u128(lo_u))
}

/// Smallest number of Newton steps that carries a `seed_digits`-wide
/// seed past `2 · prec` digits, precision doubling per step.
///
/// Derivation and the fixed-rung reproduction live on
/// [`ExtendedDyn::recip`].
fn newton_steps(seed_digits: u32, prec: u32) -> u32 {
    let target = 2 * prec;
    let mut reached = seed_digits.max(1);
    let mut steps = 0u32;
    while reached < target {
        reached = reached.saturating_mul(2);
        steps += 1;
    }
    steps
}

// ----------------------------------------------------------------------------
// The ExtNum seam: the dynamic rung speaks the same contract as the
// fixed ones. Every exemplar-relative member reads its width off the
// receiver's arena rather than off a constant.

impl ExtNum for ExtendedDyn<'_> {
    fn precision(&self) -> u32 {
        self.arena.prec
    }

    // Series caps as formulas in the working precision, carrying the
    // same safety ratios over the needed term counts that the fixed
    // rungs' constants carry (rung 1 at 50 digits / rung 2 at 110):
    //
    // * exp, |r| ≤ ln(10)/2: the term drops below 10^-(prec + 5) near
    //   n ≈ 0.55·prec + 20 (36 at 50 digits, 85 at 110), so `prec + 10`
    //   (60 / 120, the fixed rungs' values) keeps a ratio above 1.4×.
    // * sin/cos and sinh/cosh, |r| ≤ π/4 resp. |x| < 0.5: below
    //   10^-(prec + 5) near n ≈ 0.35·prec (20 at 50, 40 at 110), so
    //   `2·prec + 20` (120 / 240) keeps a ratio above 5×.
    // * log1p, |u| ≤ 0.5: needs n ≳ (prec + 5)·log2(10) ≈ 3.4·prec
    //   (166 at 50, 382 at 110), so `5·prec` (250 / 550) keeps a ratio
    //   above 1.4×.
    // * atan, |t| ≤ tan(π/8): (2n+1)·log10(1/tan(π/8)) ≥ prec + 5 gives
    //   n ≈ 1.31·prec (65 at 50, 150 at 110), so `4·prec + 10`
    //   (210 / 450) keeps a ratio above 3×.
    //
    // Every formula reproduces rung 2's constant at prec = 110, which
    // `series_cap_formulas_pin_both_widths` pins; the atan formula is
    // the one that reads 10 terms above rung 1's 200 at prec = 50,
    // since a single linear rule cannot hit both hand-picked constants
    // and a cap that is 5% generous costs nothing (the loops exit early
    // and never approach it).
    //
    // Every loop still exits early on `next_sum == sum`, so the caps
    // are convergence backstops, not iteration counts.
    fn exp_series_terms(&self) -> u32 {
        self.arena.prec + 10
    }
    fn sin_cos_series_terms(&self) -> u32 {
        2 * self.arena.prec + 20
    }
    fn sinh_cosh_series_terms(&self) -> u32 {
        2 * self.arena.prec + 20
    }
    fn log1p_series_terms(&self) -> u32 {
        5 * self.arena.prec
    }
    fn atan_series_terms(&self) -> u32 {
        4 * self.arena.prec + 10
    }

    fn zero(&self) -> Self {
        ExtendedDyn::zero(self)
    }
    fn one(&self) -> Self {
        ExtendedDyn::one(self)
    }
    fn half(&self) -> Self {
        ExtendedDyn::half(self)
    }

    fn pi(&self) -> Self {
        ExtendedDyn::pi(self)
    }
    fn e(&self) -> Self {
        ExtendedDyn::e(self)
    }
    fn ln2(&self) -> Self {
        ExtendedDyn::ln2(self)
    }
    fn ln10(&self) -> Self {
        ExtendedDyn::ln10(self)
    }
    fn inv_ln10(&self) -> Self {
        ExtendedDyn::inv_ln10(self)
    }
    fn inv_ln2(&self) -> Self {
        ExtendedDyn::inv_ln2(self)
    }
    fn pi_over_two(&self) -> Self {
        ExtendedDyn::pi_over_two(self)
    }
    fn pi_over_four(&self) -> Self {
        ExtendedDyn::pi_over_four(self)
    }
    fn tan_pi_over_eight(&self) -> Self {
        ExtendedDyn::tan_pi_over_eight(self)
    }

    fn from_i32(&self, n: i32) -> Self {
        ExtendedDyn::from_i32(self, n)
    }
    fn parse_str(&self, s: &str) -> Self {
        ExtendedDyn::parse_str(self, s)
    }
    fn from_parts_u128(&self, coef: u128, exp: i32, sign: bool) -> Self {
        ExtendedDyn::from_parts_u128(self, coef, exp, sign)
    }
    fn from_components_with_sticky(
        &self,
        coef: U256,
        exp: i32,
        sign: bool,
        pre_sticky: bool,
    ) -> Self {
        ExtendedDyn::from_components_with_sticky(self, coef, exp, sign, pre_sticky)
    }
    fn from_format<F: DecimalFormat>(&self, d: F) -> Self {
        ExtendedDyn::from_format(self, d)
    }
    fn from_extended(&self, x: Extended) -> Self {
        ExtendedDyn::from_extended(self, x)
    }
    fn saturate_overflow(&self, sign: bool) -> Self {
        ExtendedDyn::saturate_overflow(self, sign)
    }
    fn saturate_underflow(&self) -> Self {
        ExtendedDyn::saturate_underflow(self)
    }

    fn sign(self) -> bool {
        self.sign
    }
    fn exponent(self) -> i32 {
        self.exp
    }
    fn digit_count(self) -> u32 {
        ExtendedDyn::digit_count(self)
    }
    fn is_zero(self) -> bool {
        ExtendedDyn::is_zero(self)
    }
    fn with_sign(self, sign: bool) -> Self {
        ExtendedDyn::with_sign(self, sign)
    }
    fn with_exponent(self, exp: i32) -> Self {
        ExtendedDyn::with_exponent(self, exp)
    }

    fn neg(self) -> Self {
        ExtendedDyn::neg(self)
    }
    fn abs(self) -> Self {
        ExtendedDyn::abs(self)
    }
    fn add(self, other: Self) -> Self {
        ExtendedDyn::add(self, other)
    }
    fn sub(self, other: Self) -> Self {
        ExtendedDyn::sub(self, other)
    }
    fn mul(self, other: Self) -> Self {
        ExtendedDyn::mul(self, other)
    }
    fn square(self) -> Self {
        ExtendedDyn::square(self)
    }
    fn div<F: DecimalFormat>(self, other: Self) -> Self {
        ExtendedDyn::div::<F>(self, other)
    }
    fn recip<F: DecimalFormat>(self) -> Self {
        ExtendedDyn::recip::<F>(self)
    }
    fn sqrt<F: DecimalFormat>(self) -> Self {
        ExtendedDyn::sqrt::<F>(self)
    }
    fn div_u32(self, divisor: u32) -> Self {
        ExtendedDyn::div_u32(self, divisor)
    }
    fn mul_pow10_exp(self, k: i32) -> Self {
        ExtendedDyn::mul_pow10_exp(self, k)
    }

    fn cmp(self, other: Self) -> Ordering {
        ExtendedDyn::cmp(self, other)
    }

    fn trunc_to_i32(self) -> i32 {
        ExtendedDyn::trunc_to_i32(self)
    }

    fn to_format<F: DecimalFormat>(self, q_preferred: i32, rm: RoundingMode) -> (F, Status) {
        ExtendedDyn::to_format::<F>(self, q_preferred, rm)
    }
    fn to_format_with_residual<F: DecimalFormat>(
        self,
        magnitude_grows: bool,
        rm: RoundingMode,
    ) -> (F, Status) {
        ExtendedDyn::to_format_with_residual::<F>(self, magnitude_grows, rm)
    }
    fn sticks_to(self, anchor: Self) -> bool {
        ExtendedDyn::sticks_to(self, anchor)
    }
    fn near_rounding_boundary<F: DecimalFormat>(self, budget: u128) -> bool {
        ExtendedDyn::near_rounding_boundary::<F>(self, budget)
    }

    // The unbounded rung always has a wider rung available (the Ziv
    // driver simply widens the arena), so a near-boundary verdict here
    // escalates rather than delivering.
    const ESCALATES: bool = true;
    const RUNG: u8 = 3;

    fn rung_budget(&self, budget: &crate::ladder::Budget) -> u128 {
        (budget.dynamic)(self.arena.prec)
    }

    #[cfg(feature = "trig")]
    fn reduce_trig<F: DecimalFormat>(&self, x: F) -> (u32, Self, Status) {
        crate::argred::reduce_dyn::<F>(*self, x)
    }
}

// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extended2::Extended2;
    use crate::mock_format::MockFmt;
    // Only the composed `exp` differential feeds format data in; the
    // import rides its gate so a `trig,unbounded-ladder` build without
    // `exp-log` stays warning-free.
    #[cfg(feature = "exp-log")]
    use crate::mock_format::ValueFmt128;
    use alloc::string::String;
    use ferrodec_ieee::IeeeDecodedClass as Class;
    use ferrodec_multiword::U384;

    /// The two widths this suite exercises: the mirror and oracle
    /// tiers run wide (220), the cross-substrate differential runs at
    /// rung 2's own width so `Extended2` is a term-for-term oracle.
    const WIDE: u32 = 220;
    const RUNG2: u32 = 110;

    fn arena(prec: u32) -> DynArena {
        DynArena::new(prec)
    }

    fn dv<'a>(a: &'a DynArena, s: &str) -> ExtendedDyn<'a> {
        a.exemplar().parse_str(s)
    }

    fn assert_dyn_eq(got: ExtendedDyn<'_>, want: ExtendedDyn<'_>, label: &str) {
        assert_eq!(
            got.cmp(want),
            Ordering::Equal,
            "{label}: got {got:?}, want {want:?}"
        );
    }

    /// `|got − want| ≤ 10^(decade(want) + decades)`, the decade form of
    /// a relative-agreement bound. `decades = −(d − 1)` is "within one
    /// unit in the `d`-th significant digit".
    ///
    /// A zero reference admits no relative bound, so it demands exact
    /// agreement instead: a cancellation the fixed rung resolves to
    /// zero must cancel on the growable substrate too.
    fn assert_agrees(got: ExtendedDyn<'_>, want: ExtendedDyn<'_>, decades: i32, label: &str) {
        if want.is_zero() {
            assert!(
                got.is_zero(),
                "{label}: reference cancelled to zero, got {got:?}"
            );
            return;
        }
        let d = got.sub(want).abs();
        if d.is_zero() {
            return;
        }
        let d_adj = d.exp + d.digit_count() as i32 - 1;
        let w_adj = want.exp + want.digit_count() as i32 - 1;
        assert!(
            d_adj - w_adj <= decades,
            "{label}: disagreement at relative decade {} (bound {decades}); \
             got {got:?}, want {want:?}",
            d_adj - w_adj
        );
    }

    // -----------------------------------------------------------------
    // Tier 1: the mirror unit suite, transposed to prec = 220.

    #[test]
    fn add_basic() {
        let a = arena(WIDE);
        assert_dyn_eq(
            dv(&a, "1.5").add(dv(&a, "2.25")),
            dv(&a, "3.75"),
            "1.5 + 2.25",
        );
    }

    #[test]
    fn sub_basic() {
        let a = arena(WIDE);
        assert_dyn_eq(
            dv(&a, "3.75").sub(dv(&a, "1.25")),
            dv(&a, "2.5"),
            "3.75 - 1.25",
        );
    }

    #[test]
    fn mul_basic() {
        let a = arena(WIDE);
        assert_dyn_eq(dv(&a, "3.5").mul(dv(&a, "4.0")), dv(&a, "14.0"), "3.5 * 4");
    }

    #[test]
    fn mul_at_the_precision_boundary() {
        // (10^110)² = 10^220, exactly at the working width.
        let a = arena(WIDE);
        assert_dyn_eq(
            dv(&a, "1e110").mul(dv(&a, "1e110")),
            dv(&a, "1e220"),
            "10^110 squared",
        );
    }

    #[test]
    fn div_u32_expands_to_working_precision() {
        // 10/3 to 220 digits: "3." followed by 219 more 3s.
        let a = arena(WIDE);
        let mut want = String::from("3.");
        for _ in 0..219 {
            want.push('3');
        }
        assert_dyn_eq(dv(&a, "10").div_u32(3), dv(&a, &want), "10 / 3");
    }

    #[test]
    fn div_u32_terminates_clean() {
        let a = arena(WIDE);
        assert_dyn_eq(dv(&a, "100").div_u32(4), dv(&a, "25"), "100 / 4");
    }

    #[test]
    fn cmp_signs() {
        let a = arena(WIDE);
        assert_eq!(dv(&a, "1").cmp(dv(&a, "2")), Ordering::Less);
        assert_eq!(dv(&a, "-1").cmp(dv(&a, "2")), Ordering::Less);
        assert_eq!(dv(&a, "-1").cmp(dv(&a, "-2")), Ordering::Greater);
        assert_eq!(dv(&a, "0").cmp(dv(&a, "0")), Ordering::Equal);
        assert_eq!(dv(&a, "0").cmp(dv(&a, "0").neg()), Ordering::Equal);
    }

    #[test]
    fn add_cancellation_preserves_working_precision() {
        // 1 − (1 − 1e-200) recovers 1e-200 exactly: the 220-digit
        // envelope holds every digit of the cancellation.
        let a = arena(WIDE);
        let one = dv(&a, "1");
        let tiny = dv(&a, "1e-200");
        assert_dyn_eq(one.sub(one.sub(tiny)), tiny, "1 - (1 - 1e-200)");
    }

    #[test]
    fn from_extended_is_value_exact() {
        let a = arena(WIDE);
        let ex = a.exemplar();
        for s in ["3.14159265358979323846", "-0.5", "1e-30", "9.99e6000"] {
            assert_dyn_eq(ex.from_extended(Extended::parse_str(s)), dv(&a, s), s);
        }
        assert!(ex.from_extended(Extended::ZERO).is_zero());
    }

    #[test]
    fn trunc_to_i32_truncates_toward_zero() {
        let a = arena(WIDE);
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
            assert_eq!(dv(&a, s).trunc_to_i32(), want, "input {s}");
        }
    }

    #[test]
    fn sticks_to_threshold_is_precision_scaled() {
        // The band is `prec − 3` = 217 decades below the anchor.
        let a = arena(WIDE);
        let one = a.exemplar().one();
        let mut inside = String::from("1.");
        for _ in 0..217 {
            inside.push('0');
        }
        inside.push('1');
        assert!(dv(&a, &inside).sticks_to(one), "1 + 1e-218 must snap");

        let mut outside = String::from("1.");
        for _ in 0..215 {
            outside.push('0');
        }
        outside.push('1');
        assert!(
            !dv(&a, &outside).sticks_to(one),
            "1 + 1e-216 must stay separated"
        );
    }

    #[test]
    fn series_cap_formulas_pin_both_widths() {
        for (prec, exp_terms, trig_terms, log1p_terms, atan_terms) in
            [(RUNG2, 120, 240, 550, 450), (WIDE, 230, 460, 1100, 890)]
        {
            let a = arena(prec);
            let ex = a.exemplar();
            assert_eq!(ex.precision(), prec);
            assert_eq!(ex.exp_series_terms(), exp_terms, "exp at {prec}");
            assert_eq!(ex.sin_cos_series_terms(), trig_terms, "sin/cos at {prec}");
            assert_eq!(
                ex.sinh_cosh_series_terms(),
                trig_terms,
                "sinh/cosh at {prec}"
            );
            assert_eq!(ex.log1p_series_terms(), log1p_terms, "log1p at {prec}");
            assert_eq!(ex.atan_series_terms(), atan_terms, "atan at {prec}");
        }
        // The rung 2 row above is the fixed rung's own cap vector; that
        // agreement is what makes the dynamic rung a continuation of
        // the ladder rather than a second policy.
        let a = arena(RUNG2);
        let ex = a.exemplar();
        let fixed = Extended2::ZERO;
        assert_eq!(ex.exp_series_terms(), fixed.exp_series_terms());
        assert_eq!(ex.sin_cos_series_terms(), fixed.sin_cos_series_terms());
        assert_eq!(ex.sinh_cosh_series_terms(), fixed.sinh_cosh_series_terms());
        assert_eq!(ex.log1p_series_terms(), fixed.log1p_series_terms());
        assert_eq!(ex.atan_series_terms(), fixed.atan_series_terms());
    }

    #[test]
    fn near_rounding_boundary_bands_d128_drop() {
        // At 220 working digits the Decimal128 drop is 220 − 34 = 186;
        // construct prefix · 10^186 + base ± off and pin the band
        // distances, mirroring the rung 2 test's shape.
        type Shape = MockFmt<34, 6176>;

        let a = arena(WIDE);
        let ex = a.exemplar();
        let prefix: u128 = 1_234_567_890_123_456_789_012_345_678_901_234;
        let half = DecBig::from_u32(5).mul_pow10(185);
        let field = DecBig::pow10(186);
        for base in [DecBig::zero(), half, field] {
            for off in [-3i128, -1, 0, 1, 3] {
                let stem = DecBig::from_u128(prefix).mul_pow10(186).add(&base);
                let coef = if off >= 0 {
                    stem.add(&DecBig::from_u128(off.unsigned_abs()))
                } else {
                    stem.sub(&DecBig::from_u128(off.unsigned_abs()))
                };
                let v = ex.make(coef, -50, false);
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

    #[test]
    fn arena_floor_is_rung2_width_and_seeds_a_canonical_zero() {
        // The floor is rung 2's own width: the Ziv driver reaches this
        // rung only after the fixed ladder runs out.
        assert_eq!(MIN_DYN_PRECISION, 110);
        let a = arena(MIN_DYN_PRECISION);
        let ex = a.exemplar();
        assert_eq!(a.precision(), MIN_DYN_PRECISION);
        assert_eq!(ex.precision(), MIN_DYN_PRECISION);
        assert!(ex.is_zero(), "the exemplar is the canonical zero");
        assert_eq!(ex.digit_count(), 1, "zero counts one digit (GDA)");
        assert!(!ex.sign() && ex.exponent() == 0);
    }

    #[test]
    #[should_panic(expected = "below the dynamic rung's floor")]
    fn arena_below_the_floor_panics() {
        let _ = DynArena::new(MIN_DYN_PRECISION - 1);
    }

    // -----------------------------------------------------------------
    // Tier 2: oracle cross-check against astro-float at 1200 bits
    // (~361 decimal digits), agreement within 1 ULP at 220 digits.

    const ORACLE_P: usize = 1200;

    fn dyn_to_string(v: ExtendedDyn<'_>) -> String {
        if v.is_zero() {
            return String::from("0");
        }
        let sign = if v.sign { "-" } else { "" };
        alloc::format!("{sign}{}e{}", v.coef(), v.exp)
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

    /// One ULP at 220 digits is `10^-219` relative; the leading digit
    /// of a 220-digit coefficient makes that a conservative unit.
    fn af_within_ulp_220(a: &astro_float::BigFloat, b: &astro_float::BigFloat) -> bool {
        use astro_float::{BigFloat, RoundingMode as AfRm};
        let rm = AfRm::None;
        let mut cc = astro_float::Consts::new().unwrap();
        let diff = a.sub(b, ORACLE_P, rm).abs();
        let abs_b = b.abs();
        let bound = parse_af("1e-219", &mut cc);
        if abs_b.cmp(&BigFloat::from(0)) == Some(0) {
            return matches!(diff.cmp(&bound), Some(o) if o <= 0);
        }
        let rel = diff.div(&abs_b, ORACLE_P, rm);
        matches!(rel.cmp(&bound), Some(o) if o <= 0)
    }

    #[test]
    fn oracle_add_pairs() {
        let a = arena(WIDE);
        let pairs = [
            ("1.5", "2.25"),
            ("0.1", "0.2"),
            ("1e60", "1e-60"),
            ("999.9999999999999", "0.0000000000000001"),
            ("-3.5", "5.25"),
            ("1.234567890123456789012345678901234", "1e-220"),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, b_s) in pairs {
            let got = dv(&a, a_s).add(dv(&a, b_s));
            let got_af = parse_af(&dyn_to_string(got), &mut cc);
            let want_af = parse_af(a_s, &mut cc).add(
                &parse_af(b_s, &mut cc),
                ORACLE_P,
                astro_float::RoundingMode::None,
            );
            assert!(
                af_within_ulp_220(&got_af, &want_af),
                "add({a_s}, {b_s}) exceeds 1 ULP at 220 digits"
            );
        }
    }

    #[test]
    fn oracle_mul_pairs() {
        let a = arena(WIDE);
        let pairs = [
            ("3.5", "4.0"),
            ("1.1", "1.1"),
            ("0.9999999999999", "1.0000000000001"),
            ("3.14159265358979323846", "2.71828182845904523536"),
            ("1e110", "1e-110"),
            ("-1.5", "1.5"),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, b_s) in pairs {
            let got = dv(&a, a_s).mul(dv(&a, b_s));
            let got_af = parse_af(&dyn_to_string(got), &mut cc);
            let want_af = parse_af(a_s, &mut cc).mul(
                &parse_af(b_s, &mut cc),
                ORACLE_P,
                astro_float::RoundingMode::None,
            );
            assert!(
                af_within_ulp_220(&got_af, &want_af),
                "mul({a_s}, {b_s}) exceeds 1 ULP at 220 digits"
            );
        }
    }

    #[test]
    fn oracle_div_u32_cases() {
        let a = arena(WIDE);
        let cases = [
            ("10", 3u32),
            ("1", 7),
            ("355", 113),
            ("1.234567890123456789012345678901234", 17),
        ];
        let mut cc = astro_float::Consts::new().unwrap();
        for (a_s, d) in cases {
            let got = dv(&a, a_s).div_u32(d);
            let got_af = parse_af(&dyn_to_string(got), &mut cc);
            let d_af = astro_float::BigFloat::from_word(u64::from(d), ORACLE_P);
            let want_af =
                parse_af(a_s, &mut cc).div(&d_af, ORACLE_P, astro_float::RoundingMode::None);
            assert!(
                af_within_ulp_220(&got_af, &want_af),
                "div_u32({a_s}, {d}) exceeds 1 ULP at 220 digits"
            );
        }
    }

    #[test]
    fn oracle_wide_constant_product() {
        // π · e from the runtime generators at 225 digits, rounded once
        // by the 220-digit product, against the oracle's independently
        // derived π · e. Exercises the generators, the scale
        // derivations and the growable product path together.
        let a = arena(WIDE);
        let ex = a.exemplar();
        let mut cc = astro_float::Consts::new().unwrap();
        let prod = ex.pi().mul(ex.e());
        let got_af = parse_af(&dyn_to_string(prod), &mut cc);
        let pi_af = cc.pi(ORACLE_P, astro_float::RoundingMode::None);
        let one = parse_af("1", &mut cc);
        let e_af = one.exp(ORACLE_P, astro_float::RoundingMode::None, &mut cc);
        let want_af = pi_af.mul(&e_af, ORACLE_P, astro_float::RoundingMode::None);
        assert!(
            af_within_ulp_220(&got_af, &want_af),
            "pi * e exceeds 1 ULP at 220 digits"
        );
    }

    // -----------------------------------------------------------------
    // Tier 3: the cross-substrate differential at prec = 110. The
    // dynamic rung and rung 2 run the same clauses at the same width on
    // different substrates, so rung 2 is a term-for-term oracle here —
    // the load-bearing guard on the mirror.

    /// `U384` → `DecBig`, test-only: the differential needs rung 2
    /// values on the growable substrate and nothing in production does.
    fn u384_to_decbig(c: U384) -> DecBig {
        let mut digits: Vec<u8> = Vec::new();
        let mut cur = c;
        while !cur.is_zero() {
            let (q, d) = cur.div_rem10();
            digits.push(b'0' + d as u8);
            cur = q;
        }
        digits.reverse();
        DecBig::from_ascii_digits(&digits)
    }

    fn ext2_as_dyn(a: &DynArena, v: Extended2) -> ExtendedDyn<'_> {
        a.exemplar().make(u384_to_decbig(v.coef), v.exp, v.sign)
    }

    /// A Decimal128-shaped format with a working rounder and working
    /// Newton seeds.
    ///
    /// `mock_format::ValueFmt128` carries values but leaves
    /// `round_and_pack_finite`, `recip_seed` and `sqrt_seed`
    /// `unreachable!()`, and `recip` / `sqrt` need all three. This mock
    /// fills exactly those three in, on the same substrate: a
    /// round-half-even collapse to 34 digits and truncated 34-digit
    /// seeds (`10^67 / c` for the reciprocal, an integer square root
    /// for the root). Truncation costs under one unit in the 34th
    /// digit, so a seed still carries ≥ 33 correct digits — what the
    /// derived Newton step count assumes. No exponent-range clamping
    /// and no subnormals: the differential feeds it only
    /// format-representable magnitudes.
    #[derive(Clone, Copy, Debug)]
    struct SeedFmt128 {
        coef: u128,
        exp: i32,
        sign: bool,
    }

    impl DecimalFormat for SeedFmt128 {
        const BIAS: i32 = 6176;
        const PRECISION: u32 = 34;
        const ZERO: Self = Self {
            coef: 0,
            exp: 0,
            sign: false,
        };
        const NEG_ZERO: Self = Self {
            coef: 0,
            exp: 0,
            sign: true,
        };
        const ONE: Self = Self {
            coef: 1,
            exp: 0,
            sign: false,
        };
        const NEG_ONE: Self = Self {
            coef: 1,
            exp: 0,
            sign: true,
        };
        const TEN: Self = Self {
            coef: 10,
            exp: 0,
            sign: false,
        };
        const INFINITY: Self = Self {
            coef: 0,
            exp: 0,
            sign: false,
        };
        const NEG_INFINITY: Self = Self {
            coef: 0,
            exp: 0,
            sign: true,
        };
        const NAN: Self = Self {
            coef: 0,
            exp: 0,
            sign: false,
        };
        const SIGNALING_NAN: Self = Self {
            coef: 0,
            exp: 0,
            sign: false,
        };
        fn classify(self) -> Class {
            unreachable!()
        }
        fn is_nan(self) -> bool {
            unreachable!()
        }
        fn is_zero(self) -> bool {
            self.coef == 0
        }
        fn is_infinite(self) -> bool {
            unreachable!()
        }
        fn is_sign_negative(self) -> bool {
            self.sign
        }
        fn is_signaling_nan(self) -> bool {
            unreachable!()
        }
        fn abs(self) -> Self {
            Self {
                sign: false,
                ..self
            }
        }
        fn neg(self) -> Self {
            Self {
                sign: !self.sign,
                ..self
            }
        }
        fn partial_cmp_fmt(self, _other: Self) -> (Option<Ordering>, Status) {
            unreachable!()
        }
        fn nan_from(self) -> Self {
            unreachable!()
        }
        fn propagate_nan2(self, _other: Self) -> Self {
            unreachable!()
        }
        fn to_extended_parts(self) -> Option<(U256, i32, bool)> {
            Some((U256::from_u128(self.coef), self.exp, self.sign))
        }
        fn round_and_pack_finite(
            coef: U256,
            unbiased_exp: i32,
            _q_preferred: i32,
            sign: bool,
            pre_sticky: bool,
            _rm: RoundingMode,
            status: Status,
        ) -> (Self, Status) {
            let (rounded, shift) =
                round_decbig_to_prec(u256_to_decbig(coef), Self::PRECISION, pre_sticky);
            (
                Self {
                    coef: rounded.to_u128().expect("34 digits fit u128"),
                    exp: unbiased_exp + shift as i32,
                    sign,
                },
                status,
            )
        }
        fn recip_seed(self, _rm: RoundingMode) -> (Self, Status) {
            // 1/(c · 10^e) = (10^67 / c) · 10^(−67 − e).
            let q = DecBig::pow10(67).div_rem(&DecBig::from_u128(self.coef)).0;
            let shift = digit_count_of(&q).saturating_sub(Self::PRECISION);
            let (kept, _) = q.div_rem_pow10(shift);
            (
                Self {
                    coef: kept.to_u128().expect("34 digits fit u128"),
                    exp: -67 - self.exp + shift as i32,
                    sign: self.sign,
                },
                Status::OK,
            )
        }
        fn sqrt_seed(self, _rm: RoundingMode) -> (Self, Status) {
            // √(c · 10^e) = √(c · 10^s) · 10^((e − s)/2), with `s`
            // chosen to match `e`'s parity so the halving is exact and
            // deep enough that the integer root keeps ≥ 34 digits.
            let s: i32 = if self.exp.rem_euclid(2) == 0 { 68 } else { 67 };
            let (root, _) = DecBig::from_u128(self.coef)
                .mul_pow10(s.unsigned_abs())
                .isqrt();
            let shift = digit_count_of(&root).saturating_sub(Self::PRECISION);
            let (kept, _) = root.div_rem_pow10(shift);
            (
                Self {
                    coef: kept.to_u128().expect("34 digits fit u128"),
                    exp: (self.exp - s) / 2 + shift as i32,
                    sign: false,
                },
                Status::OK,
            )
        }
        fn div_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
            unreachable!()
        }
        fn mul_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
            unreachable!()
        }
        fn to_i32_fmt(self, _rm: RoundingMode) -> (i32, Status) {
            unreachable!()
        }
        fn exp_overflow_limit() -> Extended {
            unreachable!()
        }
        fn exp_underflow_limit() -> Extended {
            unreachable!()
        }
    }

    /// One unit in the 110th significant digit.
    const ULP_110: i32 = -109;

    /// Each runtime constant against the hand-curated 115-digit literal
    /// the fixed rung delivers.
    ///
    /// Observed: the two agree to relative decade −114, i.e. they can
    /// differ by one unit in the 115th digit and no more. That is the
    /// generators' documented truncation (they floor, the literals were
    /// rounded to nearest) and nothing else, which is why the ≤ 1 unit
    /// in the 110th digit asserted here has five decimal orders of
    /// margin. A scale slip in any of the exponent derivations lands
    /// whole decades away and fails immediately.
    #[test]
    fn differential_constants_match_rung2() {
        let a = arena(RUNG2);
        let ex = a.exemplar();
        let pairs: [(&str, ExtendedDyn<'_>, Extended2); 9] = [
            ("pi", ex.pi(), crate::consts::pi_ext2()),
            ("e", ex.e(), crate::consts::e_ext2()),
            ("ln2", ex.ln2(), crate::consts::ln2_ext2()),
            ("ln10", ex.ln10(), crate::consts::ln10_ext2()),
            ("inv_ln10", ex.inv_ln10(), crate::consts::inv_ln10_ext2()),
            ("inv_ln2", ex.inv_ln2(), crate::consts::inv_ln2_ext2()),
            (
                "pi_over_two",
                ex.pi_over_two(),
                crate::consts::pi_over_two_ext2(),
            ),
            (
                "pi_over_four",
                ex.pi_over_four(),
                crate::consts::pi_over_four_ext2(),
            ),
            (
                "tan_pi_over_eight",
                ex.tan_pi_over_eight(),
                crate::consts::tan_pi_over_eight_ext2(),
            ),
        ];
        for (name, got, want) in pairs {
            assert_agrees(got, ext2_as_dyn(&a, want), ULP_110, name);
        }
    }

    /// The parsed grid the arithmetic differential runs over. Chosen to
    /// cover the rounding clause (wide coefficients), the alignment
    /// clause (far-apart exponents), the cancellation clause and both
    /// signs.
    const GRID: [&str; 12] = [
        "1.5",
        "2.25",
        "0.1",
        "-3.5",
        "5.25",
        "1e60",
        "1e-60",
        "999.9999999999999",
        "3.14159265358979323846264338327950288419716939937510582097494459230781640628620899862803",
        "-1.0000000000000000000000000000000001",
        "0.9999999999999999999999999999999999",
        "355",
    ];

    /// Observed on this grid: byte-identical results, not merely
    /// results inside the ULP bound. That is the expected outcome —
    /// same clauses, same width, same round-half-even tie rule, only a
    /// different coefficient substrate — and it is what makes the
    /// asserted 1 ULP a genuine alarm rather than a slack envelope.
    #[test]
    fn differential_add_sub_mul_match_rung2() {
        let a = arena(RUNG2);
        for x in GRID {
            for y in GRID {
                let (dx, dy) = (dv(&a, x), dv(&a, y));
                let (ex2, ey2) = (Extended2::parse_str(x), Extended2::parse_str(y));
                assert_agrees(
                    dx.add(dy),
                    ext2_as_dyn(&a, ex2.add(ey2)),
                    ULP_110,
                    &alloc::format!("add({x}, {y})"),
                );
                assert_agrees(
                    dx.sub(dy),
                    ext2_as_dyn(&a, ex2.sub(ey2)),
                    ULP_110,
                    &alloc::format!("sub({x}, {y})"),
                );
                assert_agrees(
                    dx.mul(dy),
                    ext2_as_dyn(&a, ex2.mul(ey2)),
                    ULP_110,
                    &alloc::format!("mul({x}, {y})"),
                );
            }
        }
    }

    #[test]
    fn differential_div_u32_matches_rung2() {
        let a = arena(RUNG2);
        for x in GRID {
            for d in [3u32, 7, 17, 113, 1_000_003] {
                assert_agrees(
                    dv(&a, x).div_u32(d),
                    ext2_as_dyn(&a, Extended2::parse_str(x).div_u32(d)),
                    ULP_110,
                    &alloc::format!("div_u32({x}, {d})"),
                );
            }
        }
    }

    #[test]
    fn differential_recip_and_sqrt_match_rung2() {
        let a = arena(RUNG2);
        for x in GRID {
            let dx = dv(&a, x);
            let ex2 = Extended2::parse_str(x);
            assert_agrees(
                dx.recip::<SeedFmt128>(),
                ext2_as_dyn(&a, ex2.recip::<SeedFmt128>()),
                ULP_110,
                &alloc::format!("recip({x})"),
            );
            if !ex2.sign {
                assert_agrees(
                    dx.sqrt::<SeedFmt128>(),
                    ext2_as_dyn(&a, ex2.sqrt::<SeedFmt128>()),
                    ULP_110,
                    &alloc::format!("sqrt({x})"),
                );
            }
        }
    }

    /// The composed differential: one whole kernel body run on both
    /// substrates at the same width.
    ///
    /// Bound derivation. The two runs differ only in where their
    /// constants come from — 115-digit literals on rung 2, generated
    /// 115-digit values here — and the generators land within one unit
    /// of their last digit, so `ln 10` and `1/ln 10` agree to
    /// ≤ 2 × 10^-114 relative. The reduction `r = x − k·ln 10` turns
    /// that into an absolute error of `|k·ln 10| · 2e-114 ≤ 701 ·
    /// 2e-114 ≈ 1.4e-111` over this grid, and `d(e^r)/e^r = dr` maps it
    /// 1:1 into result-relative error. On top of that the two series
    /// round independently at 110 digits: ≤ 3 roundings per term over
    /// ≤ 120 terms is ≤ 360 units of 10^-110, i.e. ≤ 3.6e-108. The sum
    /// is ≤ 4e-108, so **1e-105** carries a factor of ~250 of margin
    /// while still failing loudly on a genuine clause divergence (any
    /// of which lands at 10^-109 relative or worse).
    ///
    /// Observed on this grid: byte-identical results. The 115-digit
    /// constants differ from the literals only in their last digit, and
    /// the first 110-digit rounding of the reduction absorbs it, so the
    /// derived bound above is an envelope rather than a measurement.
    #[cfg(feature = "exp-log")]
    #[test]
    fn differential_exp_body_matches_rung2() {
        use crate::exp::exp_extended_body;

        /// 1e-105 relative, in the decade form `assert_agrees` takes.
        const COMPOSED_BOUND: i32 = -105;

        let a = arena(RUNG2);
        let ex = a.exemplar();
        // (coefficient, exponent, sign) at the Decimal128 shape.
        let grid = [
            (3u128, -1i32, false), //  0.3
            (3, -1, true),         // -0.3
            (17, -1, false),       //  1.7
            (17, -1, true),        // -1.7
            (1125, -2, false),     //  11.25
            (1125, -2, true),      // -11.25
            (7005, -1, false),     //  700.5
            (65025, -2, true),     // -650.25
        ];
        for (coef, exp, sign) in grid {
            let v = ValueFmt128 { coef, exp, sign };
            let got = exp_extended_body(ex.from_format::<ValueFmt128>(v));
            let want = exp_extended_body(Extended2::from_format(v));
            assert_agrees(
                got,
                ext2_as_dyn(&a, want),
                COMPOSED_BOUND,
                &alloc::format!("exp({coef}e{exp}, sign={sign})"),
            );
        }
    }
}
