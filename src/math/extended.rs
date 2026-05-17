//! Re-export shim: Extended now lives in ferrodec-transcend (P0a.2 c3). The Decimal128 round-trip oracle stays here, parameterized at F=Decimal128, as part of the byte-identical regression gate.

pub(crate) use ferrodec_transcend::extended::Extended;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decimal::Decimal128;
    use ferrodec_ieee::RoundingMode;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    /// Sweep round-trip over a deterministic grid: every combination of
    /// representative coefficients × representative quanta. Pins the
    /// `Decimal128 → Extended → Decimal128` boundary as exact for any
    /// finite input that already fits in 34 digits. The transcendental
    /// kernels rely on this contract; without it, `to_format` could
    /// silently shift cohort or lose precision on the way back in.
    #[test]
    fn round_trip_decimal128_sweep() {
        // A spread of coefficients across the 1-to-34-digit range,
        // including the boundary at 10^34 - 1.
        let coefs: [u128; 9] = [
            1,
            7,
            10,
            12_345_u128,
            1_000_000_u128,
            10u128.pow(15),
            10u128.pow(20),
            10u128.pow(33),
            10u128.pow(34) - 1,
        ];
        // Quanta covering subnormal, normal, large-positive, and the
        // `BIASED_EXP_MAX` boundary.
        let quanta: [i32; 8] = [
            -6176, // smallest representable
            -100, -34, -1, 0, 1, 100, 6111, // largest representable
        ];
        for &coef in &coefs {
            for &q in &quanta {
                for &sign in &[false, true] {
                    if coef == 0 && sign {
                        continue;
                    }
                    let signed_coef = if sign { -(coef as i128) } else { coef as i128 };
                    let d = match Decimal128::try_new(signed_coef, q) {
                        Ok(d) => d,
                        Err(_) => continue, // out-of-range combinations: skip silently
                    };
                    let e = Extended::from_format::<Decimal128>(d);
                    let (back, _) = e.to_format::<Decimal128>(0, RoundingMode::NearestEven);
                    let (cmp, _) = back.partial_cmp(d);
                    assert_eq!(
                        cmp,
                        Some(core::cmp::Ordering::Equal),
                        "Decimal128 → Extended → Decimal128 mismatch: \
                         coef={signed_coef}, q={q}, d={d:?}, back={back:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn round_trip_decimal128() {
        for s in &[
            "0",
            "1",
            "-1",
            "1.5",
            "12345.6789",
            "-0.000001",
            "1e30",
            "-1e-30",
        ] {
            let d = parse(s);
            let e = Extended::from_format::<Decimal128>(d);
            let (back, _) = e.to_format::<Decimal128>(0, RoundingMode::NearestEven);
            let (cmp, _) = back.partial_cmp(d);
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "roundtrip failed for {s}"
            );
        }
    }
}
