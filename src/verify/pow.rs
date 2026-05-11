//! Kani harnesses for `Decimal128::pow` special-value rules.
//!
//! ### Strategy
//!
//! `pow`'s body unconditionally references `ln_extended` and the
//! `exp(y · ln(x))` extended-precision pipeline whenever the input
//! pair doesn't hit a special-case rule. CBMC would have to walk
//! that pipeline symbolically for any "any-`u128`" operand, which
//! blows the budget.
//!
//! Same mitigation as `addsub.rs`: route through the
//! `pow_special_only_for_kani` shim, which evaluates IEEE 754-2019
//! §9.2.1 rules 1–7 in closed form and returns `None` for the rule-8
//! general path. The harness asserts on the rules; general-path
//! inputs in the operand pool are silently skipped (the shim returns
//! `None`). General-case faithful-rounding correctness lives in the
//! `tests/property_pow.rs` astro-float oracle.
//!
//! Concrete claims proven here:
//!
//! * `pow(±1, ±∞) = 1` for all four sign combinations and all five
//!   rounding modes (the H1 reproducer from the May 2026 6-agent
//!   correctness review).
//! * `pow(x, ±0) = 1` for any `x` in the special-input pool except
//!   sNaN. ferrodec deliberately raises `INVALID` for sNaN here
//!   (acknowledged spec deviation; see `pow.rs` rule 1 comment).
//! * Every special-rule combination in the input pool terminates with
//!   a defined `Some` answer through the shim; no rule dispatcher
//!   regression slips an `unreachable!()` past the closed-form table.
//!
//! General-case faithful-rounding correctness is proptest-tested
//! in `tests/property_pow.rs`; finite-finite Kani symbolic execution
//! is intentionally out of scope (per ADR-0015).

#![cfg(feature = "pow")]

use crate::status::RoundingMode;
use crate::Decimal128;

const NUM_OPERANDS: u8 = 11;

fn operand(idx: u8) -> Decimal128 {
    match idx {
        0 => Decimal128::NAN,
        1 => Decimal128::SIGNALING_NAN,
        2 => Decimal128::INFINITY,
        3 => Decimal128::NEG_INFINITY,
        4 => Decimal128::ZERO,
        5 => Decimal128::NEG_ZERO,
        6 => Decimal128::ONE,
        7 => Decimal128::NEG_ONE,
        8 => Decimal128::MAX,
        9 => Decimal128::MIN,
        _ => Decimal128::from_i32(2),
    }
}

fn rm_from_u8(x: u8) -> RoundingMode {
    match x {
        0 => RoundingMode::NearestEven,
        1 => RoundingMode::NearestAway,
        2 => RoundingMode::TowardZero,
        3 => RoundingMode::TowardPositive,
        _ => RoundingMode::TowardNegative,
    }
}

/// `pow(±1, ±∞) = 1` per IEEE 754-2019 §9.2.1 rule-5 sub-case.
///
/// The H1 finding in the 6-agent review found that the negative-base
/// case (`pow(-1, ±∞)`) hit `unreachable!()` because rule 2's
/// short-circuit only matched `x = +1` and rule 6's |x|=1 arm assumed
/// rule 2 had already covered it. This harness pins the contract via
/// the closed-form shim — the broken path would still surface here
/// because rule 6 lives in `pow_special_cases`.
#[kani::proof]
fn pow_pm_one_pm_infinity_is_one() {
    let xi: u8 = kani::any();
    let yi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(xi == 6 || xi == 7); // ONE or NEG_ONE
    kani::assume(yi == 2 || yi == 3); // INFINITY or NEG_INFINITY
    kani::assume(rmi <= 4);

    let x = operand(xi);
    let y = operand(yi);
    let rm = rm_from_u8(rmi);
    let (r, _s) = x
        .pow_special_only_for_kani(y, rm)
        .expect("rule 2 / rule 6 must fire for ±1 and ±∞");
    assert!(r.to_bits() == Decimal128::ONE.to_bits());
}

/// `pow(x, ±0) = 1` for any `x` except sNaN, per rule 1.
#[kani::proof]
fn pow_x_pm_zero_is_one_except_snan() {
    let xi: u8 = kani::any();
    let yi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(xi < NUM_OPERANDS);
    kani::assume(xi != 1); // not sNaN
    kani::assume(yi == 4 || yi == 5); // ZERO or NEG_ZERO
    kani::assume(rmi <= 4);

    let x = operand(xi);
    let y = operand(yi);
    let rm = rm_from_u8(rmi);
    let (r, _s) = x
        .pow_special_only_for_kani(y, rm)
        .expect("rule 1 must fire for y = ±0");
    assert!(r.to_bits() == Decimal128::ONE.to_bits());
}

/// Every special-rule combination in the pool produces a defined
/// closed-form answer (or `None` for the general path); the shim's
/// rule dispatcher never panics regardless of which input slot fires.
/// This catches any future `unreachable!()` regression in the IEEE
/// rule table without dragging in the `ln_extended` / `exp_from_extended`
/// pipeline that drove `pow_special_pool_total`'s CBMC timeout pre-1.15.
#[kani::proof]
fn pow_special_dispatcher_total() {
    let xi: u8 = kani::any();
    let yi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(xi < NUM_OPERANDS);
    kani::assume(yi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let x = operand(xi);
    let y = operand(yi);
    let rm = rm_from_u8(rmi);
    let _ = x.pow_special_only_for_kani(y, rm);
}
