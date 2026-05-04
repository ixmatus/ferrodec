//! Criterion benches for the IEEE 754 core ops on `Decimal128`.
//!
//! Run with `cargo bench --bench core_ops` (default features only —
//! transcendentals and binary-float live in their own bench files).
//!
//! These benches exist primarily to surface regressions across
//! commits, not to micro-benchmark hot paths. Each input set is a
//! small mix of "easy" and "hard" cases:
//!   * **easy**: small integers, no rounding work.
//!   * **medium**: 34-digit coefficients with moderate alignment.
//!   * **hard**: maximum-alignment shifts that exercise the U384
//!     buffer in fma and the sticky-bit / sub-ULP paths in addsub.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use ferrodec::{Decimal128, RoundingMode};

const RM: RoundingMode = RoundingMode::NearestEven;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RM).unwrap().0
}

fn d_set() -> [Decimal128; 6] {
    [
        parse("1"),
        parse("3.14159265358979323846264338327950288"),
        parse("12345.6789"),
        parse("9.999999999999999999999999999999999e6144"),
        parse("1.234567890123456789012345678901234e-30"),
        parse("-7.5"),
    ]
}

fn add_bench(c: &mut Criterion) {
    let xs = d_set();
    c.bench_function("add", |b| {
        b.iter_batched(
            || (xs, 0u8),
            |(xs, mut i)| {
                for _ in 0..xs.len() {
                    let a = xs[i as usize % xs.len()];
                    let b = xs[(i as usize + 3) % xs.len()];
                    black_box(a.add(b, RM));
                    i = i.wrapping_add(1);
                }
            },
            BatchSize::SmallInput,
        );
    });
}

fn sub_bench(c: &mut Criterion) {
    let xs = d_set();
    c.bench_function("sub", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.sub(x, RM));
                }
            }
        });
    });
}

fn mul_bench(c: &mut Criterion) {
    let xs = d_set();
    c.bench_function("mul", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.mul(x, RM));
                }
            }
        });
    });
}

fn div_bench(c: &mut Criterion) {
    let xs = d_set();
    c.bench_function("div", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    black_box(a.div(x, RM));
                }
            }
        });
    });
}

fn sqrt_bench(c: &mut Criterion) {
    let inputs = [
        parse("2"),
        parse("3.14159265358979323846264338327950288"),
        parse("1e30"),
        parse("0.0001"),
        parse("123456789012345678.0123456789012345"),
    ];
    c.bench_function("sqrt", |b| {
        b.iter(|| {
            for &x in &inputs {
                black_box(x.sqrt(RM));
            }
        });
    });
}

fn fma_bench(c: &mut Criterion) {
    let xs = d_set();
    c.bench_function("fma", |b| {
        b.iter(|| {
            for &a in &xs {
                for &x in &xs {
                    for &y in &xs {
                        black_box(a.fma(x, y, RM));
                    }
                }
            }
        });
    });
}

criterion_group!(
    benches,
    add_bench,
    sub_bench,
    mul_bench,
    div_bench,
    sqrt_bench,
    fma_bench
);
criterion_main!(benches);
