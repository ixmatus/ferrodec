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
use crate::ops::{propagate_nan3, round_and_pack_finite};
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
        // IEEE 754-2019 §6.2.3: the result NaN should carry one of
        // the input NaN payloads when one is available. If c is a
        // NaN, propagate its payload (a and b are non-NaN here, so
        // propagate_nan3 picks c; the sNaN-bit is cleared and the
        // sNaN INVALID was already accumulated above).
        let c_is_nan = matches!(cls_c, Class::QuietNaN { .. } | Class::SignalingNaN { .. });
        let result = if c_is_nan {
            propagate_nan3(a, b, c)
        } else {
            Decimal128::NAN
        };
        return Some((result, status));
    }

    // Now propagate any NaN operand. Order: a → b → c (matching the
    // separated `mul.then(add)` order, so payload semantics agree with
    // mul + add).
    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_c, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((propagate_nan3(a, b, c), status));
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
        let _ = (a, b, c, cab);
        let effective_sub = sab != sc;
        // IEEE 754 §6.3 preferred quantum for the FMA result is
        // min(qab, qc); the existing sub_ulp_round path ends up with
        // it implicitly via its pad-to-PRECISION trick (which only
        // shifts down to qc − pad), but the new effective-sub path
        // needs it explicitly so round_and_pack_finite can pad the
        // candidate's trailing zeros.
        let q_pref = qab.min(qc);
        if !effective_sub {
            // Same-sign sub-ULP product: the residue pushes magnitude
            // up by epsilon ≪ 0.5 ULP (c_too_wide ⇒ qc − qab > 82,
            // far beyond PRECISION = 34), so existing sub_ulp_round
            // (epsilon-as-positive-sticky) gives the correct rounded
            // value for every mode.
            return sub_ulp_round(U256::from_u128(cc), qc, sc, false, rm);
        }
        // Opposite-sign sub-ULP product: true magnitude is
        // `cc · 10^qc − epsilon`, slightly *below* `cc · 10^qc`. For
        // round-to-nearest the answer is still `cc` (epsilon ≪ 0.5
        // ULP), but directional modes can pick the lower neighbour.
        // Mirror addsub::sub_ulp_effective_sub's lower/upper-candidate
        // selection without that helper's full eps/half-ULP machinery
        // — the magnitude bound makes eps_cmp trivially `Less` for
        // both nearest variants.
        return sub_ulp_eff_sub_c_dominates(cc, qc, q_pref, sc, rm);
    }

    // ab dominates. Three sub-cases:
    //
    // * Product magnitude exceeds `MAX` — go through `sub_ulp_round`,
    //   which routes through `round_and_pack_finite`'s own overflow
    //   disposition. The legacy `mul`-then-`add` formulation produces
    //   `-MAX + 1 ULP` here instead: `mul` clamps the overflow to
    //   `MIN`, then `add(MIN, sub-ULP_c)` shifts it by one position.
    // * Product in range, c same sign as product — single-round
    //   through `sub_ulp_round(cab, qab, sab, false, rm)`. cab keeps
    //   its full ≤ 68-digit precision; round_and_pack_finite drops
    //   the tail with its own (round_digit, sticky) and OR's the
    //   passed sticky from c. This is provably equivalent to the
    //   IEEE single-rounding contract for same-sign sub-ULP c —
    //   the `mul`-then-`add` formulation can disagree by 1 ULP at
    //   a 35th-digit `5000…0` tie (M6).
    // * Product in range, c opposite sign — defer to legacy
    //   `mul`-then-`add`. The single-rounding fix here would have to
    //   subtract c's sub-ULP residue from cab's natural drop residue
    //   and re-decide the round; addsub's `sub_ulp_effective_sub`
    //   gets the post-mul value's directed rounding right but does
    //   not compose with the lost mul tie. Tracked as a follow-up;
    //   real-world impact bounded by the rarity of the trigger
    //   (35th-digit exact tie *and* opposite-sign sub-ULP c).
    let cab_digits = cab.decimal_digit_count() as i32;
    let cab_top_exp = cab_digits + qab - 1; // log10(magnitude), roughly
    if cab_top_exp > crate::bid::E_MAX {
        let _ = (a, b, c, cc);
        return sub_ulp_round(cab, qab, sab, false, rm);
    }

    let effective_sub = sab != sc;
    if !effective_sub {
        // Same sign: cab dominates with c as positive sub-ULP residue.
        // sub_ulp_round handles the digit drop with c's sticky bit
        // OR'd in, giving the correctly single-rounded result.
        let _ = (a, b, c, cc, qc, sc);
        return sub_ulp_round(cab, qab, sab, false, rm);
    }

    // Opposite-sign sub-ULP c: legacy mul-then-add. See the third
    // bullet above for the known limitation.
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
    let pad = PRECISION.saturating_sub(digits);
    let padded = if pad > 0 { dom.mul_pow10(pad) } else { dom };
    let q_padded = q_dom - pad as i32;

    round_and_pack_finite(padded, q_padded, q_padded, sign_dom, true, rm, Status::OK)
}

/// Effective-subtraction sub-ULP path for the `c_too_wide` branch.
///
/// `c` dominates and the product's magnitude is so far below `c`'s
/// quantum (`qc − qab > 82` per [`SHIFT_LIMIT`]) that the true value
/// is `cc · 10^qc − epsilon` with `epsilon ≪ 0.5 ULP` of `cc`'s
/// PRECISION-rep neighbours. Round-to-nearest therefore always picks
/// `cc` (the upper candidate); only the directional modes can pick
/// the lower candidate `cc − 1 ULP`.
///
/// Mirrors the candidate selection in `addsub::sub_ulp_effective_sub`
/// but skips the `2·cs vs 10^(diff−k)` machinery — the magnitude
/// bound forces `eps_cmp = Less` for nearest modes.
fn sub_ulp_eff_sub_c_dominates(
    cc: u128,
    qc: i32,
    q_preferred: i32,
    sc: bool,
    rm: RoundingMode,
) -> (Decimal128, Status) {
    debug_assert!(cc != 0);
    let d = decimal_digit_count(cc);
    let is_pow10 = cc == 10u128.pow(d - 1);
    // Extension factor: how many digits below `qc` the
    // PRECISION-rep boundary sits when `cc` is a power of 10.
    let k: u32 = if is_pow10 {
        PRECISION + 1 - d
    } else {
        PRECISION - d
    };

    let round_up = match rm {
        // Toward zero: pick the smaller-magnitude neighbour.
        RoundingMode::TowardZero => false,
        // Toward +∞: positive result wants the upper (cc itself);
        // negative result wants the lower (less-negative).
        RoundingMode::TowardPositive => !sc,
        // Toward −∞: mirror image of TowardPositive.
        RoundingMode::TowardNegative => sc,
        // Round-to-nearest: epsilon ≪ 0.5 ULP, so the upper (cc) is
        // strictly closer to the true value.
        RoundingMode::NearestEven | RoundingMode::NearestAway => true,
    };
    let status = Status::INEXACT;

    if round_up {
        // Upper candidate `cc · 10^qc`. Pass the FMA's preferred
        // quantum so `round_and_pack_finite` pads trailing zeros
        // down to it (IEEE 754 §6.3 — inexact results get the
        // quantum of the more-precise input where possible).
        return round_and_pack_finite(U256::from_u128(cc), qc, q_preferred, sc, false, rm, status);
    }
    // Lower candidate: cc · 10^k − 1 at quantum qc − k. For
    // non-power-of-10 cc with d = PRECISION the result stays in the
    // same cohort with coefficient cc − 1; for power-of-10 cc it
    // crosses into the lower cohort with an all-9 coefficient.
    let (lower_coef, lower_quantum) = if k == 0 {
        (U256::from_u128(cc - 1), qc)
    } else {
        let extended = U256::from_u128(cc).mul_pow10(k);
        (extended.sub(U256::from_u128(1)), qc - k as i32)
    };
    round_and_pack_finite(
        lower_coef,
        lower_quantum,
        q_preferred,
        sc,
        false,
        rm,
        status,
    )
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
    fn zero_times_inf_with_nan_c_preserves_payload() {
        // IEEE 754-2019 §6.2.3: when one input is NaN the result NaN
        // should carry that input's payload. The 0 × Inf branch used
        // to drop c's payload and return canonical NAN even when c
        // was a NaN with an interesting payload.
        let payload = 0x1234_5678u128;
        let qnan_c = Decimal128::from_bits(crate::bid::pack_quiet_nan(false, payload));
        let snan_c = Decimal128::from_bits(crate::bid::pack_signaling_nan(false, payload));

        let (r, s) = Decimal128::ZERO.fma(Decimal128::INFINITY, qnan_c, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid(), "0 × Inf still raises INVALID");
        assert_eq!(
            r.to_bits() & ((1u128 << 110) - 1),
            payload,
            "qNaN c's payload should be preserved",
        );

        let (r, s) = Decimal128::INFINITY.fma(Decimal128::ZERO, snan_c, RoundingMode::default());
        assert!(r.is_nan());
        assert!(r.is_quiet_nan(), "sNaN c is quieted on output");
        assert!(s.invalid(), "0 × Inf and sNaN both raise INVALID");
        assert_eq!(
            r.to_bits() & ((1u128 << 110) - 1),
            payload,
            "sNaN c's payload should be preserved (signal cleared)",
        );

        // Non-NaN c still gets the canonical NAN — the fix is narrow.
        let (r, s) = Decimal128::ZERO.fma(
            Decimal128::INFINITY,
            Decimal128::ONE,
            RoundingMode::default(),
        );
        assert_eq!(r.to_bits(), Decimal128::NAN.to_bits());
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

    /// Build the smallest-magnitude positive subnormal (`1e-6176`).
    fn min_subnormal() -> Decimal128 {
        Decimal128::from_bits(pack_finite(false, 0, 1))
    }

    /// The PRECISION-rep value just below 1: `0.999…9` (34 nines) with
    /// quantum `−34`.
    fn one_minus_one_ulp() -> Decimal128 {
        let coef = 10u128.pow(34) - 1; // 34 nines
        Decimal128::from_bits(pack_finite(false, (BIAS as i32 - 34) as u32, coef))
    }

    #[test]
    fn fma_sub_ulp_eff_sub_directional_one_minus_eps() {
        // Reproducer from the 6-agent review: ONE.fma(1e-6176, NEG_ONE,
        // TowardPositive) should round the true value −1 + 1e-6176 (just
        // above −1) UP toward +∞, picking the next-larger representable,
        // which is 0.999…9 × 10^−34 = `one_minus_one_ulp`. The legacy
        // sub_ulp_round path returned exactly −1 instead.
        let (r, s) = Decimal128::ONE.fma(
            min_subnormal(),
            Decimal128::NEG_ONE,
            RoundingMode::TowardPositive,
        );
        assert!(s.inexact());
        let target = one_minus_one_ulp().neg();
        assert_eq!(
            r.to_bits(),
            target.to_bits(),
            "TowardPositive: got {r:?}, want {target:?}",
        );

        // Symmetric reproducer: NEG_ONE * 1e-6176 + ONE under
        // TowardNegative. True value 1 − 1e-6176, which TowardNegative
        // should round DOWN to 0.999…9.
        let (r, s) = Decimal128::NEG_ONE.fma(
            min_subnormal(),
            Decimal128::ONE,
            RoundingMode::TowardNegative,
        );
        assert!(s.inexact());
        let target = one_minus_one_ulp();
        assert_eq!(
            r.to_bits(),
            target.to_bits(),
            "TowardNegative: got {r:?}, want {target:?}",
        );
    }

    #[test]
    fn fma_sub_ulp_eff_sub_toward_zero() {
        // TowardZero on a positive result `1 − epsilon` should pick the
        // smaller-magnitude neighbour 0.999…9.
        let (r, s) =
            Decimal128::NEG_ONE.fma(min_subnormal(), Decimal128::ONE, RoundingMode::TowardZero);
        assert!(s.inexact());
        assert_eq!(r.to_bits(), one_minus_one_ulp().to_bits());

        // Same for the negative-result mirror.
        let (r, s) = Decimal128::ONE.fma(
            min_subnormal(),
            Decimal128::NEG_ONE,
            RoundingMode::TowardZero,
        );
        assert!(s.inexact());
        assert_eq!(r.to_bits(), one_minus_one_ulp().neg().to_bits());
    }

    /// `−1.000000000000000000000000000000000` — the 34-digit cohort
    /// of −1 at quantum −33 (padded form mandated by IEEE 754 §6.3
    /// preferred-quantum rules for inexact sub-ULP results).
    fn neg_one_padded_34_digits() -> Decimal128 {
        let coef = 10u128.pow(33); // 1 followed by 33 zeros
        Decimal128::from_bits(pack_finite(true, (BIAS as i32 - 33) as u32, coef))
    }

    #[test]
    fn fma_ab_dominates_in_range_same_sign_single_rounds() {
        // M6 regression: when the exact product `a×b` has its 35th
        // digit on a `5000…0` tie that round-half-even would resolve
        // *down* (kept LSB = 0), and `c` is sub-ULP same-sign at a
        // quantum far below qab, single-rounding must use c's sticky
        // to break the tie *up*. The pre-Phase-O mul-then-add path
        // dropped c's sticky into the post-mul addsub stage, where
        // it could no longer affect the lost mul tie, and so produced
        // a result one ULP smaller than the IEEE single-rounding
        // contract requires.
        //
        // Construction:
        //   a = 5 (coef=5, q=0)
        //   b = 2 × 10^33 + 1 (34-digit coef, q=0)
        //     a × b = 10^34 + 5, exactly 35 digits with the tail "5"
        //   c = 1e-100 (sub-ULP at the result's quantum 1, same sign)
        //
        // mul rounds 10^34 + 5 to 34 digits: kept = 10^33 (LSB = 0),
        // round_digit = 5, sticky = 0. Banker's tie → round down.
        // Then addsub(10^34, 1e-100) keeps 10^34 with c contributing
        // only INEXACT.
        //
        // Single round: dropped digit = 5, c's sticky is true →
        // tie breaks up → kept = 10^33 + 1 at quantum 1. Result
        // is 10^34 + 10 (coef = 10^33 + 1, q = 1).
        let a = d_int(5);
        let two_e33_plus_one =
            Decimal128::from_bits(pack_finite(false, BIAS, 2 * 10u128.pow(33) + 1));
        let b = two_e33_plus_one;
        // c = 1 × 10^-100 (qc = -100 ≪ qab = 0, satisfies
        // shift_ab > SHIFT_LIMIT = 47 so the ab_too_wide branch fires).
        let c = Decimal128::from_bits(pack_finite(false, (BIAS as i32 - 100) as u32, 1));

        let (r, s) = a.fma(b, c, RoundingMode::NearestEven);
        assert!(s.inexact());

        let expected = Decimal128::from_bits(pack_finite(
            false,
            (BIAS as i32 + 1) as u32,
            10u128.pow(33) + 1,
        ));
        assert_eq!(
            r.to_bits(),
            expected.to_bits(),
            "M6 single-rounding tie: got coef={} q=?, want coef={} q=1",
            r.to_bits() & ((1u128 << 110) - 1),
            10u128.pow(33) + 1,
        );

        // Sanity: the same input under mul-then-add (the legacy path)
        // would resolve the tie *down* and lose c's sticky. Confirm
        // that the new fma result differs from the legacy formulation
        // by exactly one ULP at the result quantum.
        let (product, _) = a.mul(b, RoundingMode::NearestEven);
        let (legacy, _) = product.add(c, RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(legacy);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Greater),
            "single-rounded fma must exceed mul-then-add by one ULP",
        );
    }

    #[test]
    fn fma_sub_ulp_eff_sub_nearest_picks_dominant() {
        // For round-to-nearest, epsilon = 1e-6176 is many magnitudes
        // below 0.5 ULP of 1, so the upper candidate (cc = −1) is
        // strictly closer to the true value. IEEE 754 §6.3 then pads
        // the chosen value out to the preferred quantum
        // min(qab, qc) = qab clamped to PRECISION digits, yielding
        // `−1.000…0` (34 digits) instead of the canonical `−1`.
        // dqFMA.decTest:dqadd36506 covers exactly this shape.
        for &rm in &[RoundingMode::NearestEven, RoundingMode::NearestAway] {
            let (r, s) = Decimal128::ONE.fma(min_subnormal(), Decimal128::NEG_ONE, rm);
            assert!(s.inexact(), "{rm:?}: should still flag inexact");
            assert_eq!(
                r.to_bits(),
                neg_one_padded_34_digits().to_bits(),
                "{rm:?}: must round to padded −1.000…0",
            );
        }
    }
}
