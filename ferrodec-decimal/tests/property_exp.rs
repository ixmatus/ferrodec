//! Property and reference tests for [`Decimal::exp`].
//!
//! Reference result strings are taken from Python's `decimal` module (libmpdec),
//! the General Decimal Arithmetic reference, and from the spec's own `exp.decTest`
//! cases; the randomized cohort-exact differential against libmpdec lives in
//! `tests/differential.rs` (the `differential` feature).

#![cfg(feature = "fmt")]

use ferrodec_decimal::{Context, Decimal, Rounding, Status};

const ROUNDINGS: [Rounding; 8] = [
    Rounding::HalfEven,
    Rounding::HalfUp,
    Rounding::HalfDown,
    Rounding::Down,
    Rounding::Up,
    Rounding::Ceiling,
    Rounding::Floor,
    Rounding::ZeroFiveUp,
];

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

fn exp_str(input: &str, ctx: &Context) -> (String, Status) {
    let (r, s) = parse(input).exp(ctx);
    (r.to_string(), s)
}

#[test]
fn exact_and_specials() {
    let c = wide(9);
    // exp(+/-0) = 1, exact (no flags).
    assert_eq!(exp_str("0", &c), ("1".to_string(), Status::OK));
    assert_eq!(exp_str("-0", &c), ("1".to_string(), Status::OK));
    // exp(-Infinity) = +0; exp(+Infinity) = +Infinity.
    assert_eq!(exp_str("-Infinity", &c), ("0".to_string(), Status::OK));
    assert_eq!(
        exp_str("Infinity", &c),
        ("Infinity".to_string(), Status::OK)
    );
    // quiet NaN propagates with no flag; signaling NaN raises Invalid.
    assert_eq!(exp_str("NaN", &c), ("NaN".to_string(), Status::OK));
    let (r, s) = parse("sNaN").exp(&c);
    assert!(r.is_nan() && !r.is_signaling_nan() && s.invalid());
}

#[test]
fn known_values_half_even() {
    // (input, precision, expected) from libmpdec / exp.decTest, half-even.
    let cases: &[(&str, u32, &str)] = &[
        ("1", 5, "2.7183"),
        ("1", 9, "2.71828183"),
        ("1", 10, "2.718281828"),
        ("1", 20, "2.7182818284590452354"),
        ("2", 10, "7.389056099"),
        ("0.5", 10, "1.648721271"),
        ("-1", 10, "0.3678794412"),
        ("-1", 9, "0.367879441"),
        ("-10", 9, "0.0000453999298"),
        ("0.693147181", 9, "2.00000000"),
        ("10", 15, "22026.4657948067"),
        ("-3.5", 15, "0.0301973834223185"),
        ("0.0001", 20, "1.0001000050001666708"),
        ("1234.5", 12, "1.36942392019E+536"),
    ];
    for &(input, prec, expected) in cases {
        let (got, st) = exp_str(input, &wide(prec));
        assert_eq!(got, expected, "exp({input}) at precision {prec}");
        assert!(
            st.inexact() && !st.overflow() && !st.underflow() && !st.clamped(),
            "exp({input}) flags {st:?}"
        );
    }
}

#[test]
fn broad_reference_set() {
    // 96 diverse cases (magnitudes, signs, precisions 7/16/28/34) from libmpdec.
    let cases: &[(&str, u32, &str)] = &[
        ("0.0000001", 7, "1.000000"),
        ("0.0000001", 16, "1.000000100000005"),
        ("0.0000001", 28, "1.000000100000005000000166667"),
        ("0.0000001", 34, "1.000000100000005000000166666670833"),
        ("0.123456789", 7, "1.131401"),
        ("0.123456789", 16, "1.131401114512234"),
        ("0.123456789", 28, "1.131401114512233603800987101"),
        ("0.123456789", 34, "1.131401114512233603800987101031456"),
        ("-0.5", 7, "0.6065307"),
        ("-0.5", 16, "0.6065306597126334"),
        ("-0.5", 28, "0.6065306597126334236037995350"),
        ("-0.5", 34, "0.6065306597126334236037995349911805"),
        ("7", 7, "1096.633"),
        ("7", 16, "1096.633158428459"),
        ("7", 28, "1096.633158428458599263720238"),
        ("7", 34, "1096.633158428458599263720238288121"),
        ("-7", 7, "0.0009118820"),
        ("-7", 16, "0.0009118819655545162"),
        ("-7", 28, "0.0009118819655545162080031360844"),
        ("-7", 34, "0.0009118819655545162080031360844092826"),
        ("42.42", 7, "2.647110E+18"),
        ("42.42", 16, "2.647109595645050E+18"),
        ("42.42", 28, "2647109595645050097.744514038"),
        ("42.42", 34, "2647109595645050097.744514038384807"),
        ("-99.9", 7, "4.111320E-44"),
        ("-99.9", 16, "4.111319781730108E-44"),
        ("-99.9", 28, "4.111319781730108180016653009E-44"),
        ("-99.9", 34, "4.111319781730108180016653008522259E-44"),
        ("3.14159265358979", 7, "23.14069"),
        ("3.14159265358979", 16, "23.14069263277919"),
        ("3.14159265358979", 28, "23.14069263277919406546045310"),
        (
            "3.14159265358979",
            34,
            "23.14069263277919406546045309773678",
        ),
        ("0.001", 7, "1.001001"),
        ("0.001", 16, "1.001000500166708"),
        ("0.001", 28, "1.001000500166708341668055754"),
        ("0.001", 34, "1.001000500166708341668055753993058"),
        ("-0.001", 7, "0.9990005"),
        ("-0.001", 16, "0.9990004998333750"),
        ("-0.001", 28, "0.9990004998333749916680553572"),
        ("-0.001", 34, "0.9990004998333749916680553571676560"),
        ("123.456", 7, "4.132944E+53"),
        ("123.456", 16, "4.132944352778093E+53"),
        ("123.456", 28, "4.132944352778093449576854412E+53"),
        ("123.456", 34, "4.132944352778093449576854412273431E+53"),
        ("-456.789", 7, "4.159661E-199"),
        ("-456.789", 16, "4.159660689053107E-199"),
        ("-456.789", 28, "4.159660689053107395937075327E-199"),
        ("-456.789", 34, "4.159660689053107395937075326888868E-199"),
        ("2.302585093", 7, "10.00000"),
        ("2.302585093", 16, "10.00000000005954"),
        ("2.302585093", 28, "10.00000000005954315982026272"),
        ("2.302585093", 34, "10.00000000005954315982026272255043"),
        ("-2.302585093", 7, "0.1000000"),
        ("-2.302585093", 16, "0.09999999999940457"),
        ("-2.302585093", 28, "0.09999999999940456840180091816"),
        ("-2.302585093", 34, "0.09999999999940456840180091816237710"),
        ("1000", 7, "1.970071E+434"),
        ("1000", 16, "1.970071114017047E+434"),
        ("1000", 28, "1.970071114017046993888879352E+434"),
        ("1000", 34, "1.970071114017046993888879352243323E+434"),
        ("-1000", 7, "5.075959E-435"),
        ("-1000", 16, "5.075958897549457E-435"),
        ("-1000", 28, "5.075958897549456765291809480E-435"),
        ("-1000", 34, "5.075958897549456765291809479574337E-435"),
        ("0.9999", 7, "2.718010"),
        ("0.9999", 16, "2.718010013867155"),
        ("0.9999", 28, "2.718010013867155437486515544"),
        ("0.9999", 34, "2.718010013867155437486515544070059"),
        ("-0.9999", 7, "0.3679162"),
        ("-0.9999", 16, "0.3679162309550180"),
        ("-0.9999", 28, "0.3679162309550179864579518329"),
        ("-0.9999", 34, "0.3679162309550179864579518329150867"),
        ("15.5", 7, "5389698"),
        ("15.5", 16, "5389698.476283012"),
        ("15.5", 28, "5389698.476283012367815210792"),
        ("15.5", 34, "5389698.476283012367815210792076178"),
        ("-15.5", 7, "1.855391E-7"),
        ("-15.5", 16, "1.855391362615978E-7"),
        ("-15.5", 28, "1.855391362615978240717108647E-7"),
        ("-15.5", 34, "1.855391362615978240717108647349252E-7"),
        ("0.0000000001", 7, "1.000000"),
        ("0.0000000001", 16, "1.000000000100000"),
        ("0.0000000001", 28, "1.000000000100000000005000000"),
        ("0.0000000001", 34, "1.000000000100000000005000000000167"),
        ("88", 7, "1.651636E+38"),
        ("88", 16, "1.651636254994002E+38"),
        ("88", 28, "1.651636254994001855528329796E+38"),
        ("88", 34, "1.651636254994001855528329796264859E+38"),
        ("-88", 7, "6.054602E-39"),
        ("-88", 16, "6.054601895401186E-39"),
        ("-88", 28, "6.054601895401185884531860534E-39"),
        ("-88", 34, "6.054601895401185884531860533810599E-39"),
        ("0.30102999566", 7, "1.351250"),
        ("0.30102999566", 16, "1.351249872561888"),
        ("0.30102999566", 28, "1.351249872561887583390200649"),
        ("0.30102999566", 34, "1.351249872561887583390200648815361"),
    ];
    for &(input, prec, expected) in cases {
        let (got, st) = exp_str(input, &wide(prec));
        assert_eq!(got, expected, "exp({input}) at precision {prec}");
        assert!(st.inexact(), "exp({input}) p{prec} inexact");
    }
}

#[test]
fn rounding_mode_is_always_half_even() {
    // exp ignores the context rounding mode (like squareRoot): every mode
    // produces the half-even result.
    for mode in ROUNDINGS {
        let c = Context::new(
            core::num::NonZeroU32::new(5).unwrap(),
            99_999,
            -99_999,
            mode,
        );
        assert_eq!(exp_str("1", &c).0, "2.7183", "mode {mode:?}");
        assert_eq!(exp_str("-1", &c).0, "0.36788", "mode {mode:?}");
    }
}

#[test]
fn monotonic_increasing() {
    let c = wide(25);
    let inputs = [
        "-100", "-3.5", "-1", "-0.0001", "0", "0.0001", "1", "2.5", "100",
    ];
    for w in inputs.windows(2) {
        let a = parse(w[0]).exp(&c).0;
        let b = parse(w[1]).exp(&c).0;
        // a < b strictly.
        assert!(
            a.compare(&b, &c).0.is_negative(),
            "exp({}) < exp({}) expected, got {a} vs {b}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn addition_theorem_metamorphic() {
    // exp(a + b) == exp(a) * exp(b) to the working precision.
    let c = wide(40);
    for (a, b) in [("1", "2"), ("0.5", "-1.25"), ("3.7", "-0.3"), ("-2", "-4")] {
        let sum = parse(a).add(&parse(b), &c).0;
        let lhs = sum.exp(&c).0;
        let rhs = parse(a).exp(&c).0.multiply(&parse(b).exp(&c).0, &c).0;
        // Each route rounds independently (a few ulp apart at precision 40);
        // compare at precision 30 so the ten guard digits absorb the slack.
        let cc = wide(30);
        assert_eq!(
            lhs.plus(&cc).0.to_string(),
            rhs.plus(&cc).0.to_string(),
            "exp({a}+{b}) vs exp({a})*exp({b})"
        );
    }
}

#[test]
fn overflow_and_underflow() {
    let c = Context::new(
        core::num::NonZeroU32::new(9).unwrap(),
        999,
        -999,
        Rounding::HalfEven,
    );

    // Far overflow -> +Infinity with Overflow + Inexact.
    let (r, s) = parse("10000").exp(&c);
    assert_eq!(r.to_string(), "Infinity");
    assert!(s.overflow() && s.inexact());

    // Far underflow -> +0 at Etiny with Underflow + Inexact + Clamped.
    let (r, s) = parse("-10000").exp(&c);
    assert_eq!(r.to_string(), "0E-1007");
    assert!(s.underflow() && s.inexact() && s.clamped());

    // Borderline finite at the top normal decade: no overflow.
    let (r, s) = parse("2302").exp(&c);
    assert_eq!(r.to_string(), "5.57054057E+999");
    assert!(s.inexact() && !s.overflow() && !s.underflow());

    // Borderline subnormal: Underflow + Inexact, but a nonzero result.
    let (r, s) = parse("-2310").exp(&c);
    assert_eq!(r.to_string(), "6.022E-1004");
    assert!(s.underflow() && s.inexact());

    // Just over the top: overflow to Infinity.
    let (r, s) = parse("2302.6").exp(&c);
    assert_eq!(r.to_string(), "Infinity");
    assert!(s.overflow() && s.inexact());
}
