//! Lockstep pin: `margin::boundary_distances` against the corpus
//! generator's margin definitions.
//!
//! `tools/gen_margin_lockstep.py` evaluated
//! `gen_transcend_vectors::{directed_margin, tie_margin}` on each
//! row's exact value and asserted the integer tail formulation against
//! them before emitting; this test pins the Rust integers to the
//! emitted ones exactly. Together the two halves prove the campaign
//! filter and the corpus generator measure the same distance.
//!
//! Counts are pinned exactly per precision bucket (house rule: never
//! an aggregate floor).

use ferrodec_campaign::margin::boundary_distances;
use ferrodec_campaign::u256_from_decimal;
use ferrodec_transcend::extended::Extended;

const EXPECTED_PER_PREC: [(u32, usize); 3] = [(7, 22), (16, 22), (34, 22)];

#[test]
fn lockstep_with_python_margins() {
    let raw = include_str!("vectors/margin_lockstep.txt");
    let mut counts = [0usize; 3];
    for line in raw.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        assert_eq!(f.len(), 7, "malformed row: {line}");
        let prec: u32 = f[0].parse().unwrap();
        let exp: i32 = f[1].parse().unwrap();
        let coef = u256_from_decimal(f[2]).expect("coef50");
        let grid_x2 = u256_from_decimal(f[3]).expect("grid_x2");
        let tie_x2 = u256_from_decimal(f[4]).expect("tie_x2");
        let m_grid: f64 = f[5].parse().unwrap();
        let m_tie: f64 = f[6].parse().unwrap();

        let x = Extended {
            coef,
            exp,
            sign: false,
        };
        let d = boundary_distances(x, prec).expect("nonzero, in-width row");

        assert_eq!(d.w, 50 - prec);
        assert!(
            d.grid_x2.cmp(grid_x2).is_eq(),
            "grid_x2 mismatch at prec {prec}: {line}"
        );
        assert!(
            d.tie_x2.cmp(tie_x2).is_eq(),
            "tie_x2 mismatch at prec {prec}: {line}"
        );

        // Loose float corroboration of the human-readable columns
        // (the integers above are the contract).
        let denom = 2.0 * 10f64.powi(d.w as i32);
        let close = |x2: ferrodec_multiword::U256, m: f64| {
            let v = decimal_to_f64(x2) / denom;
            (v - m).abs() <= 1e-9 * (m.abs() + 1e-30)
        };
        assert!(close(d.grid_x2, m_grid), "grid float drift: {line}");
        assert!(close(d.tie_x2, m_tie), "tie float drift: {line}");

        let idx = EXPECTED_PER_PREC
            .iter()
            .position(|&(p, _)| p == prec)
            .expect("unexpected precision bucket");
        counts[idx] += 1;
    }
    for (i, &(p, expected)) in EXPECTED_PER_PREC.iter().enumerate() {
        assert_eq!(counts[i], expected, "row count for prec {p}");
    }
}

/// Approximate f64 of a U256 via its decimal digits (test-only; the
/// exact comparisons above are the contract, this only corroborates
/// the emitted float columns).
fn decimal_to_f64(mut v: ferrodec_multiword::U256) -> f64 {
    let mut digits: Vec<u32> = Vec::new();
    while !v.is_zero() {
        let (q, r) = v.div_rem10();
        digits.push(r);
        v = q;
    }
    digits
        .iter()
        .rev()
        .fold(0f64, |acc, &d| acc * 10.0 + f64::from(d))
}
