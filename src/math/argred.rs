//! Re-export shim: the Payne-Hanek argument-reduction kernel moved to
//! ferrodec-transcend (P0a.2 c7). `reduce()` is re-exported under the
//! original `crate::math::argred` path so the still-in-core sincos
//! compiles unchanged; its `Decimal128` reduction tests stay here as
//! the byte-identical regression gate.

pub(crate) use ferrodec_transcend::argred::reduce;

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use crate::decimal::Decimal128;
    use crate::math::extended::Extended;
    use crate::status::RoundingMode;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn r_to_decimal(r: Extended) -> Decimal128 {
        let (d, _) = r.to_format::<Decimal128>(0, RoundingMode::NearestEven);
        d
    }

    #[test]
    fn small_input_no_reduction() {
        let x = parse("0.01");
        let (k, r, _) = reduce(x);
        assert_eq!(k, 0);
        let r_d = r_to_decimal(r);
        let (cmp, _) = r_d.partial_cmp(x);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn reduces_pi() {
        // x = π. x · 2/π = 2.0 (exactly, mathematically). With rounded
        // π input we expect k = 2, r ≈ 0.
        let x = parse("3.141592653589793238462643383279503");
        let (k, r, _) = reduce(x);
        assert_eq!(k, 2);
        // |r| should be tiny (residual of the parse rounding × π/2).
        let abs_r = r_to_decimal(r).abs();
        let bound = parse("1e-30");
        let (cmp, _) = abs_r.partial_cmp(bound);
        assert!(
            matches!(cmp, Some(core::cmp::Ordering::Less)),
            "expected tiny r, got {abs_r:?}"
        );
    }

    #[test]
    fn reduces_half_pi() {
        // x = π/2. x · 2/π = 1.0. k = 1, r ≈ 0.
        let x = parse("1.570796326794896619231321691639751");
        let (k, r, _) = reduce(x);
        assert_eq!(k, 1);
        let abs_r = r_to_decimal(r).abs();
        let bound = parse("1e-30");
        let (cmp, _) = abs_r.partial_cmp(bound);
        assert!(
            matches!(cmp, Some(core::cmp::Ordering::Less)),
            "expected tiny r, got {abs_r:?}"
        );
    }

    #[test]
    fn large_input_in_range() {
        // x = 10^15 — well beyond the |x| ≤ 10^9 cap of the legacy
        // reduction. Just check r is in [-π/4, π/4].
        let x = parse("1e15");
        let (k, r, _) = reduce(x);
        let _ = k;
        let pi_over_four = parse("0.785398163397448309615660845819876");
        let bound = pi_over_four;
        let abs_r = r_to_decimal(r).abs();
        let (cmp, _) = abs_r.partial_cmp(bound);
        assert!(
            matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            ),
            "|r| = {abs_r:?} exceeds π/4 for x = 10^15"
        );
    }

    #[test]
    fn extreme_input_in_range() {
        // Beyond what astro-float can comfortably handle in test, but
        // we can still check |r| ≤ π/4.
        let x = parse("1e3000");
        let (_, r, _) = reduce(x);
        let pi_over_four = parse("0.785398163397448309615660845819876");
        let abs_r = r_to_decimal(r).abs();
        let (cmp, _) = abs_r.partial_cmp(pi_over_four);
        assert!(
            matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            ),
            "|r| = {abs_r:?} exceeds π/4 for x = 1e3000"
        );
    }
}
