//! Exact-result, special-value, and flag gate for `Decimal32`'s
//! `log10_1p` (IEEE 754-2019 §9.2 `log10p1`; ADR-0059 Track D). The
//! sibling mirror of the root crate's
//! `tests/transcend_exact_log10p1.rs`.
//!
//! The exact family is the nines patterns on the input: a rational
//! `log10(1 + x)` at a representable `x` forces `1 + x = 10^k` with
//! integer `k`, so `x` is `10^k − 1` above zero and
//! `−(10^m − 1)·10^−m` below it. `Decimal32`'s 7 digit width, not
//! its exponent range, is what bounds `k` to `[-7, 7]`; that is the
//! asymmetry with `log10`, whose exact family runs the full `±101`.
//! The family is small enough to walk exhaustively in all five
//! rounding directions.

#![cfg(feature = "exp-log")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal32` significand width; the exact family's `|k|` ceiling.
const PRECISION: u32 = 7;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// The `k` nines integer `10^k − 1` as a decimal literal.
fn nines(k: u32) -> String {
    "9".repeat(k as usize)
}

#[test]
fn exact_nines_integers_every_mode() {
    for k in 1..=PRECISION {
        let x = parse(&nines(k));
        let want = parse(&k.to_string());
        for rm in ALL {
            let (r, st) = x.log10_1p(rm);
            assert!(
                equal(r, want),
                "log10_1p({}) at {rm:?}: got {r}, want {k}",
                nines(k)
            );
            assert_eq!(st, Status::OK, "log10_1p({}) at {rm:?}: flags", nines(k));
        }
    }
}

#[test]
fn exact_nines_fractions_every_mode() {
    for m in 1..=PRECISION {
        let literal = format!("-{}e-{m}", nines(m));
        let x = parse(&literal);
        let want = parse(&format!("-{m}"));
        for rm in ALL {
            let (r, st) = x.log10_1p(rm);
            assert!(
                equal(r, want),
                "log10_1p({literal}) at {rm:?}: got {r}, want -{m}"
            );
            assert_eq!(st, Status::OK, "log10_1p({literal}) at {rm:?}: flags");
        }
    }
}

/// The independent witness: every classified `k` reconstructs
/// `1 + x = 10^k`, once in exact `u128` integers and once through the
/// format's own `add`.
#[test]
fn classified_k_reconstructs_one_plus_x() {
    let one = Decimal32::ONE;
    for k in 1..=PRECISION {
        let n: u128 = nines(k).parse().expect("nines fit u128");
        assert_eq!(
            n + 1,
            10u128.pow(k),
            "the {k} nines integer plus one is 10^{k}"
        );
        let x = parse(&nines(k));
        let (sum, _) = x.add(one, NE);
        assert!(
            equal(sum, parse(&format!("1e{k}"))),
            "1 + {} is 10^{k}, got {sum}",
            nines(k)
        );
    }
    for m in 1..=PRECISION {
        let literal = format!("-{}e-{m}", nines(m));
        let x = parse(&literal);
        let (sum, _) = x.add(one, NE);
        assert!(
            equal(sum, parse(&format!("1e-{m}"))),
            "1 + {literal} is 10^-{m}, got {sum}"
        );
    }
}

/// `true` when `got` is `want` or one representable step from it.
fn within_one_step(got: Decimal32, want: Decimal32) -> bool {
    equal(got, want) || equal(got, want.next_up().0) || equal(got, want.next_down().0)
}

/// One ulp on either side of a nines integer, sided under the
/// directed mode that respects the side (at the nearest modes the
/// true offset is far under half an ulp of `k`, so `k` itself is the
/// correctly rounded answer there). Positive side only: below zero
/// the derivative is `10^m / ln 10`, so a one ulp input step moves
/// the result by ~`0.3`.
#[test]
fn neighbours_of_the_nines_integers_are_inexact_and_sided() {
    for k in [1u32, 4, PRECISION] {
        let x = parse(&nines(k));
        let want = parse(&k.to_string());

        let (up, _) = x.next_up();
        let (r, st) = up.log10_1p(TP);
        assert!(
            st.inexact(),
            "log10_1p(next_up({k} nines)): expected INEXACT"
        );
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Greater),
            "log10_1p(next_up({k} nines)) at TowardPositive must exceed {k}, got {r}"
        );
        assert!(
            within_one_step(r, want),
            "log10_1p(next_up({k} nines)) = {r} is more than one step from {k}"
        );

        let (dn, _) = x.next_down();
        let (r, st) = dn.log10_1p(TN);
        assert!(
            st.inexact(),
            "log10_1p(next_down({k} nines)): expected INEXACT"
        );
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Less),
            "log10_1p(next_down({k} nines)) at TowardNegative must fall below {k}, got {r}"
        );
        assert!(
            within_one_step(r, want),
            "log10_1p(next_down({k} nines)) = {r} is more than one step from {k}"
        );
    }
}

/// §7.5 flag honesty: generic inputs are irrational and raise
/// `INEXACT` in every direction; the classified family raises
/// nothing, at every quantum of its cohort.
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in ["0.5", "2", "-0.25", "1e-20", "123456", "-0.9999998", "1e90"] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.log10_1p(rm);
            assert!(
                st.inexact(),
                "log10_1p({literal}) at {rm:?}: expected INEXACT"
            );
        }
    }
    for literal in ["9", "9.000", "99", "-0.9", "-0.9000", "-0.99"] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.log10_1p(rm);
            assert_eq!(
                st,
                Status::OK,
                "log10_1p({literal}) at {rm:?}: exact result must raise nothing (§7.5)"
            );
        }
    }
}

/// `Decimal32` is the one sibling outside the `log10p1` grid-hugger
/// class its parent and `Decimal64` carry (see the root crate's
/// `tests/transcend_exact_log10p1.rs`): `1 ⊕ 10^n` loses the `1` only
/// once `n` passes the working rung's width, and `Decimal32`'s whole
/// exponent range tops out at `10^96`, inside rung 2's 110 digits. So
/// the powers of ten round correctly here at every direction, and
/// this test runs rather than being ignored.
#[test]
fn powers_of_ten_round_correctly_in_the_directed_modes() {
    // 8..35 decided by the kernel from the exact `1 + 10^n`; 36 and
    // up by the ADR-0051 residual channel
    // (`exact::log10p1_power_of_ten_exponent`), with the rung-1
    // absorption edge (49/50) and the format ceiling covered.
    for n in [8i32, 20, 35, 36, 49, 50, 60, 90, 96] {
        let x = parse(&format!("1e{n}"));
        let want = parse(&n.to_string());
        let want_up = want.next_up().0;
        let (rp, sp) = x.log10_1p(TP);
        assert!(
            equal(rp, want_up),
            "log10_1p(1e{n}) at TowardPositive: got {rp}, want {want_up}"
        );
        assert!(sp.inexact(), "log10_1p(1e{n}) at TowardPositive: INEXACT");
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.log10_1p(rm);
            assert!(
                equal(r, want),
                "log10_1p(1e{n}) at {rm:?}: got {r}, want {want}"
            );
            assert!(st.inexact(), "log10_1p(1e{n}) at {rm:?}: INEXACT");
        }
    }
}

/// The controls that keep the classifier from being vacuously green:
/// values just outside the family stay on the kernel.
#[test]
fn near_family_controls_stay_on_the_kernel() {
    for literal in ["9999998", "1e7", "-0.9999998", "-0.8", "-0.98"] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.log10_1p(rm);
            assert!(
                st.inexact(),
                "log10_1p({literal}) at {rm:?}: expected the kernel path and INEXACT"
            );
        }
    }
}

/// Special values, IEEE 754-2019 §9.2.1.
#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal32::ZERO.log10_1p(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "log10_1p(+0) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "log10_1p(+0) at {rm:?}: flags");

        let (r, st) = Decimal32::NEG_ZERO.log10_1p(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "log10_1p(-0) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "log10_1p(-0) at {rm:?}: flags");

        for literal in ["-1", "-1.000"] {
            let (r, st) = parse(literal).log10_1p(rm);
            assert!(
                r.is_infinite() && r.is_sign_negative(),
                "log10_1p({literal}) at {rm:?}: want -inf, got {r}"
            );
            assert!(st.div_by_zero(), "log10_1p({literal}) at {rm:?}: flags");
        }

        // The first is one quantum below `-1`: 7 significant digits,
        // so it survives the parse instead of rounding onto the edge.
        for literal in ["-1.000001", "-2", "-1e90"] {
            let (r, st) = parse(literal).log10_1p(rm);
            assert!(r.is_nan(), "log10_1p({literal}) at {rm:?}: want NaN");
            assert!(st.invalid(), "log10_1p({literal}) at {rm:?}: flags");
        }

        let (r, st) = Decimal32::NEG_INFINITY.log10_1p(rm);
        assert!(r.is_nan(), "log10_1p(-inf) at {rm:?}");
        assert!(st.invalid(), "log10_1p(-inf) at {rm:?}: flags");

        let (r, st) = Decimal32::INFINITY.log10_1p(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "log10_1p(+inf) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "log10_1p(+inf) at {rm:?}: flags");

        let (r, st) = Decimal32::NAN.log10_1p(rm);
        assert!(r.is_nan(), "log10_1p(NaN) at {rm:?}");
        assert_eq!(st, Status::OK, "log10_1p(NaN) at {rm:?}: flags");

        let (r, st) = Decimal32::SIGNALING_NAN.log10_1p(rm);
        assert!(
            r.is_nan() && !r.is_signaling_nan(),
            "log10_1p(sNaN) at {rm:?}"
        );
        assert!(st.invalid(), "log10_1p(sNaN) at {rm:?}: flags");
    }
}
