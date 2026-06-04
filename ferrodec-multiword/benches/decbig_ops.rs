//! Criterion benches for `DecBig`, the base-`10^9` growable bignum that backs
//! `ferrodec-decimal`'s coefficient.
//!
//! Run with
//! `cargo bench -p ferrodec-multiword --features alloc --bench decbig_ops`.
//!
//! These benches surface regressions and locate the crossover widths for the
//! performance pass (ADR-0043). `mul` is schoolbook and `div_rem` is Knuth
//! Algorithm D, both quadratic in limb count, so each op is swept across digit
//! widths from a single limb up into the high-precision tail where a
//! sub-quadratic algorithm would have to earn its keep. They are not
//! micro-benchmarks of a hot path; they are a stable mix for measuring
//! candidate deltas.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ferrodec_multiword::DecBig;

/// Digit widths spanning the crossover region: one limb (9), a few limbs
/// (18/34/50), then the high-precision tail (100..4000).
const WIDTHS: [usize; 9] = [9, 18, 34, 50, 100, 200, 500, 1000, 4000];

/// A deterministic `n`-digit `DecBig`. The leading digit is nonzero and the
/// repeating pattern never produces an all-zero limb, so the value genuinely
/// occupies `n` decimal digits.
fn digits(n: usize) -> DecBig {
    const PAT: &[u8] = b"1234567890987654321"; // 19-char cycle, leading '1'
    let bytes: Vec<u8> = (0..n).map(|i| PAT[i % PAT.len()]).collect();
    DecBig::from_ascii_digits(&bytes)
}

fn mul_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("mul");
    for &n in &WIDTHS {
        let a = digits(n);
        let b = digits(n + 1); // distinct operand, same width class
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| black_box(black_box(&a).mul(black_box(&b))));
        });
    }
    group.finish();
}

fn div_rem_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("div_rem");
    for &n in &WIDTHS {
        // Dividend ~2n digits over an n-digit divisor: an n-digit quotient,
        // the shape the transcendental kernels divide-to-precision produces.
        let dividend = digits(2 * n);
        let divisor = digits(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| black_box(black_box(&dividend).div_rem(black_box(&divisor))));
        });
    }
    group.finish();
}

fn isqrt_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("isqrt");
    for &n in &WIDTHS {
        let x = digits(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).isqrt()));
        });
    }
    group.finish();
}

fn mul_pow10_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("mul_pow10");
    for &n in &WIDTHS {
        let x = digits(n);
        let k = n as u32; // shift by a full operand width, crossing limbs
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).mul_pow10(black_box(k))));
        });
    }
    group.finish();
}

fn div_rem_pow10_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("div_rem_pow10");
    for &n in &WIDTHS {
        let x = digits(2 * n);
        let k = n as u32;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).div_rem_pow10(black_box(k))));
        });
    }
    group.finish();
}

/// One tiny / one huge operand, mirroring the fixed-format
/// `div_magnitude_extreme` guard: a long quotient from widely separated
/// magnitudes.
fn div_magnitude_extreme_bench(c: &mut Criterion) {
    let huge = digits(4000);
    let small = digits(7);
    c.bench_function("div_magnitude_extreme", |b| {
        b.iter(|| black_box(black_box(&huge).div_rem(black_box(&small))));
    });
}

criterion_group!(
    benches,
    mul_bench,
    div_rem_bench,
    isqrt_bench,
    mul_pow10_bench,
    div_rem_pow10_bench,
    div_magnitude_extreme_bench,
);
criterion_main!(benches);
