# ferrodec-decimal

Arbitrary precision decimal arithmetic in pure Rust, to the General Decimal
Arithmetic Specification (Mike Cowlishaw's `decNumber` and `decTest` family).
This is the parent specification the fixed width IEEE 754-2019 formats in the
rest of the ferrodec workspace derive from, lifted to unbounded precision. It
is `no_std`, but unlike its fixed width siblings it requires `alloc` and a
global allocator: the coefficient is a growable heap integer, so this is the
workspace's "needs an allocator" tier.

## What ferrodec-decimal is

A value is a finite number (sign, an integer coefficient of any length, and an
exponent), a signed infinity, or a quiet or signaling NaN with a diagnostic
payload. The coefficient is held as an integer, so trailing zeros are
significant and the full cohort of a value is preserved: `1.0` and `1.00` are
distinct, and a zero carries its own sign and quantum.

Two design choices carry over from the fixed formats, and one is new.

1. **Per operation status, never global flags.** Every operation returns
   `(Decimal, Status)`, the General Decimal Arithmetic conditions that one call
   raised. Nothing reads or writes a thread local register.
2. **Explicit context.** Each operation takes a `Context` by reference: the
   working precision, the adjusted exponent bounds, the rounding mode, and the
   clamp flag. The eight General Decimal Arithmetic rounding modes are all
   supported, the five shared with IEEE plus `HalfDown`, `Up`, and `ZeroFiveUp`.
3. **A growable coefficient.** Precision is bounded only by the context and by
   memory, not by a fixed format width.

## The operation surface

The whole General Decimal Arithmetic numerical specification. The core
arithmetic: `add`, `subtract`, `multiply`, `divide`, `divideInteger`, the
remainder family, `fma`, correctly rounded `squareRoot`, `quantize`, round to
integral, `reduce`, the sign operations, `compare`, `compareTotal`, `max`,
`min`, and the copy operations. And the four transcendentals: correctly rounded
`exp`, `ln`, `log10`, and `power`. `exp` / `ln` / `log10` round half-even like
`squareRoot`; `power` rounds with the context's rounding mode and is correctly
rounded by construction, stronger than the reference (which is only almost always
correctly rounded). An optional `interop` feature converts losslessly from and
roundingly to the fixed width `Decimal32` / `Decimal64` / `Decimal128`.

The crate stays on the `0.x` line pending the final API settle and a performance
pass: the public surface may still change, and the high-precision `ln` path is
not yet optimised. See `docs/decisions/0040-arbitrary-precision-transcendentals.md`
for the transcendental contract and `0038-arbitrary-precision-decimal.md` for the
overall design.

## Quick start

```rust
use ferrodec_decimal::{Context, Decimal, Rounding};

// A context with 50 working digits, rounding half to even.
let ctx = Context::new(50, 1_000_000, -1_000_000, Rounding::HalfEven);

let a = Decimal::parse_str("1").unwrap();
let b = Decimal::parse_str("3").unwrap();
let (third, status) = a.divide(&b, &ctx);

// 1 / 3 to 50 digits: "0." followed by fifty threes.
assert_eq!(third.digits(), Some(50));
assert!(status.inexact());
```

## Verification

Every operation is checked cohort-exact against CPython's `decimal` module,
which is libmpdec, the General Decimal Arithmetic reference implementation, over
a deterministic sweep that reaches the special values and the overflow,
subnormal, and clamp boundaries (`cargo test --features differential`). The
coefficient bignum is checked against `u128` ground truth and reconstruction
identities.

## How this is developed

ferrodec, including this crate, is developed with an open disclosure of the
process; see the "How ferrodec is developed" section of the workspace root
`README.md`, which covers every member crate.
