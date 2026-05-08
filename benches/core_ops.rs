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

// ---------------------------------------------------------------------------
// Targeted benches for the perf pass of the 1.11.0 cycle. These isolate
// specific hot paths so candidate optimizations can be measured against a
// stable mix rather than averaged into the broad `add` / `mul` / `div` mixes
// above.

/// Pairs with widely-separated quanta to exercise the alignment +
/// sub-ULP path in `addsub`. Each pair has |Δexp| ≈ 30, well past the
/// `ALIGN_LIMIT = 43` threshold for some pairs and well within for
/// others — a representative mix of the two regimes.
fn alignment_heavy_pairs() -> [(Decimal128, Decimal128); 6] {
    [
        (parse("1"), parse("1e-30")),
        (parse("1e6"), parse("1e-25")),
        (
            parse("1.234567890123456789012345678901234e10"),
            parse("5e-25"),
        ),
        (
            parse("9.999999999999999999999999999999999e0"),
            parse("1e-50"),
        ),
        (parse("-3.14159e-3"), parse("7.2e-80")),
        (parse("1e0"), parse("-1e-100")),
    ]
}

fn add_alignment_heavy_bench(c: &mut Criterion) {
    let pairs = alignment_heavy_pairs();
    c.bench_function("add_alignment_heavy", |b| {
        b.iter(|| {
            for &(a, x) in &pairs {
                black_box(a.add(x, RM));
            }
        });
    });
}

fn sub_alignment_heavy_bench(c: &mut Criterion) {
    let pairs = alignment_heavy_pairs();
    c.bench_function("sub_alignment_heavy", |b| {
        b.iter(|| {
            for &(a, x) in &pairs {
                black_box(a.sub(x, RM));
            }
        });
    });
}

/// Two 34-digit coefficients to force the maximum U256 product width.
fn mul_full_precision_bench(c: &mut Criterion) {
    let inputs = [
        (
            parse("9.999999999999999999999999999999999e10"),
            parse("9.999999999999999999999999999999999e10"),
        ),
        (
            parse("1.234567890123456789012345678901234e0"),
            parse("9.876543210987654321098765432109876e0"),
        ),
        (
            parse("3.141592653589793238462643383279502e3"),
            parse("2.718281828459045235360287471352662e3"),
        ),
    ];
    c.bench_function("mul_full_precision", |b| {
        b.iter(|| {
            for &(a, x) in &inputs {
                black_box(a.mul(x, RM));
            }
        });
    });
}

/// One tiny / one huge operand to force the long-division path's
/// behaviour on widely-separated magnitudes.
fn div_magnitude_extreme_bench(c: &mut Criterion) {
    let inputs = [
        (parse("1e30"), parse("1e-30")),
        (parse("1e-30"), parse("1e30")),
        (
            parse("9.999999999999999999999999999999999e6000"),
            parse("1.234567890123456789012345678901234e-6000"),
        ),
        (
            parse("1.234567890123456789012345678901234e-6000"),
            parse("9.999999999999999999999999999999999e6000"),
        ),
    ];
    c.bench_function("div_magnitude_extreme", |b| {
        b.iter(|| {
            for &(a, x) in &inputs {
                black_box(a.div(x, RM));
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
    fma_bench,
    add_alignment_heavy_bench,
    sub_alignment_heavy_bench,
    mul_full_precision_bench,
    div_magnitude_extreme_bench,
);
criterion_main!(benches);
