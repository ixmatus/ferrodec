//! Production crosscheck for `Extended::near_rounding_boundary` (M2,
//! fd-4zo.10, ADR-0059).
//!
//! The transcend crate pins the predicate against a widened reference
//! rounder; this suite pins it against the production `Decimal128`
//! rounder itself. Predicate `false` must mean every value within the
//! closed ±budget bracket (budget units in the last place of the
//! 50-digit working value) packs to an identical `(bits, status)` pair
//! through `Extended::to_format` in **all five** rounding modes;
//! predicate `true` at `budget − 1` must come with a differing witness
//! inside the bracket (the one unit of slack covers the closed-bracket
//! knife edge, where the predicate is deliberately conservative).

#![cfg(any(feature = "trig", feature = "exp-log"))]

use ferrodec::Decimal128;
use ferrodec_ieee::RoundingMode;
use ferrodec_transcend::extended::Extended;
use proptest::prelude::*;

const MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

/// Build the `Extended` whose 50-digit coefficient is
/// `prefix(34 digits) · 10^16 + tail(16 digits) + delta`, via the
/// public `parse_str` seam (the coefficient halves each fit `u128`;
/// `|delta| < 10^16` borrows or carries across the split).
fn build(prefix: u128, tail: u128, delta: i128, exp: i32, sign: bool) -> Extended {
    const FIELD: i128 = 10i128.pow(16);
    let mut hi = prefix;
    let mut lo = tail as i128 + delta;
    if lo < 0 {
        hi -= 1;
        lo += FIELD;
    } else if lo >= FIELD {
        hi += 1;
        lo -= FIELD;
    }
    let s = format!(
        "{}{}{:016}e{}",
        if sign { "-" } else { "" },
        hi,
        lo as u128,
        exp
    );
    Extended::parse_str(&s)
}

proptest! {
    #[test]
    fn predicate_sound_and_complete_vs_production_rounder(
        prefix in 10u128.pow(33)..10u128.pow(34),
        tail in 0..10u128.pow(16),
        // The floor −6214 keeps the drop position ≤ 38 digits so every
        // boundary distance below fits u128; deeper drops are covered
        // by the transcend-side reference-rounder property test.
        exp in -6214i32..=100,
        sign: bool,
        budget in 1u128..=1_000_000_000_000u128,
    ) {
        let v = build(prefix, tail, 0, exp, sign);
        let near = v.near_rounding_boundary::<Decimal128>(budget);

        // Candidate offsets: bracket endpoints, unit sidesteps, and
        // every boundary landing (with its unit sidesteps) that lies
        // inside the bracket.
        let mut offs: Vec<i128> = vec![0, 1, -1, budget as i128, -(budget as i128)];
        {
            let drop = 16u32.max(u32::try_from((-6176 - exp).max(0)).unwrap());
            prop_assert!(drop <= 38, "exp floor keeps the drop in u128 range");
            // Dropped-tail value of the 50-digit coefficient in u128:
            // low `drop` digits, spliced from the two decimal halves.
            let t: u128 = (prefix % 10u128.pow(drop - 16)) * 10u128.pow(16) + tail;
            let field = 10u128.pow(drop);
            let half = field / 2;
            let mut push_if_small = |dist: u128, negative: bool| {
                if dist <= budget {
                    let mag = dist as i128;
                    let signed_off = if negative { -mag } else { mag };
                    for cand in [signed_off, signed_off - 1, signed_off + 1] {
                        if cand.unsigned_abs() <= budget {
                            offs.push(cand);
                        }
                    }
                }
            };
            push_if_small(t, true);
            push_if_small(field - t, false);
            push_if_small(t.abs_diff(half), t > half);
        }

        let mut any_diff = false;
        for &d in &offs {
            let w = build(prefix, tail, d, exp, sign);
            for rm in MODES {
                let (base_d, base_s) = v.to_format::<Decimal128>(0, rm);
                let (out_d, out_s) = w.to_format::<Decimal128>(0, rm);
                if (out_d.to_bits(), out_s) != (base_d.to_bits(), base_s) {
                    any_diff = true;
                    prop_assert!(
                        near,
                        "unsound: predicate false but offset {d} changes {rm:?}: \
                         {:#034x}/{base_s:?} -> {:#034x}/{out_s:?}",
                        base_d.to_bits(),
                        out_d.to_bits()
                    );
                }
            }
        }
        if budget > 1 && v.near_rounding_boundary::<Decimal128>(budget - 1) {
            prop_assert!(
                any_diff,
                "incomplete: predicate true at budget-1 with no witness within the bracket"
            );
        }
    }
}
