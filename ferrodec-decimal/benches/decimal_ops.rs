//! Criterion benches for `ferrodec-decimal`: the core arithmetic and the four
//! transcendentals, swept across context precision.
//!
//! Run with `cargo bench -p ferrodec-decimal --bench decimal_ops`
//! (the default `fmt` feature supplies `parse_str`).
//!
//! The transcendentals are the named hot path for the performance pass: the
//! `ln` / `exp` series cost is roughly cubic in the working precision (ADR-0040,
//! ADR-0043). Inputs are small and fixed; the context precision is the swept
//! dimension, because it drives the working precision and so the cost. The core
//! ops use operands whose digit count tracks the precision, exercising the
//! `DecBig` coefficient at the target width. These benches surface regressions
//! and locate crossover widths, not micro-benchmark a single path.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ferrodec_decimal::{Context, DecBig, Decimal, Rounding};

/// Context precisions spanning typical use (16/34) through the high-precision
/// tail (100/500) where any sub-cubic transcendental must earn its keep.
const PRECISIONS: [u32; 5] = [16, 34, 50, 100, 500];

fn context(precision: u32) -> Context {
    Context::new(precision, 999_999, -999_999, Rounding::HalfEven)
}

fn parse(s: &str) -> Decimal {
    Decimal::parse_str(s).expect("valid literal")
}

/// An `n`-digit coefficient from a repeating nonzero-leading pattern.
fn coeff(n: usize, pat: &[u8]) -> DecBig {
    let bytes: Vec<u8> = (0..n).map(|i| pat[i % pat.len()]).collect();
    DecBig::from_ascii_digits(&bytes)
}

const PAT_A: &[u8] = b"1234567890987654321";
const PAT_B: &[u8] = b"9876543210123456789";

/// An `n`-digit finite decimal with the point placed mid-number.
fn a_wide(n: usize) -> Decimal {
    Decimal::finite(false, coeff(n, PAT_A), -(n as i32 / 2))
}

fn b_wide(n: usize) -> Decimal {
    Decimal::finite(false, coeff(n, PAT_B), -(n as i32 / 2))
}

// --- core arithmetic, operands sized to the precision -----------------------

fn add_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("add");
    for &p in &PRECISIONS {
        let n = p as usize;
        let a = a_wide(n);
        // Offset the second operand's exponent to exercise the alignment path.
        let b = Decimal::finite(false, coeff(n, PAT_B), -(n as i32 / 2) - 3);
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&a).add(black_box(&b), &ctx)));
        });
    }
    group.finish();
}

fn multiply_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("multiply");
    for &p in &PRECISIONS {
        let n = p as usize;
        let a = a_wide(n);
        let b = b_wide(n);
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&a).multiply(black_box(&b), &ctx)));
        });
    }
    group.finish();
}

fn divide_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("divide");
    for &p in &PRECISIONS {
        let n = p as usize;
        let a = a_wide(n);
        let b = b_wide(n); // distinct coefficient: a non-terminating quotient
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&a).divide(black_box(&b), &ctx)));
        });
    }
    group.finish();
}

fn sqrt_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqrt");
    for &p in &PRECISIONS {
        let x = a_wide(p as usize);
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).sqrt(&ctx)));
        });
    }
    group.finish();
}

// --- transcendentals, small fixed inputs, precision-swept -------------------

fn ln_bench(c: &mut Criterion) {
    let x = parse("2");
    let mut group = c.benchmark_group("ln");
    for &p in &PRECISIONS {
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).ln(&ctx)));
        });
    }
    group.finish();
}

fn log10_bench(c: &mut Criterion) {
    let x = parse("7"); // not a power of ten: the general path
    let mut group = c.benchmark_group("log10");
    for &p in &PRECISIONS {
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).log10(&ctx)));
        });
    }
    group.finish();
}

fn exp_bench(c: &mut Criterion) {
    let x = parse("1.5");
    let mut group = c.benchmark_group("exp");
    for &p in &PRECISIONS {
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&x).exp(&ctx)));
        });
    }
    group.finish();
}

fn power_bench(c: &mut Criterion) {
    let base = parse("2");
    let exponent = parse("1.5"); // non-integer: the exp(y*ln x) general path
    let mut group = c.benchmark_group("power");
    for &p in &PRECISIONS {
        let ctx = context(p);
        group.bench_with_input(BenchmarkId::from_parameter(p), &p, |bencher, _| {
            bencher.iter(|| black_box(black_box(&base).power(black_box(&exponent), &ctx)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    add_bench,
    multiply_bench,
    divide_bench,
    sqrt_bench,
    ln_bench,
    log10_bench,
    exp_bench,
    power_bench,
);
criterion_main!(benches);
