//! Quantum-manipulating operations for [`Decimal64`].
//!
//! * [`Decimal64::quantize`] — IEEE 754-2019 §5.3.3: rescale `self`
//!   to have the same quantum as `target`, rounding by `rm`. Raises
//!   `INVALID` when the result cannot fit in the format with the
//!   target's quantum.
//! * [`Decimal64::scaleb`] — IEEE 754-2019 §5.3.3: multiply `self`
//!   by `10^n` where `n` is an integer.
//! * [`Decimal64::logb`] — IEEE 754-2019 §5.3.3: integer log base
//!   10 of `|self|` — equivalently, the value's adjusted exponent.
//! * [`Decimal64::next_up`] / [`Decimal64::next_down`] — §5.3.1
//!   navigation operations to the next representable value.

use crate::bid::{
    classify_bits, decimal_digit_count, Class, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, E_MAX,
    PRECISION,
};
use crate::decimal::Decimal64;
use ferrodec_ieee::{should_round_up, RoundingMode, Status};

use super::round::round_and_pack_finite;

/// GDA `scaleb` constrains the shift: `|n| ≤ 2 × (Emax + precision)`.
/// For decimal64 that is `2 × (384 + 16) = 800`; `|n|` beyond it is
/// `Invalid_operation` (decTest ddscb120..127: `n = ±800` overflows
/// or underflows to a finite extreme, `n = ±801` is NaN INVALID).
/// Bounding `n` first also keeps the later `biased_exp - BIAS + n`
/// arithmetic inside `i32` (without it, `n` near `i32::MAX`
/// overflowed: panic in debug, silent wrap in release). Agent 4
/// finding Q2 / M2.
const SCALEB_N_LIMIT: u32 = 2 * (E_MAX as u32 + PRECISION);

const POW10_U64: [u64; 10] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
];

impl Decimal64 {
    /// IEEE 754-2019 §5.3.3 `quantize(self, target)`: returns a value
    /// numerically equal to `self` (after rounding by `rm`) but with
    /// the same quantum exponent as `target`.
    ///
    /// `INVALID` is raised when `target`'s quantum is incompatible
    /// (the rescaled coefficient would exceed `PRECISION` digits) or
    /// when either operand is NaN with sNaN semantics. Infinity
    /// quantizes to itself only when `target` is also infinity;
    /// otherwise INVALID.
    #[must_use]
    pub fn quantize(self, target: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(target.0);

        if let Some(special) = quantize_special_cases(ca, cb) {
            return special;
        }

        // Both finite (or self zero).
        let (sign, biased_self, coef) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let target_biased = match cb {
            Class::Finite { biased_exp, .. } | Class::Zero { biased_exp, .. } => biased_exp,
            _ => unreachable!(),
        };

        let target_q = target_biased as i32 - BIAS as i32;
        let self_q = biased_self as i32 - BIAS as i32;

        // Preferred quantum for quantize is target's quantum. Pass
        // target_q as both unbiased_exp (effectively "where the
        // coefficient lives") and q_preferred only when the
        // coefficient already sits at target_q; otherwise we need to
        // round.
        //
        // Strategy: pass coef and self_q; round_and_pack will shift
        // toward target_q (pad on inexact, strip on exact within the
        // PRECISION budget). But the function's strip logic only
        // fires up to PRECISION digits; for cases that need shifting
        // outside that envelope we have to handle directly.

        // target_biased came from classify_bits (10-bit field decode).
        let target_biased_typed = crate::bid::BiasedExp::try_from_biased(target_biased)
            .expect("target_biased from classify_bits");

        // Step 1: rescale to target_q.
        if target_q == self_q {
            // Already at the right quantum; pack as-is. coef came from
            // classify_bits which bounds it below COEFFICIENT_LIMIT.
            let coef_typed =
                crate::bid::Coefficient::try_new(coef).expect("coef from classify_bits");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign,
                    target_biased_typed,
                    coef_typed,
                )),
                Status::OK,
            );
        }

        if target_q > self_q {
            // Drop digits (round). Number of decimal places to drop:
            // target_q - self_q.
            let drop = (target_q - self_q) as u32;
            // After dropping `drop` digits we may have empty
            // coefficient (zero); rounding may or may not bump it.
            let digits: u32 = if coef == 0 {
                0
            } else {
                decimal_digit_count(coef)
            };
            let (kept, round_digit, sticky): (u64, u32, bool) = if drop >= digits {
                // All digits dropped. round_digit is the MSD if
                // drop == digits, 0 (with sticky) if drop > digits.
                if drop == digits && coef != 0 {
                    let kept = 0u64;
                    let mut c = coef;
                    let mut sticky = false;
                    while c >= 10 {
                        if c % 10 != 0 {
                            sticky = true;
                        }
                        c /= 10;
                    }
                    (kept, c as u32, sticky)
                } else {
                    (0, 0, coef != 0)
                }
            } else {
                let mut c = coef;
                let mut sticky = false;
                let mut round_digit = 0u32;
                for i in 0..drop {
                    let r = (c % 10) as u32;
                    c /= 10;
                    if i == drop - 1 {
                        round_digit = r;
                    } else if r != 0 {
                        sticky = true;
                    }
                }
                (c, round_digit, sticky)
            };

            let mut status = Status::OK;
            if round_digit != 0 || sticky {
                status |= Status::INEXACT;
            }
            let last_lsb = (kept % 10) as u32;
            let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
            let final_coef = if round_up { kept + 1 } else { kept };

            // Renormalise if rounding crossed COEFFICIENT_LIMIT (e.g.
            // 9999999 → 10000000). Per the GDA spec, if the rounded
            // coefficient exceeds PRECISION digits at the target
            // quantum, the operation is INVALID.
            if final_coef >= COEFFICIENT_LIMIT {
                // Could carry into a different cohort, but for
                // quantize the user demanded target_q; we cannot move.
                // The result with the rounded value at target_q would
                // need (PRECISION + 1) digits, which violates the
                // format. Per GDA, INVALID.
                return (Decimal64::NAN, Status::INVALID);
            }

            // The COEFFICIENT_LIMIT check above guarantees final_coef < limit.
            let final_coefficient = crate::bid::Coefficient::try_new(final_coef)
                .expect("final_coef < COEFFICIENT_LIMIT");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign,
                    target_biased_typed,
                    final_coefficient,
                )),
                status,
            );
        }

        // target_q < self_q: pad coef with trailing zeros.
        //
        // H6 fix (Phase 1 Agent 4 Q1, `ddQuantize.decTest` case
        // `ddqua537`): a zero coefficient is representable at any
        // quantum in the format's exponent range, regardless of how
        // many decimal positions of "padding" the target quantum
        // requires. Per IEEE 754-2019 §5.3.2 + GDA `quantize`,
        // validity depends on whether the result fits in the format's
        // representable quantum envelope. `target_biased` was decoded
        // from a `Decimal64` bit pattern, so it is always in the
        // canonical envelope `[0, BIASED_EXP_MAX]`. Short-circuit
        // before the digit-count gate so cases like
        // `quantize(0, 1e-299)` return `0E-299` rather than
        // `(NaN, INVALID)`.
        if coef == 0 {
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign,
                    target_biased_typed,
                    crate::bid::Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        let pad = (self_q - target_q) as u32;
        let new_digits: u32 = decimal_digit_count(coef) + pad;
        if new_digits > PRECISION {
            // Coefficient would not fit in PRECISION digits at the
            // target quantum: INVALID per GDA.
            return (Decimal64::NAN, Status::INVALID);
        }
        if (pad as usize) >= POW10_U64.len() {
            // Cannot pad that much without u64 overflow; INVALID.
            return (Decimal64::NAN, Status::INVALID);
        }
        let new_coef = coef * POW10_U64[pad as usize];
        // new_digits <= PRECISION (checked above), so new_coef < 10^PRECISION = COEFFICIENT_LIMIT.
        let new_coefficient =
            crate::bid::Coefficient::try_new(new_coef).expect("new_coef < COEFFICIENT_LIMIT");
        (
            Decimal64::from_bits(crate::bid::pack_finite(
                sign,
                target_biased_typed,
                new_coefficient,
            )),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.2 `sameQuantum(self, other)`: `true` when the
    /// two operands have the same quantum (encoding) exponent.
    ///
    /// Special-operand rules per the General Decimal Arithmetic spec:
    /// two NaNs (quiet or signaling, any payload) have the same
    /// quantum; two infinities have the same quantum; a special paired
    /// with a finite/zero does not. For finite and zero operands the
    /// quantum is the biased exponent, so cohort matters: `1.0` and
    /// `1.00` differ. Never raises a status flag (no signaling-NaN
    /// trap). Mirrors `Decimal128::same_quantum` so values keep the
    /// same predicate across precisions.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal64::Decimal64;
    ///
    /// let a = Decimal64::try_new(123, -2).unwrap(); // 1.23, q=10^-2
    /// let b = Decimal64::try_new(456, -2).unwrap(); // 4.56, q=10^-2
    /// assert!(a.same_quantum(b));
    ///
    /// let c = Decimal64::try_new(12, -1).unwrap(); // 1.2, q=10^-1
    /// assert!(!a.same_quantum(c));
    /// ```
    #[inline]
    #[must_use]
    pub fn same_quantum(self, other: Self) -> bool {
        use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};
        match (classify_bits(self.0), classify_bits(other.0)) {
            (QuietNaN { .. } | SignalingNaN { .. }, QuietNaN { .. } | SignalingNaN { .. }) => true,
            (Infinity { .. }, Infinity { .. }) => true,
            (
                Zero { biased_exp: a, .. } | Finite { biased_exp: a, .. },
                Zero { biased_exp: b, .. } | Finite { biased_exp: b, .. },
            ) => a == b,
            _ => false,
        }
    }

    /// IEEE 754-2019 §5.3.3 `scaleB(self, n)`: returns `self * 10^n`.
    ///
    /// Equivalent to shifting the unbiased exponent by `n`. NaN
    /// propagates per the §6.2.3 rule; `±∞` passes through unchanged.
    /// GDA constrains the shift to `|n| ≤ 2 × (Emax + precision)`
    /// (800 for decimal64); a larger magnitude is `Invalid_operation`
    /// and returns a quiet NaN. Within range, an out-of-format result
    /// exponent triggers overflow / underflow via the standard
    /// `round_and_pack_finite` path (`±0` and finite values alike).
    #[must_use]
    pub fn scaleb(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        if let Some(special) = scaleb_special_cases(class) {
            return special;
        }
        // Zero or finite: apply the 10^n shift through the standard
        // round-and-pack path. The `|n|` envelope INVALID depends on
        // the runtime `n`, so it lives here, not in the special
        // cases.
        match class {
            Class::Zero { sign, biased_exp } => {
                if n.unsigned_abs() > SCALEB_N_LIMIT {
                    return (Decimal64::NAN, Status::INVALID);
                }
                let q = biased_exp as i32 - BIAS as i32 + n;
                round_and_pack_finite(0, q, q, sign, false, rm, Status::OK)
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                if n.unsigned_abs() > SCALEB_N_LIMIT {
                    return (Decimal64::NAN, Status::INVALID);
                }
                let q = biased_exp as i32 - BIAS as i32 + n;
                round_and_pack_finite(coefficient, q, q, sign, false, rm, Status::OK)
            }
            _ => unreachable!("special cases handled by scaleb_special_cases"),
        }
    }

    /// IEEE 754-2019 §5.3.3 `logB(self)`: integer base-10 logarithm
    /// of `|self|` returned as a `Decimal64` at quantum 0.
    ///
    /// * `±0` → `−∞` + `DIV_BY_ZERO`.
    /// * `±∞` → `+∞`.
    /// * NaN → propagated NaN (sNaN raises INVALID).
    /// * Otherwise: `floor(log10(|self|))` = adjusted exponent
    ///   = `Q(self) + digits(coef) − 1`.
    #[must_use]
    pub fn logb(self) -> (Self, Status) {
        let class = classify_bits(self.0);
        if let Some(special) = logb_special_cases(class) {
            return special;
        }
        // Finite: logb = adjusted exponent = Q + digits(coef) − 1.
        match class {
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let q = biased_exp as i32 - BIAS as i32;
                let adj = q + decimal_digit_count(coefficient) as i32 - 1;
                // `q ∈ [−398, 369]` and `digits(coef) ∈ [1, 16]`, so
                // `adj ∈ [−398, 384]`: a three-digit integer that is
                // always representable as a `Decimal64` at quantum 0.
                // `try_new` therefore cannot fail here, so the old
                // sign-split `unwrap_or(ZERO / NEG_ZERO)` fallbacks
                // were dead arms; collapse to one `expect`.
                (
                    Decimal64::try_new(i64::from(adj), 0)
                        .expect("logb adjusted exponent is a small integer, representable at q=0"),
                    Status::OK,
                )
            }
            _ => unreachable!("special cases handled by logb_special_cases"),
        }
    }

    /// IEEE 754-2019 §5.3.1 `nextUp(self)`: the next representable
    /// `Decimal64` strictly greater than `self`, navigating along
    /// the `Decimal64` number line (signed-zero distinguished, +∞
    /// stays at +∞, −∞ moves to MIN, NaN propagates).
    ///
    /// Returns `(value, Status)`. A signaling-NaN input is quieted
    /// and raises `INVALID`; all other inputs return `Status::OK`.
    /// Per IEEE 754-2019 §5.3.1 the finite-to-+∞ transition does
    /// not raise OVERFLOW.
    #[must_use]
    pub fn next_up(self) -> (Self, Status) {
        if let Some(special) = next_up_special_cases(self) {
            return special;
        }
        // Finite: step one ULP along the Decimal64 number line.
        let (sign, bexp, coef) = match classify_bits(self.0) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!(),
        };
        // Renormalise to the lowest representable cohort: expand the
        // coefficient toward PRECISION digits, bounded by biased_exp
        // = 0 (the subnormal floor). Without this, an in-cohort ±1
        // step at a high-quantum input would skip over numerically
        // adjacent values (`next_up(5)` would return `6`, not the
        // actual ULP `5.000000000000001`).
        let digits = crate::bid::decimal_digit_count(coef);
        let expand = (PRECISION - digits).min(bexp);
        let new_coef = coef * crate::bid::pow10(expand);
        let new_bexp = bexp - expand;
        // new_bexp ≤ bexp ≤ BIASED_EXP_MAX (bexp from classify_bits).
        let new_bexp_typed = crate::bid::BiasedExp::try_from_biased(new_bexp)
            .expect("new_bexp <= bexp from classify_bits");
        if !sign {
            if new_coef + 1 < COEFFICIENT_LIMIT {
                let coef_typed = crate::bid::Coefficient::try_new(new_coef + 1)
                    .expect("checked < COEFFICIENT_LIMIT");
                return (
                    Decimal64::from_bits(crate::bid::pack_finite(
                        false,
                        new_bexp_typed,
                        coef_typed,
                    )),
                    Status::OK,
                );
            }
            if new_bexp == BIASED_EXP_MAX {
                return (Decimal64::INFINITY, Status::OK);
            }
            // new_bexp < BIASED_EXP_MAX checked above, so new_bexp + 1 fits.
            let bumped_bexp = crate::bid::BiasedExp::try_from_biased(new_bexp + 1)
                .expect("new_bexp + 1 <= BIASED_EXP_MAX");
            let pow_coef = crate::bid::Coefficient::try_new(COEFFICIENT_LIMIT / 10)
                .expect("COEFFICIENT_LIMIT / 10 < COEFFICIENT_LIMIT");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(false, bumped_bexp, pow_coef)),
                Status::OK,
            );
        }
        if new_coef == COEFFICIENT_LIMIT / 10 && new_bexp > 0 {
            let dec_bexp =
                crate::bid::BiasedExp::try_from_biased(new_bexp - 1).expect("new_bexp > 0 checked");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    true,
                    dec_bexp,
                    crate::bid::Coefficient::MAX,
                )),
                Status::OK,
            );
        }
        if new_coef > 1 {
            let coef_typed = crate::bid::Coefficient::try_new(new_coef - 1)
                .expect("new_coef - 1 < new_coef < COEFFICIENT_LIMIT");
            return (
                Decimal64::from_bits(crate::bid::pack_finite(true, new_bexp_typed, coef_typed)),
                Status::OK,
            );
        }
        (
            Decimal64::from_bits(crate::bid::pack_finite(
                true,
                crate::bid::BiasedExp::MIN,
                crate::bid::Coefficient::ZERO,
            )),
            Status::OK,
        )
    }

    /// IEEE 754-2019 §5.3.1 `nextDown(self) = −nextUp(−self)`.
    ///
    /// Returns `(value, Status)`. A signaling-NaN input is quieted
    /// and raises `INVALID`; all other inputs return `Status::OK`.
    #[must_use]
    pub fn next_down(self) -> (Self, Status) {
        if let Some(special) = next_down_special_cases(self) {
            return special;
        }
        let (r, s) = self.neg().next_up();
        (r.neg(), s)
    }

    /// The radix of the format: `10`.
    ///
    /// Constant for every decimal format; provided for parity with the
    /// IEEE 754-2019 `radix` operation and the `num_traits` `Float`
    /// surface.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal64::Decimal64;
    ///
    /// assert_eq!(Decimal64::radix(), 10);
    /// ```
    #[inline]
    #[must_use]
    pub const fn radix() -> u32 {
        10
    }

    /// Kani-only entry for the binary `quantize` special-case branch
    /// (NaN propagation and infinity handling) without the
    /// finite-finite rescale arithmetic. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn quantize_special_only_for_kani(self, target: Self) -> Option<(Self, Status)> {
        quantize_special_cases(classify_bits(self.0), classify_bits(target.0))
    }

    /// Kani-only entry for the `scaleb` special-case branch (NaN /
    /// ±∞; the result is independent of `n`). ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn scaleb_special_only_for_kani(self) -> Option<(Self, Status)> {
        scaleb_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `logb` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn logb_special_only_for_kani(self) -> Option<(Self, Status)> {
        logb_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `next_up` special-case branch (NaN /
    /// ±0 / ±∞) without the finite ULP arithmetic. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn next_up_special_only_for_kani(self) -> Option<(Self, Status)> {
        next_up_special_cases(self)
    }

    /// Kani-only entry for the `next_down` special-case branch.
    /// ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn next_down_special_only_for_kani(self) -> Option<(Self, Status)> {
        next_down_special_cases(self)
    }
}

/// Resolve every `quantize` operand combination that does not reach
/// the finite-finite rescale arithmetic. NaN is inspected in the
/// fixed order `[self, target]` (signaling → quiet + INVALID, quiet
/// → quiet + OK), then infinity: `quantize(±∞, ±∞)` preserves the
/// sign of `self`, any other infinity pairing is `NaN + INVALID`.
/// Returns `None` only when both operands are finite or zero, the
/// single case that needs the rescale. Shared by production
/// `quantize` and the Kani shim so the two cannot drift.
fn quantize_special_cases(ca: Class, cb: Class) -> Option<(Decimal64, Status)> {
    if let Class::SignalingNaN { sign, payload } = ca {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let Class::SignalingNaN { sign, payload } = cb {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let Class::QuietNaN { sign, payload } = ca {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let Class::QuietNaN { sign, payload } = cb {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    // Infinity: only well-defined when both are infinities (the
    // result preserving the sign of self).
    if let Class::Infinity { sign } = ca {
        if matches!(cb, Class::Infinity { .. }) {
            return Some((
                Decimal64::from_bits(crate::bid::pack_infinity(sign)),
                Status::OK,
            ));
        }
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(cb, Class::Infinity { .. }) {
        // self finite, target infinity → INVALID.
        return Some((Decimal64::NAN, Status::INVALID));
    }
    None
}

/// Resolve every `scaleb` input class that does not reach the
/// round-and-pack shift. `±∞` passes through unchanged, NaN
/// propagates per §6.2.3. Returns `None` for `Zero` / finite, where
/// the `|n|` envelope and the `10^n` shift apply (both depend on the
/// runtime `n`). Shared by production `scaleb` and the Kani shim.
fn scaleb_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign } => Some((
            Decimal64::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        )),
        Class::Zero { .. } | Class::Finite { .. } => None,
    }
}

/// Resolve every `logb` input class that does not reach the
/// adjusted-exponent computation. `±0 → −∞ + DIV_BY_ZERO`,
/// `±∞ → +∞`, NaN propagates. Returns `None` only for finite
/// non-zero. Shared by production `logb` and the Kani shim.
fn logb_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal64::INFINITY, Status::OK)),
        Class::Zero { .. } => Some((Decimal64::NEG_INFINITY, Status::DIV_BY_ZERO)),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `next_up` input that does not reach the finite ULP
/// arithmetic: signaling NaN is quieted and raises INVALID, quiet
/// NaN passes through with OK, `±0 → MIN_POSITIVE`, `+∞ → +∞`,
/// `−∞ → MIN`. Returns `None` only for a finite non-zero, where the
/// adjacent-value step is computed. Shared by production `next_up`
/// and the Kani shim so the two cannot drift.
fn next_up_special_cases(d: Decimal64) -> Option<(Decimal64, Status)> {
    // L11: decode the bit pattern exactly once. The previous shape
    // chained `is_signaling_nan()` / `is_nan()` / `is_zero()` /
    // `is_infinite()`, each of which re-ran `classify_bits`.
    match classify_bits(d.0) {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        // Quiet NaN passes through unchanged to preserve its payload.
        Class::QuietNaN { .. } => Some((d, Status::OK)),
        Class::Zero { .. } => Some((Decimal64::MIN_POSITIVE, Status::OK)),
        Class::Infinity { sign } => Some((
            if sign {
                Decimal64::MIN
            } else {
                Decimal64::INFINITY
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `next_down` input that does not reach the finite
/// arithmetic. `next_down(d) = −next_up(−d)`, so the special cases
/// are the negated `next_up` specials of `−d`, with the signaling
/// NaN of `d` itself handled first (quieted + INVALID, like
/// `next_up`). Returns `None` only for a finite non-zero. Shared by
/// production `next_down` and the Kani shim.
fn next_down_special_cases(d: Decimal64) -> Option<(Decimal64, Status)> {
    // L11: one decode of `d` for the signaling-NaN check; the
    // `−next_up(−d)` identity then drives the remaining specials
    // (with `next_up`'s own single decode of `−d`).
    if let Class::SignalingNaN { sign, payload } = classify_bits(d.0) {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    next_up_special_cases(d.neg()).map(|(r, s)| (r.neg(), s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BiasedExp, Coefficient};

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn same_quantum_cohort_and_specials() {
        // Same biased exponent ⇒ same quantum, regardless of value.
        assert!(from_int(123, -2).same_quantum(from_int(456, -2)));
        assert!(from_int(0, -2).same_quantum(from_int(999, -2)));
        // Different cohort of the same numeric value.
        assert!(!from_int(10, -1).same_quantum(from_int(100, -2)));
        // Two NaNs share a quantum; NaN vs finite does not.
        assert!(Decimal64::NAN.same_quantum(Decimal64::SIGNALING_NAN));
        assert!(!Decimal64::NAN.same_quantum(from_int(1, 0)));
        // Two infinities share a quantum; ∞ vs finite does not.
        assert!(Decimal64::INFINITY.same_quantum(Decimal64::NEG_INFINITY));
        assert!(!Decimal64::INFINITY.same_quantum(from_int(1, 0)));
        // Never raises a flag (predicate is total) — nothing to assert
        // on status because the API returns a bare `bool` by design.
    }

    #[test]
    fn quantize_h6_zero_at_deep_target_quantum() {
        // H6 regression (Phase 1 Agent 4 Q1, `ddQuantize.decTest`
        // case `ddqua537`): `quantize(0, 1E-299)` should return
        // `0E-299` (zero at the target quantum), not `(NaN, INVALID)`.
        // A zero coefficient is representable at any quantum in the
        // format's exponent envelope; the digit-count gate that
        // applies to non-zero coefficients does not apply.
        //
        // 1E-299 has unbiased quantum -299, well inside the
        // representable range [-398, +369]. try_new(1, -299) is
        // directly constructible.
        let zero_q0 = Decimal64::ZERO;
        let target = Decimal64::try_new(1, -299).unwrap();
        let (r, status) = zero_q0.quantize(target, RoundingMode::NearestEven);
        let expected = Decimal64::try_new(0, -299).unwrap();
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "quantize(0E+0, 1E-299) should equal 0E-299, got {r:?}"
        );
        assert!(status.is_ok(), "quantize zero is exact, got {status:?}");

        // Symmetric: negative zero preserves sign through quantize.
        let (r, _) = Decimal64::NEG_ZERO.quantize(target, RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());

        // Extreme: quantize(0, 1E-398) — quantum at format minimum.
        let target_min = Decimal64::try_new(1, -398).unwrap();
        let (r, status) = Decimal64::ZERO.quantize(target_min, RoundingMode::NearestEven);
        let expected_min = Decimal64::try_new(0, -398).unwrap();
        assert_eq!(r.to_bits(), expected_min.to_bits());
        assert!(status.is_ok());
    }

    #[test]
    fn quantize_pad_with_zeros() {
        // quantize(1, 1E-2) = 1.00 (= 100 × 10^-2)
        let (r, s) = from_int(1, 0).quantize(from_int(1, -2), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 2).unwrap(),
            Coefficient::try_new(100).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn quantize_round_to_target_quantum() {
        // quantize(1.234, 1E-1) = 1.2 (rounded)
        let (r, s) = from_int(1234, -3).quantize(from_int(1, -1), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 1).unwrap(),
            Coefficient::try_new(12).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn quantize_overflow_invalid() {
        // quantize(MAX, 1E0) — the rescaled coefficient would have
        // 97 digits at quantum 0, way over PRECISION = 7. INVALID.
        let (r, s) = Decimal64::MAX.quantize(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn quantize_infinity_passthrough() {
        let (r, s) = Decimal64::INFINITY.quantize(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.is_ok());

        let (r, s) = Decimal64::INFINITY.quantize(from_int(1, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn scaleb_basic() {
        // 1.5 × 10^2 = 150
        let (r, _) = from_int(15, -1).scaleb(2, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS + 1).unwrap(),
            Coefficient::try_new(15).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());

        // 5 × 10^-3 = 0.005
        let (r, _) = from_int(5, 0).scaleb(-3, RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 3).unwrap(),
            Coefficient::try_new(5).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn scaleb_overflow_underflow() {
        let (r, s) = Decimal64::MAX.scaleb(10, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());

        let (r, s) = Decimal64::MIN_POSITIVE.scaleb(-100, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn scaleb_n_envelope_and_no_i32_overflow() {
        // M2 / Agent 4 Q2. GDA envelope for decimal64 is |n| ≤ 800
        // (decTest ddscb120..127). n = ±800 still resolves to a
        // finite extreme; |n| ≥ 801 is Invalid_operation; the
        // i32::MIN / i32::MAX extremes must not panic or wrap.
        let (r, s) = from_int(123, -2).scaleb(800, RoundingMode::NearestEven);
        assert!(r.is_infinite() && s.overflow());

        let (r, s) = from_int(123, -2).scaleb(-800, RoundingMode::NearestEven);
        assert!(r.is_zero() && s.underflow());

        for n in [801, 802, -801, -802, i32::MAX, i32::MIN] {
            let (r, s) = from_int(123, -2).scaleb(n, RoundingMode::NearestEven);
            assert!(r.is_quiet_nan(), "scaleb n={n} is Invalid_operation");
            assert!(s.invalid(), "scaleb n={n} raises INVALID");
        }

        // Zero operand takes the same envelope.
        let (r, s) = Decimal64::ZERO.scaleb(i32::MAX, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && s.invalid());

        // sNaN still raises INVALID regardless of n.
        let (r, s) = Decimal64::SIGNALING_NAN.scaleb(5, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan() && s.invalid());
    }

    #[test]
    fn logb_basic() {
        // logb(1) = 0
        let (r, _) = Decimal64::ONE.logb();
        assert_eq!(r.to_bits(), from_int(0, 0).to_bits());

        // logb(100) = 2
        let (r, _) = from_int(100, 0).logb();
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());

        // logb(0.001) = -3
        let (r, _) = from_int(1, -3).logb();
        assert_eq!(r.to_bits(), from_int(-3, 0).to_bits());
    }

    #[test]
    fn logb_specials() {
        let (r, s) = Decimal64::ZERO.logb();
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, _) = Decimal64::INFINITY.logb();
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, s) = Decimal64::NAN.logb();
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_zero_to_min_positive() {
        let (r, s) = Decimal64::ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.to_bits());
        assert!(s.is_ok());

        let (r, s) = Decimal64::NEG_ZERO.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_finite_renormalises_to_ulp() {
        // Regression: the previous implementation incremented the
        // coefficient in the *stored* cohort, returning `6` for
        // next_up(5). The IEEE 754 spec requires the *adjacent
        // representable value*, which for 5 stored at the maximum
        // cohort is 5 + 10^-15 = 5.000000000000001.
        let (r, s) = from_int(5, 0).next_up();
        // Expected: 5_000_000_000_000_001 × 10^-15 — biased_exp =
        // BIAS - 15 = 383, coefficient = 5_000_000_000_000_001.
        assert_eq!(
            r.to_bits(),
            Decimal64::from_bits(pack_finite(
                false,
                BiasedExp::try_from_biased(BIAS - 15).unwrap(),
                Coefficient::try_new(5_000_000_000_000_001).unwrap()
            ))
            .to_bits()
        );
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_at_infinity() {
        let (r, s) = Decimal64::INFINITY.next_up();
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        // -∞.next_up() = MIN
        let (r, s) = Decimal64::NEG_INFINITY.next_up();
        assert_eq!(r.to_bits(), Decimal64::MIN.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn next_down_basic() {
        // next_down(0) = -MIN_POSITIVE
        let (r, s) = Decimal64::ZERO.next_down();
        assert_eq!(r.to_bits(), Decimal64::MIN_POSITIVE.neg().to_bits());
        assert!(s.is_ok());

        // next_down(MIN_POSITIVE) = 0
        let (r, s) = Decimal64::MIN_POSITIVE.next_down();
        assert!(r.is_zero() && !r.is_sign_negative());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_qnan_propagates() {
        let (r, s) = Decimal64::NAN.next_up();
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn next_up_snan_quiets_and_raises_invalid() {
        // Regression: §5.3.1 says a signaling-NaN input must be
        // quieted *and* raise INVALID. The previous signature `pub
        // fn next_up(self) -> Self` couldn't carry the flag.
        let (r, s) = Decimal64::SIGNALING_NAN.next_up();
        assert!(r.is_quiet_nan());
        assert!(!r.is_signaling_nan());
        assert!(s.invalid());
    }

    #[test]
    fn next_down_snan_quiets_and_raises_invalid() {
        let (r, s) = Decimal64::SIGNALING_NAN.next_down();
        assert!(r.is_quiet_nan());
        assert!(!r.is_signaling_nan());
        assert!(s.invalid());
    }

    #[test]
    fn radix_is_ten() {
        assert_eq!(Decimal64::radix(), 10);
    }
}
