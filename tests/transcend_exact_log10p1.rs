//! Exact-result, special-value, and flag gate for `Decimal128`'s
//! `log10_1p` (IEEE 754-2019 §9.2 `log10p1`; ADR-0059 Track D).
//!
//! `log10p1` inherits `log10`'s number theory through its argument:
//! a rational `log10(1 + x)` at a representable `x` forces
//! `(1 + x)^b = 10^a`, hence `1 + x = 10^k` and `b = 1`, so the exact
//! family is the *nines patterns* on the input: `9`, `99`, `999`, …
//! above zero and `-0.9`, `-0.99`, … below it. The family is finite
//! and small (`k ∈ [-34, 34]` at `Decimal128`, bounded by the digit
//! width rather than the exponent range), so this gate walks it
//! exhaustively in all five rounding directions rather than sampling
//! it, checks each classified `k` against an independently
//! reconstructed `10^k`, and pins the neighbours one ulp away as
//! inexact and on the correct side.
//!
//! The §7.5 stake: a spurious `INEXACT` on any of these 68 inputs is
//! a flag defect, and a directed-mode misround on them is the
//! ADR-0047 failure shape the input-side classifiers exist to close.

#![cfg(feature = "exp-log")]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` significand width; the exact family's `|k|` ceiling.
const PRECISION: u32 = 34;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// The `k` nines integer `10^k − 1` as a decimal literal.
fn nines(k: u32) -> String {
    "9".repeat(k as usize)
}

// ---------------------------------------------------------------------------
// The exact family, exhaustively, in every rounding direction.

/// Every positive member of the exact family: `x = 10^k − 1` for
/// `k ∈ 1..=34`, the `k` nines integer. Each must deliver exactly `k`
/// with `Status::OK` in all five directions: the value because the
/// classifier packs the true coefficient, and the clean status
/// because §7.5 forbids `INEXACT` on an exact result.
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

/// Every negative member: `x = −(10^m − 1)·10^−m` for `m ∈ 1..=34`,
/// the `m` nines fraction `−0.9`, `−0.99`, …. Each must deliver
/// exactly `−m` with `Status::OK` in all five directions. These are
/// the delicate half: they sit within one quantum of the `−1` domain
/// edge, where the function's slope is `10^m / ln 10`.
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

/// The independent witness: for every classified `k`, reconstruct
/// `10^k` and confirm it really is `1 + x`. Two routes, neither of
/// them the classifier's own arithmetic: exact `u128` integers for
/// the positive family, and the format's own `add` against a parsed
/// power of ten for both families.
#[test]
fn classified_k_reconstructs_one_plus_x() {
    let one = Decimal128::ONE;
    for k in 1..=PRECISION {
        // Integer route: the k nines plus one is 10^k, in u128 (10^34
        // is comfortably inside u128's ~3.4e38 envelope).
        let n: u128 = nines(k).parse().expect("nines fit u128");
        assert_eq!(
            n + 1,
            10u128.pow(k),
            "the {k} nines integer plus one is 10^{k}"
        );
        // Format route: 1 ⊕ x reproduces 10^k exactly.
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

// ---------------------------------------------------------------------------
// Neighbours of the exact family.

/// `true` when `got` is `want` or one representable step from it.
fn within_one_step(got: Decimal128, want: Decimal128) -> bool {
    equal(got, want) || equal(got, want.next_up().0) || equal(got, want.next_down().0)
}

/// One ulp on either side of a nines integer: `log10p1` is strictly
/// increasing, so the neighbour above `10^k − 1` has a true value
/// just above `k` and the neighbour below one just below it. Both are
/// irrational, so both raise `INEXACT`, and both land within one
/// representable step of `k`.
///
/// The sides are pinned under the directed mode that respects them.
/// At the nearest modes the true offset (`~4.3e-35` at `k = 1`) is
/// far under half an ulp of `k`, so the correctly rounded answer is
/// `k` itself, and a "strictly above" assertion there would be
/// asserting a misround.
///
/// Positive side only, deliberately: below zero the derivative is
/// `10^m / ln 10`, so the neighbours of the `m` nines fraction move
/// the result by ~`0.3`, not by an ulp. That is the function's
/// steepness at the domain edge, not a defect.
#[test]
fn neighbours_of_the_nines_integers_are_inexact_and_sided() {
    for k in [1u32, 17, PRECISION] {
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

// ---------------------------------------------------------------------------
// Flag honesty (§7.5).

/// Generic finite inputs are irrational under `log10p1` and must
/// raise `INEXACT` in every direction; the classified family must
/// raise nothing at all. The two halves together are the §7.5
/// contract: the flag says "the delivered value differs from the true
/// one", never "the kernel rounded something internally".
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "0.5",
        "2",
        "-0.25",
        "1e-20",
        "123456",
        "-0.999999999999999999999999999999998",
        // A huge generic input, and the integer-anchor family member
        // beside it: the latter is delivered by the ADR-0051 residual
        // channel (`exact::log10p1_power_of_ten_exponent`), INEXACT
        // like everything else on this list.
        "1.5e6000",
        "1e6000",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.log10_1p(rm);
            assert!(
                st.inexact(),
                "log10_1p({literal}) at {rm:?}: expected INEXACT"
            );
        }
    }
    // Cohort-insensitivity: the classifier reads the stripped form, so
    // a nines value stored at another quantum takes the exact path too.
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

/// The integer-anchor family of `log10p1`: for `x = 10^n` the true
/// value is `n + 10^-n / ln 10`, strictly above the integer `n` and
/// vastly below the next boundary, so `TowardPositive` must deliver
/// `next_up(n)` and every other mode `n`, always `INEXACT`. The wide
/// band's `1 ⊕ x` absorbs the `1` once `n` passes the rung width and
/// lands the working value exactly ON the grid point `n`, which no
/// fixed rung can move off; the kernel therefore decides the family
/// input side through the ADR-0051 residual channel for `n ≥ 36`
/// (`exact::log10p1_power_of_ten_exponent` carries the proof). This
/// test was the defect's discovery pin (a `ladder_audit` build
/// panicked here; default builds misrounded the directed modes for
/// `n` past the top fixed rung's width) and is now the family's
/// standing witness across both delivery regimes: `n ≤ 35` decided
/// by the kernel (exact `1 + 10^n` at rung width), `n ≥ 36` by the
/// residual channel, with the rung-1 and rung-2 absorption edges
/// (`49/50`, `106/107`) and the format ceiling covered.
#[test]
fn powers_of_ten_round_correctly_in_the_directed_modes() {
    for n in [
        2i32, 10, 34, 35, 36, 49, 50, 106, 107, 110, 200, 1000, 6000, 6144,
    ] {
        let x = parse(&format!("1e{n}"));
        let want = parse(&n.to_string());
        let want_up = want.next_up().0;
        // Below ~34 the residual 10^-n / ln 10 is visible in the
        // format's own digits, so the result is not the grid pair
        // (log10_1p(1e2) = 2.0043…); the corpus and property tests
        // own those values. Here only the flag matters.
        if n < 34 {
            for rm in ALL {
                let (r, st) = x.log10_1p(rm);
                assert!(r.is_finite(), "log10_1p(1e{n}) at {rm:?}: finite");
                assert!(st.inexact(), "log10_1p(1e{n}) at {rm:?}: INEXACT");
            }
            continue;
        }
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

/// One past each end of the exact family: `10^35 − 1` needs 35
/// digits, so it is not representable, and the widest representable
/// input just outside the family is inexact. The controls that keep
/// the classifier from being vacuously green.
#[test]
fn near_family_controls_stay_on_the_kernel() {
    for literal in [
        // A nines integer with one digit perturbed: 9…98.
        "9999999999999999999999999999999998",
        // 10^k itself, one step above the k nines.
        "1e34",
        // A nines fraction with a digit perturbed.
        "-0.99999999999999999999999999999998",
        // 10^-m − 1 is the family; 2·10^-m − 1 is not.
        "-0.8",
        "-0.98",
    ] {
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

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1.

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal128::ZERO.log10_1p(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "log10_1p(+0) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "log10_1p(+0) at {rm:?}: flags");

        let (r, st) = Decimal128::NEG_ZERO.log10_1p(rm);
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

        // The first of these is one quantum below `-1`: 34 significant
        // digits, so it survives the parse instead of rounding back
        // onto the `-1` domain edge.
        for literal in ["-1.000000000000000000000000000000001", "-2", "-1e6000"] {
            let (r, st) = parse(literal).log10_1p(rm);
            assert!(r.is_nan(), "log10_1p({literal}) at {rm:?}: want NaN");
            assert!(st.invalid(), "log10_1p({literal}) at {rm:?}: flags");
        }

        let (r, st) = Decimal128::NEG_INFINITY.log10_1p(rm);
        assert!(r.is_nan(), "log10_1p(-inf) at {rm:?}");
        assert!(st.invalid(), "log10_1p(-inf) at {rm:?}: flags");

        let (r, st) = Decimal128::INFINITY.log10_1p(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "log10_1p(+inf) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "log10_1p(+inf) at {rm:?}: flags");

        let (r, st) = Decimal128::NAN.log10_1p(rm);
        assert!(r.is_nan(), "log10_1p(NaN) at {rm:?}");
        assert_eq!(st, Status::OK, "log10_1p(NaN) at {rm:?}: flags");

        let (r, st) = Decimal128::SIGNALING_NAN.log10_1p(rm);
        assert!(
            r.is_nan() && !r.is_signaling_nan(),
            "log10_1p(sNaN) at {rm:?}"
        );
        assert!(st.invalid(), "log10_1p(sNaN) at {rm:?}: flags");
    }
}
