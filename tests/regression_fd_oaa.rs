//! fd-oaa: Decimal128 FMA correctness defect at large biased exponents.
//!
//! Found incidentally during the decimal32 correctness slice when a
//! `cargo test --workspace` run persisted a shrunk
//! `property_fma_oracle` counterexample. The shrunk operands (decoded
//! from the Decimal128 bit patterns) are:
//!
//! * `a` = `biased_exp` 6209, coef 1            → `1 × 10^33`
//! * `b` = `biased_exp` 6176, coef 1            → `1`
//! * `c` = `biased_exp` 6161, coef 3000000000000000 → `3.0`
//!
//! Exact `a × b + c` = `10^33 + 3` =
//! `1000000000000000000000000000000003`, which is exactly 34
//! significant digits. Decimal128 precision is 34, so the correctly
//! rounded result is that value with **no rounding** and status
//! `OK` (no `INEXACT`).
//!
//! The released kernel diverts this case into the sub-ULP path via a
//! static `shift_ab > SHIFT_LIMIT` trigger even though the grown
//! digit counts still fit the U384 buffer, dropping the `+ 3` and
//! raising a spurious `INEXACT`. This is the parent-crate analogue of
//! the static-alignment-window anti-pattern fixed in decimal64 /
//! decimal32 (ADR-0018 / ADR-0019).
//!
//! These tests are the durable reproducer (ADR-0010 reproduce-first);
//! they are independent of proptest seed internals. The scope-gate
//! probes (`add`, `mul`-then-`add`) determine whether the defect is
//! FMA-only or also present in the parent Decimal128 add path.

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode};

/// `1 × 10^33`.
fn a() -> Decimal128 {
    Decimal128::try_new(1, 33).unwrap()
}

/// `1`.
fn b() -> Decimal128 {
    Decimal128::try_new(1, 0).unwrap()
}

/// `3.0` in the exact cohort of the shrunk counterexample: coefficient
/// `3 000 000 000 000 000` at exponent `-15`.
fn c_shrunk() -> Decimal128 {
    Decimal128::try_new(3_000_000_000_000_000, -15).unwrap()
}

/// `3` as the minimal cohort (coefficient `3`, exponent `0`).
fn c_plain() -> Decimal128 {
    Decimal128::try_new(3, 0).unwrap()
}

/// Exact correctly rounded result: `10^33 + 3`, 34 digits, exponent 0.
fn want() -> Decimal128 {
    Decimal128::try_new(1_000_000_000_000_000_000_000_000_000_000_003, 0).unwrap()
}

fn eq(got: Decimal128, expect: Decimal128) -> bool {
    let (cmp, _) = got.partial_cmp(expect);
    cmp == Some(Ordering::Equal)
}

/// The fd-oaa reproducer in its exact shrunk cohort. Was `#[ignore]`
/// in the Phase 1 commit (reproduce-first); the Phase 2 kernel fix
/// removes the `#[ignore]` so the fix and its proof land together.
#[test]
fn fd_oaa_fma_shrunk_cohort() {
    let (got, st) = a().fma(b(), c_shrunk(), RoundingMode::NearestEven);
    assert!(
        eq(got, want()),
        "fma(1e33, 1, 3.0[coef 3e15,exp -15]) = {got:?}, want {:?}",
        want()
    );
    assert!(
        !st.inexact(),
        "exact representable result must not raise INEXACT, got status {st:?}"
    );
}

/// Same value, minimal `c` cohort — isolates whether the defect
/// depends on `c`'s exponent or only on the product's narrow
/// coefficient.
#[test]
fn fd_oaa_fma_plain_cohort() {
    let (got, st) = a().fma(b(), c_plain(), RoundingMode::NearestEven);
    assert!(
        eq(got, want()),
        "fma(1e33, 1, 3) = {got:?}, want {:?}",
        want()
    );
    assert!(!st.inexact(), "got status {st:?}");
}

/// Scope gate: does the parent Decimal128 `add` path independently
/// drop the small addend? `1e33 + 3` is exactly representable in 34
/// digits.
#[test]
fn fd_oaa_scope_gate_add() {
    let (got, st) = a().add(c_shrunk(), RoundingMode::NearestEven);
    assert!(
        eq(got, want()),
        "add(1e33, 3.0) = {got:?}, want {:?}",
        want()
    );
    assert!(!st.inexact(), "got status {st:?}");
}

/// Scope gate: `mul`-then-`add` (the path the FMA in-range arm defers
/// to). `mul(1e33, 1)` is exact; `add(., 3)` is exactly representable.
#[test]
fn fd_oaa_scope_gate_mul_then_add() {
    let (prod, sp) = a().mul(b(), RoundingMode::NearestEven);
    assert!(!sp.inexact(), "mul(1e33,1) must be exact, status {sp:?}");
    let (got, ss) = prod.add(c_shrunk(), RoundingMode::NearestEven);
    assert!(eq(got, want()), "mul-then-add = {got:?}, want {:?}", want());
    assert!(!ss.inexact(), "got status {ss:?}");
}
