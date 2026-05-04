//! Criterion benches for the transcendentals (gated by the
//! `transcendentals` feature).
//!
//! Run with
//! `cargo bench --features=transcendentals --bench transcendentals`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ferrodec::{Decimal128, RoundingMode};

const RM: RoundingMode = RoundingMode::NearestEven;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RM).unwrap().0
}

fn typical_inputs() -> [Decimal128; 4] {
    [
        parse("0.5"),
        parse("1.234567890123456789"),
        parse("3.14159265358979323846264338327950288"),
        parse("100"),
    ]
}

fn exp_bench(c: &mut Criterion) {
    let xs = typical_inputs();
    c.bench_function("exp", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.exp(RM));
            }
        });
    });
}

fn ln_bench(c: &mut Criterion) {
    let xs = typical_inputs();
    c.bench_function("ln", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.ln(RM));
            }
        });
    });
}

fn log10_bench(c: &mut Criterion) {
    let xs = typical_inputs();
    c.bench_function("log10", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.log10(RM));
            }
        });
    });
}

fn sin_bench(c: &mut Criterion) {
    let xs = [
        parse("0.5"),
        parse("1.234"),
        parse("3.14159265358979"),
        parse("1e15"), // exercises Payne-Hanek path
    ];
    c.bench_function("sin", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.sin(RM));
            }
        });
    });
}

fn cos_bench(c: &mut Criterion) {
    let xs = [
        parse("0.5"),
        parse("1.234"),
        parse("3.14159265358979"),
        parse("1e15"),
    ];
    c.bench_function("cos", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.cos(RM));
            }
        });
    });
}

fn pow_bench(c: &mut Criterion) {
    let pairs = [
        (parse("2"), parse("10")),
        (parse("3.14"), parse("2.71")),
        (parse("0.5"), parse("0.5")),
        (parse("1.0001"), parse("1000")),
    ];
    c.bench_function("pow", |b| {
        b.iter(|| {
            for &(x, y) in &pairs {
                black_box(x.pow(y, RM));
            }
        });
    });
}

criterion_group!(
    benches,
    exp_bench,
    ln_bench,
    log10_bench,
    sin_bench,
    cos_bench,
    pow_bench
);
criterion_main!(benches);
