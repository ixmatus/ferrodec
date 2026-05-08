//! Criterion benches for parsing, formatting, and integer conversions.
//!
//! Run with `cargo bench --features=fmt --bench conversions`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ferrodec::{Decimal128, RoundingMode};

const RM: RoundingMode = RoundingMode::NearestEven;

fn parse_bench(c: &mut Criterion) {
    let strings = [
        "1",
        "3.14159265358979323846264338327950288",
        "1.234567890123456789012345678901234e-30",
        "9.999999999999999999999999999999999e6144",
        "-123456789",
        "0.000001",
    ];
    c.bench_function("parse_str", |b| {
        b.iter(|| {
            for s in &strings {
                black_box(Decimal128::parse_str(s, RM).unwrap());
            }
        });
    });
}

fn format_bench(c: &mut Criterion) {
    extern crate alloc;
    use alloc::string::String;
    let parse = |s: &str| Decimal128::parse_str(s, RM).unwrap().0;
    let xs = [
        parse("3.14159265358979323846264338327950288"),
        parse("1e-30"),
        parse("9.999999999999999999999999999999999e6144"),
        parse("-7.5"),
        parse("0.0001"),
    ];
    c.bench_function("format", |b| {
        b.iter(|| {
            let mut sink: String = String::new();
            for &x in &xs {
                use core::fmt::Write;
                sink.clear();
                write!(&mut sink, "{x}").unwrap();
                black_box(&sink);
            }
        });
    });
}

fn from_i128_bench(c: &mut Criterion) {
    let ints: [i128; 6] = [
        0,
        1,
        -1234567890,
        i128::MAX,
        i128::MIN,
        9_999_999_999_999_999_999_999_999_999_999_999_i128,
    ];
    c.bench_function("from_i128", |b| {
        b.iter(|| {
            for &n in &ints {
                black_box(Decimal128::from_i128(n, RM));
            }
        });
    });
}

fn from_i32_bench(c: &mut Criterion) {
    let ints: [i32; 5] = [0, 1, -1234567, i32::MAX, i32::MIN];
    c.bench_function("from_i32", |b| {
        b.iter(|| {
            for &n in &ints {
                black_box(Decimal128::from_i32(n));
            }
        });
    });
}

fn from_u32_bench(c: &mut Criterion) {
    let ints: [u32; 5] = [0, 1, 1234567, u32::MAX, u32::MAX / 3];
    c.bench_function("from_u32", |b| {
        b.iter(|| {
            for &n in &ints {
                black_box(Decimal128::from_u32(n));
            }
        });
    });
}

fn from_u64_bench(c: &mut Criterion) {
    let ints: [u64; 5] = [0, 1, 1234567890, u64::MAX, 9_876_543_210_123_456_789];
    c.bench_function("from_u64", |b| {
        b.iter(|| {
            for &n in &ints {
                black_box(Decimal128::from_u64(n));
            }
        });
    });
}

fn to_i64_bench(c: &mut Criterion) {
    let parse = |s: &str| Decimal128::parse_str(s, RM).unwrap().0;
    let xs = [
        parse("0"),
        parse("123456789"),
        parse("-9876543210"),
        parse("1.5"),
        parse("9.999999999999999999999999999999999e18"),
    ];
    c.bench_function("to_i64", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.to_i64(RM));
            }
        });
    });
}

fn to_i32_bench(c: &mut Criterion) {
    let parse = |s: &str| Decimal128::parse_str(s, RM).unwrap().0;
    let xs = [
        parse("0"),
        parse("123456"),
        parse("-1234567"),
        parse("1.5"),
        parse("2147483647"),
    ];
    c.bench_function("to_i32", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.to_i32(RM));
            }
        });
    });
}

fn to_u64_bench(c: &mut Criterion) {
    let parse = |s: &str| Decimal128::parse_str(s, RM).unwrap().0;
    let xs = [
        parse("0"),
        parse("12345"),
        parse("9876543210"),
        parse("1.5"),
        parse("1.844674407370955161e19"),
    ];
    c.bench_function("to_u64", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.to_u64(RM));
            }
        });
    });
}

fn to_u128_bench(c: &mut Criterion) {
    let parse = |s: &str| Decimal128::parse_str(s, RM).unwrap().0;
    let xs = [
        parse("0"),
        parse("12345"),
        parse("9876543210"),
        parse("1.5"),
        parse("3.402823669209384634633746074317682e38"),
    ];
    c.bench_function("to_u128", |b| {
        b.iter(|| {
            for &x in &xs {
                black_box(x.to_u128(RM));
            }
        });
    });
}

criterion_group!(
    benches,
    parse_bench,
    format_bench,
    from_i128_bench,
    from_i32_bench,
    from_u32_bench,
    from_u64_bench,
    to_i64_bench,
    to_i32_bench,
    to_u64_bench,
    to_u128_bench,
);
criterion_main!(benches);
