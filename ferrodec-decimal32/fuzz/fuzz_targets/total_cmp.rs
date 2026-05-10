//! Fuzz target: arbitrary `(u32, u32)` pairs through `total_cmp`,
//! `partial_cmp`, `compare_total_magnitude`. Verifies totality
//! (`total_cmp` always returns Ordering, never panics) and basic
//! transitivity surrogates.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec_decimal32::Decimal32;

fuzz_target!(|data: (u32, u32, u32)| {
    let (bits_a, bits_b, bits_c) = data;
    let a = Decimal32::from_bits(bits_a);
    let b = Decimal32::from_bits(bits_b);
    let c = Decimal32::from_bits(bits_c);

    // totalOrder: never panics, always returns Ordering.
    let ab = a.total_cmp(b);
    let ba = b.total_cmp(a);
    // Anti-symmetry: a.total_cmp(b) == b.total_cmp(a).reverse()
    assert_eq!(
        ab,
        ba.reverse(),
        "total_cmp anti-symmetry: a={a:?} b={b:?}"
    );
    // Reflexivity: a.total_cmp(a) == Equal.
    assert_eq!(a.total_cmp(a), core::cmp::Ordering::Equal);

    // Transitivity surrogate: if a <= b <= c then a <= c.
    let bc = b.total_cmp(c);
    if ab != core::cmp::Ordering::Greater && bc != core::cmp::Ordering::Greater {
        let ac = a.total_cmp(c);
        assert!(
            ac != core::cmp::Ordering::Greater,
            "total_cmp transitivity violated: a={a:?} b={b:?} c={c:?}"
        );
    }

    // partial_cmp / compare_total_magnitude don't panic.
    let _ = a.partial_cmp(b);
    let _ = a.compare_total_magnitude(b);
});
