//! Property and reference tests for [`Decimal::ln`] and [`Decimal::log10`].
//!
//! Reference result strings come from Python's `decimal` module (libmpdec) and
//! the spec's `ln.decTest` / `log10.decTest`; the randomized cohort-exact
//! differential is in `tests/differential.rs`.

#![cfg(feature = "fmt")]

use ferrodec_decimal::{Context, Decimal, Rounding, Status};

fn wide(prec: u32) -> Context {
    Context::new(
        core::num::NonZeroU32::new(prec).unwrap(),
        99_999,
        -99_999,
        Rounding::HalfEven,
    )
}

fn parse(s: &str) -> Decimal {
    Decimal::parse_str(s).expect("valid literal")
}

#[test]
fn ln_specials() {
    let c = wide(9);
    // ln(0) and ln(-0) = -Infinity, with no flag.
    assert_eq!(parse("0").ln(&c), (parse("-Infinity"), Status::OK));
    assert_eq!(parse("-0").ln(&c), (parse("-Infinity"), Status::OK));
    // ln(1) = +0 exact for every cohort of one.
    for one in ["1", "1.0", "1.000000000000000", "1E+0"] {
        let (r, s) = parse(one).ln(&c);
        assert_eq!(
            (r.to_string(), s),
            ("0".to_string(), Status::OK),
            "ln({one})"
        );
    }
    // ln(+Infinity) = +Infinity.
    assert_eq!(parse("Infinity").ln(&c), (parse("Infinity"), Status::OK));
    // ln(negative) and ln(-Infinity) are Invalid_operation.
    for neg in ["-1", "-0.0007", "-9999", "-Infinity"] {
        let (r, s) = parse(neg).ln(&c);
        assert!(r.is_nan() && s.invalid(), "ln({neg})");
    }
    // sNaN raises Invalid; qNaN propagates.
    assert!(parse("sNaN").ln(&c).1.invalid());
    assert_eq!(parse("NaN").ln(&c).0.to_string(), "NaN");
}

#[test]
fn log10_specials_and_powers_of_ten() {
    let c = wide(9);
    assert_eq!(parse("0").log10(&c), (parse("-Infinity"), Status::OK));
    assert_eq!(parse("-0").log10(&c), (parse("-Infinity"), Status::OK));
    assert_eq!(parse("Infinity").log10(&c), (parse("Infinity"), Status::OK));
    for neg in ["-1", "-10", "-Infinity"] {
        let (r, s) = parse(neg).log10(&c);
        assert!(r.is_nan() && s.invalid(), "log10({neg})");
    }
    // Exact powers of ten -> exact integer exponent, no flags, any cohort.
    let exact: &[(&str, &str)] = &[
        ("1", "0"),
        ("10", "1"),
        ("100", "2"),
        ("1000", "3"),
        ("10.0", "1"),
        ("100.00", "2"),
        ("0.1", "-1"),
        ("0.001", "-3"),
        ("0.000001", "-6"),
        ("1E+9", "9"),
        ("1E-9", "-9"),
    ];
    for &(input, want) in exact {
        let (r, s) = parse(input).log10(&c);
        assert_eq!(
            (r.to_string(), s),
            (want.to_string(), Status::OK),
            "log10({input})"
        );
    }
}

#[test]
fn ln_known_values() {
    let cases: &[(&str, u32, &str)] = &[
        ("2", 9, "0.693147181"),
        ("2", 16, "0.6931471805599453"),
        ("2", 28, "0.6931471805599453094172321215"),
        ("2", 34, "0.6931471805599453094172321214581766"),
        ("3", 9, "1.09861229"),
        ("3", 34, "1.098612288668109691395245236922526"),
        ("0.5", 9, "-0.693147181"),
        ("0.5", 28, "-0.6931471805599453094172321215"),
        ("10", 9, "2.30258509"),
        ("10", 28, "2.302585092994045684017991455"),
        ("100", 16, "4.605170185988091"),
        ("0.0007", 16, "-7.264430222920869"),
        ("1.5", 16, "0.4054651081081644"),
        ("0.99", 9, "-0.0100503359"),
        ("0.99", 28, "-0.01005033585350144118354885756"),
        ("1.0001", 9, "0.0000999950003"),
        ("1.0001", 28, "0.00009999500033330833533316668095"),
        ("0.9999", 9, "-0.000100005000"),
        ("0.9999", 34, "-0.0001000050003333583353335000142869644"),
        ("0.7", 16, "-0.3566749439387324"),
        ("123.456", 28, "4.815884817283263883109232105"),
        ("1000000", 16, "13.81551055796427"),
        ("0.000001", 28, "-13.81551055796427410410794873"),
        ("7", 34, "1.945910149055313305105352743443180"),
        ("0.1", 28, "-2.302585092994045684017991455"),
        ("9.999", 16, "2.302485087993712"),
        ("1.00000001", 16, "9.999999950000000E-9"),
        ("5e40", 16, "93.71284163219593"),
        ("3e-30", 28, "-67.97894050115326082914449840"),
        ("2.718281828459045", 9, "1.00000000"),
    ];
    for &(input, prec, expected) in cases {
        let (r, s) = parse(input).ln(&wide(prec));
        assert_eq!(r.to_string(), expected, "ln({input}) p{prec}");
        assert!(s.inexact(), "ln({input}) p{prec} inexact");
    }
}

#[test]
fn log10_known_values() {
    let cases: &[(&str, u32, &str)] = &[
        ("2", 9, "0.301029996"),
        ("2", 34, "0.3010299956639811952137388947244930"),
        ("3", 16, "0.4771212547196624"),
        ("0.5", 28, "-0.3010299956639811952137388947"),
        ("0.0007", 16, "-3.154901959985743"),
        ("1.5", 28, "0.1760912590556812420812890085"),
        ("0.99", 16, "-0.004364805402450085"),
        ("1.0001", 16, "0.00004342727686266964"),
        ("0.9999", 28, "-0.00004343161980751038455604402381"),
        ("0.7", 9, "-0.154901960"),
        ("123.456", 16, "2.091512201627772"),
        ("9.999", 28, "0.9999565683801924896154439560"),
        ("1.00000001", 16, "4.342944797317794E-9"),
        ("5e40", 16, "40.69897000433602"),
        ("3e-30", 16, "-29.52287874528034"),
        ("2.718281828459045", 16, "0.4342944819032518"),
    ];
    for &(input, prec, expected) in cases {
        let (r, s) = parse(input).log10(&wide(prec));
        assert_eq!(r.to_string(), expected, "log10({input}) p{prec}");
        assert!(s.inexact(), "log10({input}) p{prec} inexact");
    }
}

#[test]
fn log10_exact_integer_cohort() {
    // log10(10^n) = n exactly for a wide range of n, all cohorts of the input.
    let c = wide(30);
    for n in -50i32..=50 {
        let input = Decimal::parse_str(&format!("1E{n}")).expect("valid");
        let (r, s) = input.log10(&c);
        assert_eq!(r.to_string(), n.to_string(), "log10(1E{n})");
        assert!(!s.inexact(), "log10(1E{n}) is exact");
    }
}

#[test]
fn exp_ln_round_trip() {
    // exp(ln(x)) == x to the working precision (compare at reduced precision to
    // absorb the two independent roundings).
    let c = wide(40);
    let cc = wide(30);
    for input in [
        "2", "0.5", "10", "123.456", "0.0007", "7", "1000000", "3.14159", "0.99",
    ] {
        let x = parse(input);
        let round_trip = x.ln(&c).0.exp(&c).0;
        // Compare values (not cohort strings) at the reduced precision: the
        // round trip is an inexact full-precision result whose value matches x.
        let lhs = round_trip.plus(&cc).0;
        let rhs = x.plus(&cc).0;
        assert!(
            lhs.compare(&rhs, &cc).0.is_zero(),
            "exp(ln({input})): {lhs} vs {rhs}"
        );
    }
}

#[test]
fn ln_log10_consistency() {
    // log10(x) == ln(x) / ln(10) to the working precision.
    let c = wide(40);
    let cc = wide(30);
    let ln10 = parse("10").ln(&c).0;
    for input in ["2", "0.7", "123.456", "0.0007", "9.999", "5e40"] {
        let x = parse(input);
        let via_ln = x.ln(&c).0.divide(&ln10, &c).0.plus(&cc).0;
        let direct = x.log10(&c).0.plus(&cc).0;
        assert!(
            via_ln.compare(&direct, &cc).0.is_zero(),
            "log10({input}) vs ln/ln10: {via_ln} vs {direct}"
        );
    }
}

#[test]
fn ln_monotonic_increasing() {
    let c = wide(25);
    let inputs = [
        "0.001", "0.5", "0.99", "1.0001", "1.5", "2", "10", "1000", "1e20",
    ];
    for w in inputs.windows(2) {
        let a = parse(w[0]).ln(&c).0;
        let b = parse(w[1]).ln(&c).0;
        assert!(
            a.compare(&b, &c).0.is_negative(),
            "ln({}) < ln({})",
            w[0],
            w[1]
        );
    }
}
