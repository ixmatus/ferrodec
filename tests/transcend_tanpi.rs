//! Exact-result, pole, special-value, and metamorphic gate for
//! `Decimal128::tan_pi` (IEEE 754-2019 §9.2 `tanPi`; ADR-0061 Track D
//! group D4).
//!
//! The classifier `ferrodec_transcend::exact_pi::tanpi_exact` claims a
//! complete exact set with three rows: the integers (zero, signed
//! `(−1)^n · sign(x)`), the quarter integers (`±1`), and the half
//! integers (`±∞` with `DIV_BY_ZERO`). `tanPi` is the only member of
//! the trio with an exact `±1` family, and the reason is decimal
//! representability: `0.25` and `0.75` and their translates are format
//! values, where the `k ± 1/6` and `k ± 1/3` abscissas the sine and
//! cosine would need are not.
//!
//! ## The poles carry no overflow gate, by proof
//!
//! [`poles_cannot_overflow`] is the executable form of ADR-0061's
//! deliberate absence. Representing `n + 1/2 + δ` forces `δ` to be a
//! nonzero multiple of the operand's own quantum, and a pole
//! neighborhood has magnitude at least `1/2`, so `|δ| ≥ 10^-34` and
//! `|tanPi| = |cot(πδ)| ≤ 1/(π|δ|) ≤ 10^34/π ≈ 3.18·10^33` — 6111
//! decades inside this format's ceiling. The test drives the closest
//! representable neighbours on both sides of a pole and requires a
//! finite result of exactly that magnitude, so a future kernel that
//! saturated instead would break here loudly.
//!
//! ## The `±1` neighbourhood needs no anchor either
//!
//! The same quantum floor bounds the offset from a quarter integer
//! below by `10^-34`, so the tightest hug the format can express is
//! `2π·10^-34 ≈ 6.3·10^-34`, which rung 1 resolves with twelve decades
//! to spare. The kernel carries the ADR-0051 arm ADR-0061 specifies,
//! but its gate (`adj(ε) ≤ −38`) sits three decades below that floor
//! and so never fires; [`quarter_integer_hug_resolves_on_the_ladder`]
//! pins the behaviour the ladder delivers instead.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `tan(54°) = √(5 + 2√5)` at 34 digits.
const TAN_54: &str = "1.376381920471173538207209581910888";
/// `tan(18°)` at 34 digits.
const TAN_18: &str = "0.3249196962329063261558714122151345";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

fn assert_exact(got: (Decimal128, Status), want: Decimal128, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

fn assert_zero(got: (Decimal128, Status), neg: bool, label: &str) {
    let (r, st) = got;
    assert!(r.is_zero(), "{label}: got {r}, want a zero");
    assert_eq!(r.is_sign_negative(), neg, "{label}: zero sign, got {r}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        // tanPi(±0) is ±0.
        assert_zero(
            Decimal128::ZERO.tan_pi(rm),
            false,
            &format!("tanPi(+0) [{rm:?}]"),
        );
        assert_zero(
            Decimal128::NEG_ZERO.tan_pi(rm),
            true,
            &format!("tanPi(-0) [{rm:?}]"),
        );

        // tanPi(±∞) is a quiet NaN with INVALID.
        for (x, label) in [
            (Decimal128::INFINITY, "+inf"),
            (Decimal128::NEG_INFINITY, "-inf"),
        ] {
            let (r, st) = x.tan_pi(rm);
            assert!(r.is_nan(), "tanPi({label}) [{rm:?}] = {r}, want NaN");
            assert!(st.invalid(), "tanPi({label}) [{rm:?}] status {st:?}");
        }

        let (r, st) = Decimal128::NAN.tan_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "tanPi(NaN) [{rm:?}] = {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.tan_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "tanPi(sNaN) [{rm:?}] = {r} {st:?}"
        );
    }
}

/// The integer row: zero signed `(−1)^n · sign(x)`. The parity flips
/// the zero's sign, which the sine's integer row does not do, so this
/// pins the two apart.
#[test]
fn exact_at_the_integers() {
    for (label, odd) in [
        ("0", false),
        ("1", true),
        ("2", false),
        ("3", true),
        ("17", true),
        ("100", false),
        ("1000001", true),
        ("1E+34", false),
        ("1E+6100", false),
        ("9999999999999999999999999999999999", true),
        ("2.0", false),
        ("200E-2", false),
    ] {
        for rm in ALL {
            // Positive operand: sign is the parity alone.
            assert_zero(
                parse(label).tan_pi(rm),
                odd,
                &format!("tanPi({label}) [{rm:?}]"),
            );
            // Negative operand: parity XOR the operand's sign.
            let neg = format!("-{label}");
            assert_zero(
                parse(&neg).tan_pi(rm),
                !odd,
                &format!("tanPi({neg}) [{rm:?}]"),
            );
        }
    }
}

/// The quarter-integer row: `+1` at `n + 1/4` and `−1` at `n + 3/4`,
/// period 1, reflected through the odd function. The family the decimal
/// formats keep.
#[test]
fn exact_at_the_quarter_integers() {
    let one = parse("1");
    let neg_one = parse("-1");
    for (label, positive) in [
        ("0.25", true),
        ("0.75", false),
        ("1.25", true),
        ("1.75", false),
        ("2.25", true),
        ("2.75", false),
        ("100.25", true),
        ("100.75", false),
        // Cohort variants.
        ("0.250", true),
        ("0.2500000000000000000000000000000000", true),
        ("25E-2", true),
        ("75E-2", false),
        // The widest quarter integer this format can spell.
        ("99999999999999999999999999999999.25", true),
        ("99999999999999999999999999999999.75", false),
    ] {
        let want = if positive { one } else { neg_one };
        for rm in ALL {
            assert_exact(
                parse(label).tan_pi(rm),
                want,
                &format!("tanPi({label}) [{rm:?}]"),
            );
            let neg = format!("-{label}");
            assert_exact(
                parse(&neg).tan_pi(rm),
                want.neg(),
                &format!("tanPi({neg}) [{rm:?}]"),
            );
        }
    }
}

/// The half-integer row: the poles. `+∞` for even `n`, `−∞` for odd
/// `n`, odd-reflected for a negative operand, `DIV_BY_ZERO` and never
/// `INEXACT` in every direction.
#[test]
fn poles_at_the_half_integers() {
    for (label, neg_result) in [
        ("0.5", false),
        ("1.5", true),
        ("2.5", false),
        ("3.5", true),
        ("100.5", false),
        ("101.5", true),
        ("2.50", false),
        ("250E-2", false),
        ("999999999999999999999999999999999.5", true),
    ] {
        for rm in ALL {
            let (r, st) = parse(label).tan_pi(rm);
            assert!(r.is_infinite(), "tanPi({label}) [{rm:?}] = {r}, want ±inf");
            assert_eq!(
                r.is_sign_negative(),
                neg_result,
                "tanPi({label}) [{rm:?}] pole sign, got {r}"
            );
            assert!(st.div_by_zero(), "tanPi({label}) [{rm:?}] status {st:?}");
            assert!(
                !st.inexact(),
                "tanPi({label}) [{rm:?}] is an exact pole: {st:?}"
            );

            // Odd reflection.
            let negl = format!("-{label}");
            let (r, st) = parse(&negl).tan_pi(rm);
            assert!(r.is_infinite(), "tanPi({negl}) [{rm:?}] = {r}");
            assert_eq!(
                r.is_sign_negative(),
                !neg_result,
                "tanPi({negl}) [{rm:?}] pole sign, got {r}"
            );
            assert!(st.div_by_zero(), "tanPi({negl}) [{rm:?}] status {st:?}");
        }
    }
}

/// The no-overflow proof, driven at the closest representable
/// neighbours of a pole. `|δ| = 10^-34` is the tightest offset the
/// format can spell beside `0.5`, so this is the largest pole value the
/// operation can ever produce: `1/(π·10^-34) ≈ 3.18·10^33`, finite,
/// with 6111 decades of headroom. The sign alternates across the pole.
#[test]
fn poles_cannot_overflow() {
    // `10^34/π`, from `1/π = 0.3183098861837906715377675267450287240…`
    // rounded to this format's 34 digits. `cot(t) = 1/t − t/3 − …`, and
    // the correction is `≈ 10^-34`, thirty-four decades under the last
    // digit, so the cap is `1/t` to every digit the format carries.
    let cap = parse("3.183098861837906715377675267450287E+33");
    // The global ceiling `10^34/π` over every pole neighbourhood, and a
    // floor loose enough to hold for all of them.
    let global_cap = parse("3.184E+33");
    let floor = parse("1E+32");
    // (operand, expected result sign). The offset a pole neighbour can
    // carry depends on the operand's own adjusted exponent: beside
    // `0.5` it is `10^-34`, beside `1.5` only `10^-33`, because the
    // integer digit eats one of the 34. The cap tracks that, which is
    // the quantum argument doing its work.
    let cases: [(&str, bool); 4] = [
        // Just below 0.5: tan climbs to +∞.
        ("0.4999999999999999999999999999999999", false),
        // Just above 0.5: tan comes back from −∞.
        ("0.5000000000000000000000000000000001", true),
        // Just below 1.5: the mirrored pole, one decade tamer.
        ("1.499999999999999999999999999999999", false),
        ("1.500000000000000000000000000000001", true),
    ];
    for (label, neg) in cases {
        for rm in ALL {
            let (r, st) = parse(label).tan_pi(rm);
            assert!(
                r.is_finite(),
                "tanPi({label}) [{rm:?}] = {r}: the pole cap says finite"
            );
            assert!(!st.div_by_zero(), "tanPi({label}) [{rm:?}]: {st:?}");
            assert!(st.inexact(), "tanPi({label}) [{rm:?}] must be INEXACT");
            assert_eq!(
                r.is_sign_negative(),
                neg,
                "tanPi({label}) [{rm:?}] = {r}: wrong side of the pole"
            );
            let mag = r.abs();
            assert!(
                mag.partial_cmp(floor).0 == Some(Ordering::Greater)
                    && mag.partial_cmp(global_cap).0 == Some(Ordering::Less),
                "tanPi({label}) [{rm:?}] = {r} left the proven 10^34/π cap"
            );
        }
    }
    // The tightest neighbour the format can spell lands on the derived
    // cap `1/(π·10^-34)`: the largest value this operation can produce.
    let (r, _) = parse("0.4999999999999999999999999999999999").tan_pi(NE);
    assert!(
        eq(r, cap),
        "the pole cap drifted: got {r}, want {cap} (= 1/(π·10^-34))"
    );
}

/// The `±1` neighbourhood. The kernel's anchor gate never fires here
/// (the quantum floor sits three decades above it), so this pins what
/// the ladder delivers: the hug at the tightest reachable offset is
/// `2π·10^-34`, which straddles the `5·10^-34` midpoint above 1 and so
/// rounds *away* from the anchor under nearest, while the toward-zero
/// directions return exactly 1.
#[test]
fn quarter_integer_hug_resolves_on_the_ladder() {
    let one = parse("1");
    let above = parse("1.000000000000000000000000000000001");
    // Just above 0.25: tan is increasing, so the value passes 1.
    let x = parse("0.2500000000000000000000000000000001");
    for rm in [NE, NA, TP] {
        let (r, st) = x.tan_pi(rm);
        assert!(eq(r, above), "tanPi(0.25+ulp) [{rm:?}] = {r}, want {above}");
        assert!(st.inexact(), "tanPi(0.25+ulp) [{rm:?}] must be INEXACT");
    }
    for rm in [TZ, TN] {
        let (r, _) = x.tan_pi(rm);
        assert!(eq(r, one), "tanPi(0.25+ulp) [{rm:?}] = {r}, want 1");
    }
    // Just below 0.25: strictly under 1, so every direction stays there.
    let y = parse("0.2499999999999999999999999999999999");
    for rm in ALL {
        let (r, st) = y.tan_pi(rm);
        assert!(st.inexact(), "tanPi(0.25-ulp) [{rm:?}] must be INEXACT");
        assert!(
            r.partial_cmp(one).0 == Some(Ordering::Less),
            "tanPi(0.25-ulp) [{rm:?}] = {r}, want strictly under 1"
        );
    }
    // The 3/4 mirror, where the anchor is −1 and the magnitude grows on
    // the other side.
    let z = parse("0.7500000000000000000000000000000001");
    for rm in [NE, NA] {
        let (r, _) = z.tan_pi(rm);
        assert!(
            r.abs().partial_cmp(one).0 == Some(Ordering::Less),
            "tanPi(0.75+ulp) [{rm:?}] = {r}: magnitude should shrink"
        );
        assert!(r.is_sign_negative(), "tanPi(0.75+ulp) [{rm:?}] = {r}");
    }
}

/// The fifth-turn values against closed forms derived independently of
/// this kernel, plus the quotient identity through the format's own
/// division.
#[test]
fn known_values_and_the_quotient_identity() {
    for (label, want) in [("0.3", TAN_54), ("0.1", TAN_18), ("0.7", TAN_54)] {
        let (r, st) = parse(label).tan_pi(NE);
        let expect = if label == "0.7" {
            parse(&format!("-{want}"))
        } else {
            parse(want)
        };
        assert!(eq(r, expect), "tanPi({label}) = {r}, want {expect}");
        assert!(st.inexact(), "tanPi({label}) is irrational: {st:?}");
    }
    // tanPi = sinPi/cosPi to within one format division.
    let tolerance = parse("1E-30");
    for label in ["0.3", "0.1", "1.2", "1.8", "12.34", "0.0001"] {
        let x = parse(label);
        let (t, _) = x.tan_pi(NE);
        let (s, _) = x.sin_pi(NE);
        let (c, _) = x.cos_pi(NE);
        let (q, _) = s.div(c, NE);
        let (diff, _) = t.sub(q, NE);
        let (scaled, _) = diff.div(q, NE);
        assert!(
            scaled.abs().partial_cmp(tolerance).0 == Some(Ordering::Less),
            "tanPi({label}) = {t} but sinPi/cosPi = {q}"
        );
    }
}

/// `tanPi` is odd, bit for bit.
#[test]
fn oddness_is_bitwise() {
    for label in [
        "0.3",
        "0.7",
        "1.2",
        "1.8",
        "3.7",
        "0.25",
        "0.5",
        "1",
        "2",
        "1E-20",
        "0.0001",
        "12345.6789",
        "1E+40",
    ] {
        let x = parse(label);
        let neg = parse(&format!("-{label}"));
        for rm in ALL {
            let (a, sa) = x.tan_pi(rm);
            let (b, sb) = neg.tan_pi(rm.for_negation());
            assert_eq!(
                a.to_bits(),
                b.neg().to_bits(),
                "tanPi(-{label}) [{rm:?}] is not -tanPi({label}): {a} vs {b}"
            );
            assert_eq!(sa, sb, "tanPi(±{label}) [{rm:?}] flags differ");
        }
    }
}

/// `tanPi` has period 1 in revolutions, bit for bit, wherever both
/// operands are representable. The exact reduction is what makes this
/// hold at every magnitude rather than degrading with the argument.
///
/// The poles are the one deliberate exception, pinned separately in
/// [`poles_break_period_one_by_design`].
#[test]
fn period_one_is_bitwise() {
    for label in ["0.3", "0.1", "0.7", "0.25", "0.75", "0.0001", "0.123456789"] {
        let x = parse(label);
        let (shifted, st_add) = x.add(parse("1"), NE);
        assert!(!st_add.inexact(), "{label} + 1 must be exact here");
        for rm in ALL {
            let (a, sa) = x.tan_pi(rm);
            let (b, sb) = shifted.tan_pi(rm);
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "tanPi({label}) = {a} but tanPi({label} + 1) = {b}"
            );
            assert_eq!(sa, sb, "tanPi period-1 flags differ at {label}");
        }
    }
}

/// The poles are the one place period 1 does NOT hold bitwise, and the
/// break is §9.2.1's sign convention rather than a reduction defect:
/// the pole at `n + 1/2` is `+∞` for even `n` and `−∞` for odd `n`, so
/// stepping a whole revolution flips the delivered sign even though the
/// two-sided limit is identical. Pinned here so the convention cannot
/// drift and so the exception is visible next to the rule.
#[test]
fn poles_break_period_one_by_design() {
    for (a, b) in [("0.5", "1.5"), ("1.5", "2.5"), ("2.5", "3.5")] {
        let (ra, sa) = parse(a).tan_pi(NE);
        let (rb, sb) = parse(b).tan_pi(NE);
        assert!(ra.is_infinite() && rb.is_infinite());
        assert_ne!(
            ra.is_sign_negative(),
            rb.is_sign_negative(),
            "tanPi({a}) and tanPi({b}) should straddle the parity rule"
        );
        assert!(sa.div_by_zero() && sb.div_by_zero());
    }
    // Two revolutions restores the sign: the parity has period 2.
    for (a, b) in [("0.5", "2.5"), ("1.5", "3.5")] {
        let (ra, _) = parse(a).tan_pi(NE);
        let (rb, _) = parse(b).tan_pi(NE);
        assert_eq!(
            ra.to_bits(),
            rb.to_bits(),
            "tanPi({a}) and tanPi({b}) should agree"
        );
    }
}

/// Small arguments track the slope `π` rather than sticking to the
/// operand: `tan(πx) ≈ πx`, the non-anchor fact at zero.
#[test]
fn small_arguments_track_the_pi_slope() {
    let pi = parse("3.141592653589793238462643383279503");
    for label in ["1E-20", "1E-40", "1E-100", "1E-3000"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.tan_pi(rm);
            assert!(st.inexact(), "tanPi({label}) [{rm:?}] must be INEXACT");
            assert!(!eq(r, x), "tanPi({label}) [{rm:?}] stuck to the operand");
        }
        let (r, _) = x.tan_pi(NE);
        let (want, _) = x.mul(pi, NE);
        let (diff, _) = r.sub(want, NE);
        let (scaled, _) = diff.div(want, NE);
        assert!(
            scaled.abs().partial_cmp(parse("1E-30")).0 == Some(Ordering::Less),
            "tanPi({label}) = {r} is not π·x = {want}"
        );
    }
}
