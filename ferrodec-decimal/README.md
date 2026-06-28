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

Two design choices carry over from the fixed formats, one is new, and one fixes
the value semantics.

1. **Per operation status, never global flags.** Every operation returns
   `(Decimal, Status)`, the General Decimal Arithmetic conditions that one call
   raised. Nothing reads or writes a thread local register.
2. **Explicit context.** Each operation takes a `Context` by reference: the
   working precision, the adjusted exponent bounds, the rounding mode, and the
   clamp flag. The eight General Decimal Arithmetic rounding modes are all
   supported, the five shared with IEEE plus `HalfDown`, `Up`, and `ZeroFiveUp`.
3. **A growable coefficient.** Precision is bounded only by the context and by
   memory, not by a fixed format width.
4. **Representation equality, and total ordering.** `==` (`PartialEq` / `Eq`)
   compares the representation, so the whole cohort matters: `1.0` and `1.00`
   are equal in value but distinct representations and compare unequal. Since
   2.0 (ADR-0055), `Decimal` implements `PartialOrd` / `Ord` as the IEEE 754
   `totalOrder` (the relation `compare_total` exposes), which is the only
   ordering lawful against that structural `Eq`: it returns `Equal` exactly when
   the representations are identical, so `a == b` iff `a.cmp(&b) == Equal`. So
   `<` / `>` is the total order: it distinguishes cohorts (`1.0` ranks above
   `1.00`) and signed zeros, and orders NaNs, rather than being a numeric
   comparison. Numeric, cohort-collapsing comparison stays `compare` /
   `compare_total`. Because `Ord`'s provided `Ord::max` / `Ord::min` shadow a
   context-aware operation at value receivers, the General Decimal Arithmetic
   `max` / `min` are exposed as `maxnum` / `minnum` (IEEE 754-2019
   `maximumNumber` / `minimumNumber`); see ADR-0055, which supersedes the
   no-ordering stance of ADR-0045.

## The operation surface

The whole General Decimal Arithmetic specification. The core arithmetic: `add`,
`subtract`, `multiply`, `divide`, `divideInteger`, the remainder family, `fma`,
correctly rounded `squareRoot`, `quantize`, round to integral, `reduce`, the
sign operations, `compare`, `compareTotal`, `max`, `min`, and the copy
operations. The four transcendentals: correctly rounded `exp`, `ln`, `log10`,
and `power`; `exp` / `ln` / `log10` round half-even like `squareRoot`, and
`power` rounds with the context's rounding mode and is correctly rounded by
construction, stronger than the reference (which is only almost always correctly
rounded). And the miscellaneous tier: the logical `and` / `or` / `xor` /
`invert`, `shift` / `rotate`, `scaleb` / `logb`, `nextPlus` / `nextMinus` /
`nextToward`, `compareSignal`, `compareTotalMagnitude`, `maxMagnitude` /
`minMagnitude`, `sameQuantum`, `class`, the classification predicates, and
`radix`. The Rust methods render these spec names in snake case
(`divide_integer`, `next_toward`, `compare_signal`, `compare_total_magnitude`,
`same_quantum`, and so on). The two exceptions are the `max` and `min`
operations, exposed as `maxnum` / `minnum` so they are not shadowed by the
`Ord::max` / `Ord::min` provided methods (ADR-0055).

Values come from `parse_str`, the exact integer constructors (`from_i64` and
its siblings), or, behind the `binary-float` feature, a lossless `TryFrom<f64>`
/ `TryFrom<f32>` that yields the float's precise decimal value rather than the
shortest one. An optional `interop` feature converts losslessly from and
roundingly to the fixed-width `Decimal32` / `Decimal64` / `Decimal128`; the
narrowing direction takes the five IEEE rounding modes, since the three GDA-only
modes do not apply to a format that does not define them (ADR-0005).

The operation surface is the complete specification and the public API is
settled, so the crate is at `1.0`. The General Decimal Arithmetic spelling of the
operation names is deliberate and differs from the shorter, operator aligned
names the fixed width siblings use; that divergence and the rest of the settled
surface are recorded in ADR-0045. See ADR-0041 for the miscellaneous surface,
ADR-0040 for the transcendental contract, and ADR-0038 for the overall design.

## Quick start

```rust
use ferrodec_decimal::{Context, Decimal, NonZeroU32, Rounding};

// A context with 50 working digits, rounding half to even. The precision
// is a NonZeroU32: a zero working precision is unrepresentable.
let ctx = Context::new(
    NonZeroU32::new(50).unwrap(),
    1_000_000,
    -1_000_000,
    Rounding::HalfEven,
);

let a = Decimal::parse_str("1").unwrap();
let b = Decimal::parse_str("3").unwrap();
let (third, status) = a.divide(&b, &ctx);

// 1 / 3 to 50 digits: "0." followed by fifty threes.
assert_eq!(third.digits(), Some(50));
assert!(status.inexact());
```

## Performance

The arbitrary precision kernels were optimised in a measured pass (ADR-0043
baseline, ADR-0044 results) and a follow-up patch (ADR-0046). Four algorithmic
changes carry the high precision cost down: Paterson-Stockmeyer rectangular
splitting of the logarithm series, trading the linear count of full-width
multiplies for a square-root one; a Karatsuba multiply for the coefficient bignum
above a limb threshold, leaving the small operand path on schoolbook; binary
splitting of the `ln 2` / `ln 10` constants, whose series is a small rational;
and argument halving for `exp` (`e^r = (e^(r / 2^j))^(2^j)`). On the recorded
host (Apple M2 Max, `rustc` 1.95.0), against the pre-pass baseline:

| operation        | working precision | before  | after            |
|------------------|------------------:|--------:|------------------|
| `ln`             |        500 digits | 5.16 ms | 1.03 ms (5.0x)   |
| `log10`          |        500 digits | 4.66 ms | 1.60 ms (2.9x)   |
| `exp`            |        500 digits | 3.90 ms | 1.46 ms (2.7x)   |
| `power`          |        500 digits | 9.83 ms | 2.77 ms (3.5x)   |
| coefficient `mul`|       4000 digits |  521 µs | 188 µs (2.8x)    |

The common low precision path and the core arithmetic are unchanged; the wins are
at high precision, where the quadratic and cubic costs dominated. The numbers are
host specific, so reproduce them on a target with `cargo bench`. A Newton
reciprocal division was evaluated and not adopted: the bench puts its crossover
beyond the precisions this crate runs (ADR-0046).

## Verification

Every operation is checked against the General Decimal Arithmetic reference two
ways. The vendored general decTest suite (`cargo test`) is the spec-authored,
cohort-exact cross-check of the whole operation surface. A randomized libmpdec
differential (`cargo test --features differential`, against CPython's `decimal`
module) sweeps the special values and the overflow, subnormal, and clamp
boundaries under all eight rounding modes; `power` is compared within a one-ulp
band, since this crate's is correctly rounded by construction while the
reference is only almost always. The coefficient bignum is checked against
`u128` ground truth and reconstruction identities.

## How this is developed

ferrodec, including this crate, is developed with an open disclosure of the
process; see the "How ferrodec is developed" section of the workspace root
`README.md`, which covers every member crate.
