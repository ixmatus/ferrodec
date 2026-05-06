//! Fuzz target: feed arbitrary byte strings through `Decimal128::parse_str`.
//!
//! Kani covers the special-case dispatch on hand-curated inputs;
//! libFuzzer covers the long tail of malformed input. The body asserts
//! parse never panics and that any parsed value (if any) round-trips
//! through `Display` to a string the parser also accepts (modulo
//! cohort).

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec::{Decimal128, RoundingMode};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = core::str::from_utf8(data) else {
        return;
    };
    let Ok((d, _status)) = Decimal128::parse_str(s, RoundingMode::NearestEven) else {
        return;
    };
    // If parse succeeds, Display + parse must produce a numerically
    // equivalent value (cohort may shift; numeric value must match).
    let rendered = format!("{d}");
    let (back, _) = Decimal128::parse_str(&rendered, RoundingMode::NearestEven)
        .expect("Display output must re-parse");
    if d.is_nan() {
        assert!(back.is_nan());
    } else {
        let (cmp, _) = back.partial_cmp(d);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "round-trip mismatch: {s:?} → {d:?} → {rendered} → {back:?}"
        );
    }
});
