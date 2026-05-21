//! Decimal-digit decomposition for the `Decimal128` coefficient.
//!
//! The General Decimal Arithmetic decNumber extension operations
//! (`logical_invert`, `logical_and`, `logical_or`, `logical_xor`,
//! `shift`, `rotate`) work on the coefficient as an array of base-10
//! digits, not as a binary integer. This module gives the kernels a
//! round-trip pair: [`coefficient_to_digits`] writes the digits of a
//! `u128` into a caller-provided `[u8; PRECISION]` buffer (least
//! significant at index 0), [`digits_to_coefficient`] reconstructs the
//! integer.
//!
//! The helpers stay precision-local rather than landing in
//! `ferrodec-ieee` or `ferrodec-multiword`, following the carve-out
//! stated by `ferrodec-ieee/src/digits.rs` for the `u32` / `u64`
//! variants of digit counting: the natural callers of digit
//! decomposition never need to call across formats. Each precision
//! decomposes its own coefficient and reuses none of the others'.
//! See ADR-0031 for the GDA-extension scope this module supports.

/// Decimal128 coefficient precision, in base-10 digits.
pub(crate) const PRECISION: usize = 34;

/// Writes the base-10 digits of `c` into `out` with the least
/// significant digit at index 0. Returns the number of significant
/// digits actually produced.
///
/// The count matches
/// [`ferrodec_ieee::digits::decimal_digit_count_u128`]: `c == 0`
/// yields one digit (the literal zero). Indices `[count..PRECISION]`
/// are zero-filled on return so callers may treat the whole buffer
/// as a fixed-width digit stream when the operation demands it
/// (the logical and shift / rotate kernels rely on this).
///
/// Panics in debug builds if any decimal digit would exceed nine,
/// which is unreachable for a well-formed BID-128 coefficient.
#[allow(dead_code)] // Wired by ADR-0031 op kernels.
pub(crate) const fn coefficient_to_digits(mut c: u128, out: &mut [u8; PRECISION]) -> usize {
    let mut i = 0usize;
    // Emit at least one digit so the literal zero produces a `0` at
    // index 0 and a returned count of 1.
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
    // Zero-fill the unused tail so callers can read the whole buffer
    // as a precision-wide digit stream without tracking `count` in
    // the inner loop.
    while i < PRECISION {
        out[i] = 0;
        i += 1;
    }
    count
}

/// Reconstructs a `u128` coefficient from the digit array produced by
/// [`coefficient_to_digits`], or any slice with the same convention
/// (least significant digit at index 0, each byte in `0..=9`).
///
/// The caller is responsible for ensuring the reconstructed value
/// fits in `u128`. The longest valid input has length [`PRECISION`]
/// and bounds the result by `10^PRECISION - 1`, well below `u128::MAX`.
/// Bytes outside `0..=9` panic in debug builds; release builds
/// produce an unspecified but well-defined `u128`.
#[allow(dead_code)] // Wired by ADR-0031 op kernels.
pub(crate) const fn digits_to_coefficient(digits: &[u8]) -> u128 {
    let mut c: u128 = 0;
    let mut i = digits.len();
    while i > 0 {
        i -= 1;
        let d = digits[i];
        debug_assert!(d <= 9);
        c = c * 10 + d as u128;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrodec_ieee::decimal_digit_count_u128;
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
        for k in 0u32..=33 {
            let c: u128 = 10u128.pow(k);
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
        for k in 1u32..=34 {
            let c: u128 = 10u128.pow(k) - 1;
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
        // 10^34 - 1 is the largest representable Decimal128
        // coefficient; the count reaches PRECISION and the buffer is
        // saturated with nines.
        let c: u128 = 10u128.pow(34) - 1;
        let mut buf = [0u8; PRECISION];
        let count = coefficient_to_digits(c, &mut buf);
        assert_eq!(count, PRECISION);
        assert_eq!(buf, [9u8; PRECISION]);
    }

    #[test]
    fn digits_to_coefficient_inverse() {
        for c in [
            0u128,
            1,
            9,
            10,
            99,
            100,
            1_234_567_890,
            10u128.pow(33),
            10u128.pow(34) - 1,
        ] {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            assert_eq!(
                digits_to_coefficient(&buf[..count]),
                c,
                "round-trip mismatch for {c}"
            );
            // Reading the whole zero-padded buffer must also reconstruct
            // exactly, because higher-order zeros do not change the value.
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
        fn round_trip(c in 0u128..=10u128.pow(34) - 1) {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            prop_assert!(count >= 1);
            prop_assert!(count <= PRECISION);
            prop_assert_eq!(digits_to_coefficient(&buf[..count]), c);
            prop_assert_eq!(digits_to_coefficient(&buf), c);
        }

        #[test]
        fn count_agrees_with_decimal_digit_count(c in 0u128..=10u128.pow(34) - 1) {
            let mut buf = [0u8; PRECISION];
            let count = coefficient_to_digits(c, &mut buf);
            prop_assert_eq!(count as u32, decimal_digit_count_u128(c));
        }

        #[test]
        fn digits_in_range(c in 0u128..=10u128.pow(34) - 1) {
            let mut buf = [0u8; PRECISION];
            coefficient_to_digits(c, &mut buf);
            for &d in &buf {
                prop_assert!(d <= 9);
            }
        }
    }
}
