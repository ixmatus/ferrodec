//! IEEE 754 fused multiply-add for [`Decimal128`].
//!
//! `fusedMultiplyAdd(a, b, c)` returns `(a × b) + c` with a single
//! rounding step: the intermediate `a × b` is held exactly as a
//! 226-bit product, aligned against `c` in a 384-bit buffer, combined,
//! and then rounded once to 34 decimal digits.
//!
//! The U384 buffer holds ≈ 115 decimal digits — enough for the
//! 68-digit product shifted up by ≈ 47 places, or the 34-digit addend
//! shifted up by ≈ 81 places. Past that, the smaller operand is
//! strictly sub-ULP at PRECISION precision and the kernel falls to one
//! of two sub-ULP paths, depending on which side dominates:
//!
//! * **`c` dominates** (sub-ULP product, e.g. `1e-6176 × 0.1 + 1`):
//!   route through [`sub_ulp_round`] so status flags reflect the
//!   *final* result's quantum, not the intermediate product's. This
//!   is what avoids the spurious `UNDERFLOW` that the old separated
//!   `mul`-then-`add` formulation reported.
//! * **`a × b` dominates**, with the product within `±MAX`: defer to
//!   `mul`-then-`add`. Addsub already encodes the sign-aware
//!   directed-rounding logic for sub-ULP-effective-sub, and the
//!   product's own status flags carry the same disposition the fma
//!   would.
//! * **`a × b` dominates and would overflow**: skip `mul` (which
//!   clamps to `MIN/MAX` plus its own `OVERFLOW`) and route through
//!   `sub_ulp_round`, which lets `round_and_pack_finite`'s overflow
//!   disposition pick the correctly-rounded `±MAX` / `±Inf`.
//!
//! ## Edges handled
//!
//! * Any NaN operand → NaN; any sNaN raises `INVALID`.
//! * `0 × ∞` (or `∞ × 0`) regardless of `c` → NaN + `INVALID`.
//! * `±∞ × finite_nonzero + ∓∞` → NaN + `INVALID`.
//! * `±∞ × finite_nonzero + finite or ±∞` → `±∞`.
//! * `c == ±∞` and product is finite → `±∞` (resolved before the
//!   zero-product branch so `0 × x + ±∞` doesn't reach the
//!   finite-c handler).
//! * `0 × finite + c` → `c` rebased to the IEEE 754 §6.3 preferred
//!   quantum, with the sign-of-zero rule applied when `c` is itself
//!   zero.
//! * Sign of pure-cancellation zero result follows IEEE 754 §6.3:
//!   `+0` except under `TowardNegative`.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, Class, BIAS, BIASED_EXP_MAX,
    PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::{u256::widening_mul_u128, U256, U384};
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

/// Maximum digit growth we accept inside the U384 alignment buffer
/// before falling to the sub-ULP (sticky-only) path.
///
/// The product `a × b` occupies ≤ 68 decimal digits; shifting it up by
/// `SHIFT_LIMIT` keeps the buffer below `U384`'s ≈ 115-digit envelope
/// with a comfortable margin. The same limit suffices for the
/// `c`-side shift because `c` itself is ≤ 34 digits, much smaller than
/// the product.
const SHIFT_LIMIT: u32 = 47;

impl Decimal128 {
    /// IEEE 754 `fusedMultiplyAdd(self, b, c)`.
    ///
    /// Returns `((self × b) + c, status)` rounded once according to `rm`.
    #[must_use]
    pub fn fma(self, b: Self, c: Self, rm: RoundingMode) -> (Self, Status) {
        if let Some(early) = fma_special_cases(self, b, c, rm) {
            return early;
        }
        fma_finite_kernel(self, b, c, rm)
    }

    /// Kani-only entry point that returns the special-case branch only.
    ///
    /// Symbolic proofs of NaN / Inf / Zero behaviour skip the heavy
    /// alignment + rounding pipeline, mirroring the addsub/mul/div
    /// pattern in [`crate::verify`].
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn fma_special_only_for_kani(
        self,
        b: Self,
        c: Self,
        rm: RoundingMode,
    ) -> Option<(Self, Status)> {
        fma_special_cases(self, b, c, rm)
    }
}

/// Resolve every `(a, b, c)` triple that doesn't reach the
/// finite-finite-finite product/align/round pipeline.
///
/// Returns `None` only when all three operands are finite *and* the
/// product `a × b` is finite-non-zero (i.e. neither `a` nor `b` is zero
/// or infinite). For zero-product inputs the addend `c` is returned at
/// the IEEE 754 §6.3 preferred quantum.
#[inline]
fn fma_special_cases(
    a: Decimal128,
    b: Decimal128,
    c: Decimal128,
    rm: RoundingMode,
) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let cls_c = classify_bits(c.to_bits());

    // sNaN flag: raised even if some other operand also forces NaN.
    let snan = matches!(cls_a, Class::SignalingNaN { .. })
        || matches!(cls_b, Class::SignalingNaN { .. })
        || matches!(cls_c, Class::SignalingNaN { .. });
    let mut status = if snan { Status::INVALID } else { Status::OK };

    // 0 × Inf invalidates the operation regardless of c (per IEEE 754
    // §7.2). NaN-from-product follows the same sNaN rule.
    let a_zero = matches!(cls_a, Class::Zero { .. });
    let b_zero = matches!(cls_b, Class::Zero { .. });
    let a_inf = matches!(cls_a, Class::Infinity { .. });
    let b_inf = matches!(cls_b, Class::Infinity { .. });
    if (a_zero && b_inf) || (a_inf && b_zero) {
        status |= Status::INVALID;
        return Some((Decimal128::NAN, status));
    }

    // Now propagate any NaN operand. Order: a → b → c (matching the
    // separated `mul.then(add)` order, so payload semantics agree with
    // mul + add when we eventually preserve payloads).
    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_c, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((Decimal128::NAN, status));
    }

    // Determine the product's "shape" without rounding it.
    let (sa, _, _) = decompose(cls_a);
    let (sb, _, _) = decompose(cls_b);
    let product_sign = sa ^ sb;
    let product_inf = a_inf || b_inf;
    let product_zero = a_zero || b_zero;

    // Infinite product: combine with c.
    if product_inf {
        match cls_c {
            Class::Infinity { sign: sc } => {
                if sc != product_sign {
                    status |= Status::INVALID;
                    return Some((Decimal128::NAN, status));
                }
                return Some((Decimal128::from_bits(pack_infinity(product_sign)), status));
            }
            // Finite c (zero or non-zero) is dominated by ±Inf.
            _ => {
                return Some((Decimal128::from_bits(pack_infinity(product_sign)), status));
            }
        }
    }

    // c is ±Inf with a finite (zero or non-zero) product → c.
    // Resolve this *before* the zero-product branch so `0 × x + ±Inf`
    // returns `±Inf`, not a panic in the zero+c handler.
    if let Class::Infinity { sign } = cls_c {
        return Some((Decimal128::from_bits(pack_infinity(sign)), status));
    }

    // Zero product: result is just c rebased to preferred quantum,
    // with the sign rule for `(±0) + c`.
    if product_zero {
        let qab = product_quantum(cls_a, cls_b);
        return Some(combine_zero_product_with_c(
            product_sign,
            qab,
            cls_c,
            rm,
            status,
        ));
    }

    // Finite product, c is ±0 → emit ab rounded with q_pref = min(qab, qc).
    if let Class::Zero {
        sign: sc,
        biased_exp: ec,
    } = cls_c
    {
        let (_, ea, ca) = decompose(cls_a);
        let (_, eb, cb) = decompose(cls_b);
        let qab = (ea as i32 - BIAS as i32) + (eb as i32 - BIAS as i32);
        let qc = ec as i32 - BIAS as i32;
        let q_pref = qab.min(qc);
        let (hi, lo) = widening_mul_u128(ca, cb);
        let coef = U256 { lo, hi };
        // Sign rule for `nonzero_ab + (±0)`: the non-zero summand's sign
        // wins, so `sc` is irrelevant here.
        let _ = sc;
        return Some(round_and_pack_finite(
            coef,
            qab,
            q_pref,
            product_sign,
            false,
            rm,
            status,
        ));
    }

    // All three finite-non-zero — fall through to the full kernel.
    None
}

/// Exact preferred-quantum exponent of the (possibly zero) product, as
/// `q_a + q_b`.
fn product_quantum(cls_a: Class, cls_b: Class) -> i32 {
    let (_, ea, _) = decompose(cls_a);
    let (_, eb, _) = decompose(cls_b);
    (ea as i32 - BIAS as i32) + (eb as i32 - BIAS as i32)
}

/// Result of `(±0) + c` for fma's zero-product branch.
///
/// When `c` is finite-nonzero the answer is `c` re-emitted with the
/// preferred quantum `min(q_ab, q_c)` (clamped to representability).
/// When `c` is zero we apply the IEEE 754 sign-of-zero rule for two
/// zeros.
fn combine_zero_product_with_c(
    product_sign: bool,
    qab: i32,
    cls_c: Class,
    rm: RoundingMode,
    status: Status,
) -> (Decimal128, Status) {
    match cls_c {
        Class::Zero {
            sign: sc,
            biased_exp: ec,
        } => {
            let qc = ec as i32 - BIAS as i32;
            let target = qab.min(qc);
            let target_clamped = target.clamp(-(BIAS as i32), BIASED_EXP_MAX as i32 - BIAS as i32);
            let result_sign = if product_sign == sc {
                product_sign
            } else {
                rm == RoundingMode::TowardNegative
            };
            (
                Decimal128::from_bits(pack_finite(
                    result_sign,
                    (target_clamped + BIAS as i32) as u32,
                    0,
                )),
                status,
            )
        }
        Class::Finite {
            sign: sc,
            biased_exp: ec,
            coefficient: cc,
        } => {
            // Re-emit c at quantum min(qab, qc) when possible.
            let qc = ec as i32 - BIAS as i32;
            let target = qab.min(qc);
            // Use the standard rounding pipeline: c at its current
            // (cohort, quantum) with q_pref = target. round_and_pack
            // will shift down toward target if the coefficient has
            // headroom.
            let coef = U256::from_u128(cc);
            (
                round_and_pack_finite(coef, qc, target, sc, false, rm, status).0,
                status,
            )
        }
        _ => {
            debug_assert!(false, "combine_zero_product_with_c on non-finite c");
            (Decimal128::NAN, status)
        }
    }
}

/// Single-rounding kernel for the finite-finite-finite case where the
/// product `a × b` is itself finite and non-zero.
///
/// Layout:
/// 1. `cab = ca · cb` exactly in U256 (≤ 226 bits, ≤ 68 digits).
/// 2. Determine target quantum `q_target = min(q_ab, q_c)`.
/// 3. Align `cab` and `cc` into a U384 buffer at `q_target`. If the
///    shift would overflow the buffer, the smaller operand is sub-ULP
///    relative to the larger and is collapsed to a sticky bit
///    (handled in [`fma_sub_ulp`]).
/// 4. Combine (effective add or sub).
/// 5. Shift the U384 sum right until it fits in U256 (sticky bit
///    accumulates any non-zero dropped digit).
/// 6. Hand off to [`round_and_pack_finite`] with `q_preferred =
///    q_target`.
fn fma_finite_kernel(
    a: Decimal128,
    b: Decimal128,
    c: Decimal128,
    rm: RoundingMode,
) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let cls_c = classify_bits(c.to_bits());
    let (sa, ea, ca) = decompose(cls_a);
    let (sb, eb, cb) = decompose(cls_b);
    let (sc, ec, cc) = decompose(cls_c);
    debug_assert!(ca != 0 && cb != 0 && cc != 0);

    let sab = sa ^ sb;
    let qab = (ea as i32 - BIAS as i32) + (eb as i32 - BIAS as i32);
    let qc = ec as i32 - BIAS as i32;

    // Exact 226-bit product.
    let (hi, lo) = widening_mul_u128(ca, cb);
    let cab = U256 { lo, hi };

    let target = qab.min(qc);
    let q_pref = target;
    let shift_ab = (qab - target) as u32;
    let shift_c = (qc - target) as u32;
    let effective_sub = sab != sc;

    // Bound check: would aligning into U384 overflow the buffer? The
    // shifted operand grows by `shift_*` decimal digits.
    let cab_grown_digits = cab.decimal_digit_count() + shift_ab;
    let cc_grown_digits = decimal_digit_count(cc) + shift_c;

    // U384 ≈ 115 digits; we leave headroom for the add to carry into a
    // 116th digit. SHIFT_LIMIT keeps both side comfortably below.
    let ab_too_wide = shift_ab > SHIFT_LIMIT || cab_grown_digits > 110;
    let c_too_wide = shift_c > SHIFT_LIMIT.saturating_add(35) || cc_grown_digits > 110;

    if ab_too_wide || c_too_wide {
        let _ = effective_sub;
        return fma_sub_ulp(
            a,
            b,
            c,
            cab,
            qab,
            sab,
            cc,
            qc,
            sc,
            rm,
            ab_too_wide,
            c_too_wide,
        );
    }

    // Common path: both shifts fit. Promote into U384.
    let buf_ab = U384::from_u256(cab).mul_pow10(shift_ab);
    let buf_c = U384::from_u128(cc).mul_pow10(shift_c);

    let (combined, sign_out) = if effective_sub {
        match buf_ab.cmp(buf_c) {
            core::cmp::Ordering::Greater => (buf_ab.sub(buf_c), sab),
            core::cmp::Ordering::Less => (buf_c.sub(buf_ab), sc),
            core::cmp::Ordering::Equal => {
                return zero_after_cancellation(rm, Status::OK, target);
            }
        }
    } else {
        (buf_ab.add(buf_c), sab)
    };

    if combined.is_zero() {
        return zero_after_cancellation(rm, Status::OK, target);
    }

    // Shift the U384 down to a U256 with sticky tracking, then round.
    let (residue, exp_shift, sticky) = combined.shift_right_to_u256(false);
    let target_after = target + exp_shift as i32;

    round_and_pack_finite(
        residue,
        target_after,
        q_pref,
        sign_out,
        sticky,
        rm,
        Status::OK,
    )
}

/// Sub-ULP path: the smaller summand is below 1 ULP of the larger at
/// PRECISION precision, so only its sticky contribution matters.
/// Three sub-cases:
///
/// * **`c_too_wide`** — `c × 10^shift_c` overflows the buffer, so `c`
///   dominates and `a × b` is sub-ULP. The product may be subnormal
///   (e.g. `1e-6176 × 0.1 + 1`) which would make `mul`-then-`add`
///   raise a spurious `UNDERFLOW`. Single-round through
///   [`sub_ulp_round`] to skip `mul`'s exception ladder; status is
///   driven by the *final* result's quantum.
/// * **`ab_too_wide` with overflowing product** — `cab × 10^qab` is
///   beyond `±MAX`. `mul`'s overflow disposition would clamp to
///   `MIN`/`MAX` and `add(MIN, sub-ULP_c)` would then walk one ULP
///   off; route through [`sub_ulp_round`] so
///   [`round_and_pack_finite`]'s overflow disposition picks the
///   correctly-rounded `±MAX` / `±Inf`.
/// * **`ab_too_wide` with in-range product** — defer to legacy
///   `mul`-then-`add`. The product's magnitude *is* the result's
///   magnitude, and addsub's `sub_ulp_effective_sub` already encodes
///   the directed-rounding lower/upper-candidate decision; replicating
///   that logic here would be duplicate code.
#[allow(clippy::too_many_arguments)]
fn fma_sub_ulp(
    a: Decimal128,
    b: Decimal128,
    c: Decimal128,
    cab: U256,
    qab: i32,
    sab: bool,
    cc: u128,
    qc: i32,
    sc: bool,
    rm: RoundingMode,
    ab_too_wide: bool,
    c_too_wide: bool,
) -> (Decimal128, Status) {
    debug_assert!(
        ab_too_wide ^ c_too_wide,
        "exactly one side may be sub-ULP at this point"
    );
    if c_too_wide {
        let _ = (a, b, c, qab, sab, cab);
        return sub_ulp_round(U256::from_u128(cc), qc, sc, false, rm);
    }

    // ab dominates. Two sub-cases:
    //
    // * Product magnitude exceeds `MAX` — go through `sub_ulp_round`,
    //   which routes through `round_and_pack_finite`'s own overflow
    //   disposition. The legacy `mul`-then-`add` formulation produces
    //   `-MAX + 1 ULP` here instead: `mul` clamps the overflow to
    //   `MIN`, then `add(MIN, sub-ULP_c)` shifts it by one position.
    // * Product magnitude in normal range — defer to legacy
    //   `mul`-then-`add`. This lets `addsub`'s `sub_ulp_effective_sub`
    //   pick the correct lower/upper candidate under directed
    //   rounding (the very logic this kernel would otherwise have to
    //   re-implement).
    let cab_digits = cab.decimal_digit_count() as i32;
    let cab_top_exp = cab_digits + qab - 1; // log10(magnitude), roughly
    if cab_top_exp > crate::bid::E_MAX {
        let _ = (a, b, c, cc);
        return sub_ulp_round(cab, qab, sab, false, rm);
    }

    let _ = (cab, qab, sab, cc, qc, sc);
    let (product, st1) = a.mul(b, rm);
    let (sum, st2) = product.add(c, rm);
    (sum, st1 | st2)
}

/// Round the dominant summand `dom` when the other summand is strictly
/// sub-ULP at `q_dom`'s precision. The sub-ULP residue's contribution
/// is encoded as a sticky bit; `round_and_pack_finite` then applies
/// the IEEE 754 rounding mode (sign-aware for directed modes).
fn sub_ulp_round(
    dom: U256,
    q_dom: i32,
    sign_dom: bool,
    other_is_zero: bool,
    rm: RoundingMode,
) -> (Decimal128, Status) {
    if other_is_zero {
        return round_and_pack_finite(dom, q_dom, q_dom, sign_dom, false, rm, Status::OK);
    }

    // Pad the dominant coefficient to PRECISION digits so the sticky
    // bit is anchored at the correct ULP position (one digit below the
    // padded LSB). Without padding, `round_and_pack_finite` would not
    // descend into the round-with-sticky pipeline because there's no
    // "excess digits to drop"; its sticky-aware bumping only fires
    // when `digits ≥ PRECISION`. The pad multiplications by 10 are
    // exact and preserve the value.
    let digits = dom.decimal_digit_count();
    let pad = if digits < PRECISION {
        PRECISION - digits
    } else {
        0
    };
    let padded = if pad > 0 { dom.mul_pow10(pad) } else { dom };
    let q_padded = q_dom - pad as i32;

    round_and_pack_finite(padded, q_padded, q_padded, sign_dom, true, rm, Status::OK)
}

fn zero_after_cancellation(
    rm: RoundingMode,
    status: Status,
    target_unbiased_exp: i32,
) -> (Decimal128, Status) {
    let sign = rm == RoundingMode::TowardNegative;
    let biased_exp = (target_unbiased_exp + BIAS as i32).clamp(0, BIASED_EXP_MAX as i32) as u32;
    (
        Decimal128::from_bits(pack_finite(sign, biased_exp, 0)),
        status,
    )
}

/// Decompose a finite or zero `Class` into `(sign, biased_exp, coefficient)`.
fn decompose(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        Class::Infinity { sign } => (sign, BIAS, 0),
        _ => {
            debug_assert!(false, "fma::decompose called on NaN");
            (false, BIAS, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn d_int(c: i128) -> Decimal128 {
        if c == 0 {
            return Decimal128::ZERO;
        }
        let sign = c < 0;
        let coef = c.unsigned_abs();
        Decimal128::from_bits(pack_finite(sign, BIAS, coef))
    }

    #[test]
    fn nan_propagates() {
        let (r, _) = Decimal128::ONE.fma(Decimal128::NAN, Decimal128::ONE, RoundingMode::default());
        assert!(r.is_nan());

        let (r, s) = Decimal128::SIGNALING_NAN.fma(
            Decimal128::ONE,
            Decimal128::ONE,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());

        let (r, s) = Decimal128::ONE.fma(
            Decimal128::ONE,
            Decimal128::SIGNALING_NAN,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn zero_times_inf_is_invalid_nan() {
        let (r, s) = Decimal128::ZERO.fma(
            Decimal128::INFINITY,
            Decimal128::ONE,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_basic() {
        // 2 * 3 + 4 = 10
        let (r, _) = d_int(2).fma(d_int(3), d_int(4), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(10));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 5 * (-7) + 100 = 65
        let (r, _) = d_int(5).fma(d_int(-7), d_int(100), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(65));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn fma_with_zero_addend() {
        let (r, _) = d_int(7).fma(d_int(11), Decimal128::ZERO, RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(77));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn fma_with_one_multiplier() {
        for &v in &[1i128, -1, 7, -42] {
            let (r, _) = Decimal128::ONE.fma(d_int(v), Decimal128::ZERO, RoundingMode::default());
            let (cmp, _) = r.partial_cmp(d_int(v));
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        }
    }
}
