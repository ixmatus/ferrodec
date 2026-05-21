//! Decimal-digit decomposition for the `Decimal64` coefficient.
//!
//! Per-precision counterpart to the parent's `src/digits.rs`; see
//! that module's preamble and ADR-0031 for the rationale (the
//! `ferrodec-ieee/src/digits.rs:9-13` carve-out applies). The
//! `Decimal64` coefficient fits in `u64`, so the digit buffer is
//! `[u8; PRECISION]` with `PRECISION = 16`.

/// Decimal64 coefficient precision, in base-10 digits.
pub(crate) const PRECISION: usize = 16;

/// Writes the base-10 digits of `c` into `out` with the least
/// significant digit at index 0. Returns the number of significant
/// digits actually produced.
///
/// The count matches [`crate::bid::decimal_digit_count`]: `c == 0`
/// yields one digit. Indices `[count..PRECISION]` are zero-filled so
/// callers may treat the whole buffer as a fixed-width digit stream
/// (the logical / shift / rotate kernels rely on this).
///
/// Panics in debug builds if any decimal digit would exceed nine,
/// which is unreachable for a well-formed BID-64 coefficient.
#[allow(dead_code)] // Wired by ADR-0031 op kernels.
pub(crate) const fn coefficient_to_digits(mut c: u64, out: &mut [u8; PRECISION]) -> usize {
    let mut i = 0usize;
    loop {
        let d = (c % 10) as u8;
        debug_assert!(d <= 9);
        out[i] = d;
        c /= 10;
        i += 1;
        if c == 0 {
            break;
        }
        debug_assert!(i < PRECISION);
    }
    let count = i;
    while i < PRECISION {
        out[i] = 0;
        i += 1;
    }
    count
}

/// Reconstructs a `u64` coefficient from a digit array (least
/// significant digit at index 0). Bytes outside `0..=9` panic in
/// debug builds.
///
/// The caller is responsible for ensuring the reconstructed value
/// fits in `u64`. The longest valid input has length [`PRECISION`]
/// and bounds the result by `10^PRECISION - 1`, well below `u64::MAX`.
#[allow(dead_code)] // Wired by ADR-0031 op kernels.
pub(crate) const fn digits_to_coefficient(digits: &[u8]) -> u64 {
    let mut c: u64 = 0;
    let mut i = digits.len();
    while i > 0 {
        i -= 1;
        let d = digits[i];
        debug_assert!(d <= 9);
        c = c * 10 + d as u64;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::decimal_digit_count;
    use proptest::prelude::*;

    #[test]
    fn zero_yields_one_digit_and_zero_fill() {
        let mut buf = [0u8; PRECISION];
        let count = coefficient_to_digits(0, &mut buf);
        assert_eq!(count, 1);
        assert_eq!(buf, [0u8; PRECISION]);
    }

    #[test]
    fn powers_of_ten() {
        for k in 0u32..=15 {
            let c: u64 = 10u64.pow(k);
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            assert_eq!(count, (k + 1) as usize, "10^{k}");
            assert_eq!(buf[k as usize], 1, "10^{k}");
            for (i, &b) in buf.iter().enumerate() {
                if i != k as usize {
                    assert_eq!(b, 0, "10^{k} stray digit at {i}");
                }
            }
        }
    }

    #[test]
    fn one_below_each_power_is_all_nines() {
        for k in 1u32..=16 {
            let c: u64 = 10u64.pow(k) - 1;
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            assert_eq!(count, k as usize, "10^{k} - 1");
            for &b in &buf[..k as usize] {
                assert_eq!(b, 9, "10^{k} - 1 not all nines");
            }
        }
    }

    #[test]
    fn max_coefficient_fills_precision() {
        let c: u64 = 10u64.pow(16) - 1;
        let mut buf = [0u8; PRECISION];
        let count = coefficient_to_digits(c, &mut buf);
        assert_eq!(count, PRECISION);
        assert_eq!(buf, [9u8; PRECISION]);
    }

    #[test]
    fn digits_to_coefficient_inverse() {
        for c in [
            0u64,
            1,
            9,
            10,
            99,
            100,
            1_234_567_890,
            10u64.pow(15),
            10u64.pow(16) - 1,
        ] {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            assert_eq!(
                digits_to_coefficient(&buf[..count]),
                c,
                "round-trip mismatch for {c}"
            );
            assert_eq!(
                digits_to_coefficient(&buf),
                c,
                "padded round-trip mismatch for {c}"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2048))]

        #[test]
        fn round_trip(c in 0u64..=10u64.pow(16) - 1) {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            prop_assert!(count >= 1);
            prop_assert!(count <= PRECISION);
            prop_assert_eq!(digits_to_coefficient(&buf[..count]), c);
            prop_assert_eq!(digits_to_coefficient(&buf), c);
        }

        #[test]
        fn count_agrees_with_decimal_digit_count(c in 0u64..=10u64.pow(16) - 1) {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            prop_assert_eq!(count as u32, decimal_digit_count(c));
        }

        #[test]
        fn digits_in_range(c in 0u64..=10u64.pow(16) - 1) {
            let mut buf = [0u8; PRECISION];
            coefficient_to_digits(c, &mut buf);
            for &d in &buf {
                prop_assert!(d <= 9);
            }
        }
    }
}
