//! Criterion benches for the comparison APIs: `partial_cmp` (IEEE
//! numeric ordering), `total_cmp` (IEEE 754:2019 §5.10 totalOrder
//! predicate), and `compare_total_magnitude` (totalOrder on
//! magnitudes).
//!
//! Run with `cargo bench --bench comparison --features=fmt`.
//!
//! These exercise the comparison hot path in two patterns:
//! * **All-pairs**: every input compared with every other (small N²
//!   shape; biases toward branch-prediction-friendly access).
//! * **Sort**: a `Vec<Decimal128>` sorted via `total_cmp`. Closer to
//!   real workloads that use `Decimal128` as a key.
//!
//! Inputs span finite, ±0, ±Inf, and NaN-with-payload to exercise
//! every branch in the comparator.
#![cfg(feature = "fmt")]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ferrodec::{Decimal128, RoundingMode};

const RM: RoundingMode = RoundingMode::NearestEven;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RM).unwrap().0
}

fn cmp_set() -> [Decimal128; 12] {
    [
        parse("0"),
        parse("-0"),
        parse("1"),
        parse("-1"),
        parse("3.14159265358979323846264338327950288"),
        parse("12345.6789"),
        parse("9.999999999999999999999999999999999e6144"),
        parse("1.234567890123456789012345678901234e-30"),
        parse("Infinity"),
        parse("-Infinity"),
        parse("NaN42"),
        parse("sNaN17"),
    ]
}

fn partial_cmp_bench(c: &mut Criterion) {
    let xs = cmp_set();
    c.bench_function("partial_cmp_pairwise", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.partial_cmp(x));
                }
            }
        });
    });
}

fn total_cmp_bench(c: &mut Criterion) {
    let xs = cmp_set();
    c.bench_function("total_cmp_pairwise", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.total_cmp(x));
                }
            }
        });
    });
}

fn compare_total_magnitude_bench(c: &mut Criterion) {
    let xs = cmp_set();
    c.bench_function("compare_total_magnitude_pairwise", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.compare_total_magnitude(x));
                }
            }
        });
    });
}

/// Sort a 64-element `Vec<Decimal128>` via `total_cmp`. Each iteration
/// rebuilds the vec (cloned from a precomputed seed) so the previous
/// iteration's sorted state doesn't bias the next.
fn total_cmp_sort_bench(c: &mut Criterion) {
    extern crate alloc;
    use alloc::vec::Vec;

    // 64-element seed: 16 finite values, 16 negatives, 16 specials,
    // 16 mixed. Built so the comparator hits every branch.
    let xs = cmp_set();
    let seed: Vec<Decimal128> = (0..64).map(|i| xs[i % xs.len()]).collect();

    c.bench_function("total_cmp_sort_64", |b| {
        b.iter(|| {
            let mut v = seed.clone();
            v.sort_by(|a, b| a.total_cmp(*b));
            black_box(v);
        });
    });
}

criterion_group!(
    benches,
    partial_cmp_bench,
    total_cmp_bench,
    compare_total_magnitude_bench,
    total_cmp_sort_bench,
);
criterion_main!(benches);
