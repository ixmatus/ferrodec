//! Decimal-digit-count helpers shared across the ferrodec family.
//!
//! Every sibling crate's arithmetic kernels compute the number of
//! decimal digits in a coefficient to bound alignment shifts,
//! scaling factors, and digit-drop counts. This module provides a
//! single source of truth for the `u128` variant, used by the
//! sibling crates' u128-working-width kernels (addsub, mul, div,
//! rem, sqrt, fma).
//!
//! The `u32` and `u64` variants stay precision-local (in each
//! sibling's `bid.rs`) because their natural callers — pack /
//! unpack helpers, BID layout — never need to call across
//! precisions.

/// Decimal digit count of `n`. Returns 1 for `n == 0` (matching
/// the GDA convention: a zero coefficient is one digit wide).
///
/// Implemented via [`u128::ilog10`], which is `const fn` and
/// branchless on every platform Rust supports.
#[must_use]
pub const fn decimal_digit_count_u128(n: u128) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_one_digit() {
        assert_eq!(decimal_digit_count_u128(0), 1);
    }

    #[test]
    fn powers_of_ten() {
        for k in 0u32..=38 {
            let n: u128 = 10u128.pow(k);
            assert_eq!(decimal_digit_count_u128(n), k + 1, "10^{k}");
        }
    }

    #[test]
    fn one_below_each_power() {
        for k in 1u32..=38 {
            let n: u128 = 10u128.pow(k) - 1;
            assert_eq!(decimal_digit_count_u128(n), k, "10^{k} − 1");
        }
    }

    #[test]
    fn u128_max() {
        // u128::MAX = 340_282_366_920_938_463_463_374_607_431_768_211_455
        // ≈ 3.4 × 10^38, so 39 digits.
        assert_eq!(decimal_digit_count_u128(u128::MAX), 39);
    }
}
