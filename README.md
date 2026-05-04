# ferrodec

An IEEE 754 (2019) Decimal128 library for Rust, written for embedded targets that need decimal arithmetic without surprises.

## What ferrodec is

ferrodec implements the BID 128 (Binary Integer Decimal) format from IEEE 754:2019. The encoding gives 34 decimal digits of precision, an exponent range from 10⁻⁶¹⁴³ through 10⁺⁶¹⁴⁴, every IEEE special value (signed zero, signed infinity, quiet and signaling NaN), and the full classification surface. The crate is `no_std`, allocates nothing on its own, and compiles cleanly down to Cortex M0+ (ARMv6 M, no floating point unit, no hardware divide).

Three design choices shape the library.

1. **Per operation status, never global flags.** Every operation returns `(Decimal128, Status)`. The `Status` records the IEEE flags raised by that one call: INVALID, DIV_BY_ZERO, OVERFLOW, UNDERFLOW, INEXACT. Callers compose flags however they like; ferrodec never reads or writes a thread local register.
2. **Methods, not operators.** ferrodec does not implement `core::ops::Add` or its siblings. An operator would silently swallow the `Status`, hide the `RoundingMode` parameter, and pretend that decimal arithmetic is as forgiving as integer arithmetic. It is not. Spelling each operation out (`a.add(b, rm)`, `x.sqrt(rm)`, `y.cos(rm)`) keeps the contract visible at every call site.
3. **Explicit rounding.** Every inexact operation takes a `RoundingMode` argument. The five IEEE 754:2019 directions are supported: `NearestEven` (the default), `NearestAway`, `TowardZero`, `TowardPositive`, and `TowardNegative`.

## Quick start

```toml
[dependencies]
ferrodec = "0"
```

```rust
use ferrodec::{Decimal128, RoundingMode};

let rm = RoundingMode::NearestEven;
let a = Decimal128::parse_str("1.1", rm).unwrap().0;
let b = Decimal128::parse_str("2.2", rm).unwrap().0;
let (sum, _status) = a.add(b, rm);
assert_eq!(format!("{sum}"), "3.3");
```

Notice what this short example demonstrates. Decimal addition gives the exact result a human would expect (`1.1 + 2.2 = 3.3`), without the `0.30000000000000004` artifact that binary floating point produces. The status flag records that the inputs were inexact representations of `1.1` and `2.2`, not that the addition itself was, and the `Display` output preserves the quantum that the input strings implied.

## Feature surface

ferrodec is feature gated so the embedded floor pays only for what it uses.

| Feature | Default | What it adds | Code size |
|---------|---------|--------------|-----------|
| `fmt` | yes | `parse_str`, `Display` (uses `core::fmt::Write`, no `alloc`) | small |
| `transcendentals` | no | `exp`, `exp2`, `ln`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `pow`, `cbrt` | moderate |
| `binary-float` | no | `to_f64`, `to_f32`, `from_f64`, `from_f32` (pulls in `fmt`) | small |
| `kani` | no | Compile the formal verification harnesses; off in normal builds | none in production |

## What you can call

The public API divides into five clusters.

### Construction and conversion

The named constants: `Decimal128::ZERO`, `NEG_ZERO`, `ONE`, `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`, `MIN_POSITIVE_NORMAL`, `INFINITY`, `NEG_INFINITY`, `NAN`, `SIGNALING_NAN`. The integer round trip: `from_i32`, `from_i64`, `from_i128`, `from_u32`, `from_u64`, `from_u128`, plus the `to_i32` … `to_u128` family that the reverse direction provides. The raw bit pattern: `from_bits` and `to_bits` for the 128 bit BID encoding. With `binary-float` enabled: `from_f32`, `from_f64`, `to_f32`, `to_f64`. With `fmt` enabled: `parse_str` and `Display`.

### Classification

`is_nan`, `is_signaling_nan`, `is_quiet_nan`, `is_infinite`, `is_finite`, `is_zero`, `is_normal`, `is_subnormal`, `is_sign_negative`, `is_sign_positive`, and `classify` (which returns `core::num::FpCategory`).

### Sign and ordering

`abs`, `neg`, `copysign`, `signum`. For ordering, `partial_cmp` provides numeric comparison and returns `(Option<Ordering>, Status)`; `total_cmp` provides the IEEE 754:2019 totalOrder predicate over all bit patterns, including NaN payloads.

### Arithmetic

`add`, `sub`, `mul`, `div`, `fma`, `sqrt`, `rem`. Each returns `(Decimal128, Status)` and takes a `RoundingMode`, except `rem`, which is exact whenever it terminates.

### Rounding to integral

`floor`, `ceil`, `trunc`, `round` (ties away from zero), and `round_ties_even` give the conventional API. The IEEE 754:2019 §5.3 family lives alongside as `round_to_integral(rm)` and `round_to_integral_exact(rm)`; the second variant raises `INEXACT` for non integer inputs.

### Transcendentals (feature `transcendentals`)

| Family | Methods |
|--------|---------|
| Exponential | `exp`, `exp2` |
| Logarithm | `ln`, `log2`, `log10` |
| Power and root | `pow`, `cbrt` |
| Trigonometric | `sin`, `cos`, `tan` |
| Inverse trigonometric | `asin`, `acos`, `atan`, `atan2` |
| Hyperbolic | `sinh`, `cosh`, `tanh` |
| Inverse hyperbolic | `asinh`, `acosh`, `atanh` |

## Accuracy

ferrodec promises faithful rounding, meaning ≤ 1 ULP at 34 digits, for the core IEEE operations and for every transcendental on the typical input domain. Three caveats apply at the boundaries.

* **Hyperbolic forwards on `|x| ≥ 0.5`** compose two `exp` calls and combine. Each call rounds correctly, but the composition stretches the envelope to about 5 ULP at the upper edge. Inside `|x| < 0.5` the kernel uses a direct Taylor series and stays at 1 ULP.
* **Inverse hyperbolics** compose `ln(x + sqrt(x² ± 1))` and inherit the same envelope (≤ 5 ULP) as the hyperbolic forwards.
* **`tan(x)` near the asymptotes** at odd multiples of π/2 returns ±∞. Note the absence of a DIV_BY_ZERO flag: `tan` produces a transcendental asymptote, not a literal IEEE division by zero.

The trigonometric reduction handles the full Decimal128 magnitude range. `sin(10^15)` and `sin(10^3000)` round as accurately as `sin(0.5)` does, because argument reduction uses the algorithm of Payne and Hanek with a 6 300 digit table of 2/π. Inputs that fall within one ULP of an integer multiple of π/2 (the rounded value of π itself, for example) cancel down to a 33 digit residual; the windowed multiplication widens to U512 to recover the remaining 50 digits, so even those boundary points round at ≤ 1 ULP.

## Supported targets

ferrodec compiles for any target Rust 1.84 supports. Three targets exercise on every commit.

* `x86_64-unknown-linux-gnu` for Linux CI.
* `aarch64-apple-darwin` for macOS CI.
* `thumbv6m-none-eabi` (the Cortex M0+ floor: STM32U0 family, no FPU, no hardware divide, as little as 32 KB of RAM in the cheapest parts) for the embedded ship target.

`cargo build --target thumbv6m-none-eabi --no-default-features --features=transcendentals,binary-float` is part of the CI matrix.

## Verification

ferrodec leans on four overlapping verification stacks.

1. **Unit tests** (`cargo test`). 330 tests in the library plus per module integration suites.
2. **Property tests** (proptest). Twelve files cover add/sub/mul/div/sqrt/rem, exp, ln, sincos, the inverse and hyperbolic functions, pow, the binary float conversions, and the addsub alignment edge case. Each cross checks against `astro-float`, a pure Rust arbitrary precision oracle, at the documented per function envelope.
3. **Conformance vectors** (`tests/conformance.rs`). The runner consumes every `dq*.decTest` file from Mike Cowlishaw's [General Decimal Arithmetic Testcases](https://speleotrove.com/decimal/dectest.html), 6 610 cases total. Pass/fail/skip totals act as regression guards: any drop below the floor or rise above the ceiling fails the build. Current totals are 6 216 pass, 0 fail, 394 skip. The skips are operations and rounding modes outside IEEE 754:2019.
4. **Formal verification** (Kani, behind `--features=kani`). 50 harnesses prove NaN propagation, sign rules, special value invariants, encode/decode round trips, and basic arithmetic identities for the IEEE special case dispatch paths. The harnesses use bounded operand shims (`*_special_only_for_kani`) so CBMC need not reason about the alignment and rounding loops.

## Performance

A tight feedback loop matters more than chasing microseconds, but the criterion benches in `benches/` exist so regressions surface quickly. Representative numbers from a 2024 era macOS host (`cargo bench --bench core_ops`):

* `add`: 28 µs across a 6×6 input matrix (about 800 ns per call).
* `mul`: 45 µs (1.3 µs per call).
* `div`: 54 µs (1.5 µs per call).
* `sqrt`: 22 µs over five inputs (4.4 µs per call).
* `fma`: 500 µs across a 6×6×6 input matrix (2.3 µs per call).

Run `cargo bench --features=transcendentals --bench transcendentals` for the math kernels and `cargo bench --features=fmt --bench conversions` for parse and format throughput.

## Why no `core::ops`

Three reasons. First, every IEEE operation needs a `RoundingMode`; an `Add` impl cannot accept one without departing from the trait. Second, every operation produces a `Status` that callers must be free to inspect or compose; an `Add` impl cannot return one without departing from the trait. Third, the IEEE arithmetic identities that callers expect (`a + 0 == a`, `a × 1 == a`) sometimes hold only modulo cohort, not bit pattern; using `==` for IEEE numeric comparison would silently change the meaning of equality.

The same reasoning leads us to implement `Eq` and `PartialEq` as bitwise equality. `partial_cmp` returns the IEEE numeric comparison; `total_cmp` returns the IEEE 754:2019 totalOrder predicate; `==` returns whether the two `u128` representations are identical. That trade keeps `Decimal128` usable as a `HashMap` key, predictable in tests, and trivially `const` comparable.

## Internals worth knowing

Code that uses ferrodec rarely needs to know what is inside, but two pieces show up in error messages and benchmark output.

* **The multiword stack.** The IEEE pipeline uses `U256`, `U384`, and `U512` (in `src/multiword/`) as wider intermediates. They mirror each other's surface for symmetry. None ever escapes the crate.
* **`Extended`** (in `src/math/extended.rs`). Transcendentals run their inner kernels at 50 digits of precision: a U256 backed `coef`, an `i32` exponent, and a sign. The kernel rounds once at the `Decimal128` boundary. The 50 digit envelope absorbs the cumulative error of typical 30 to 200 term Taylor series and lets the final result round faithfully.

## License

Available under either MIT or Apache 2.0, at the user's option.

## Reading list

For background on the algorithms ferrodec uses, in roughly increasing order of subtlety:

* IEEE 754:2019, the standard itself, especially §3 (storage), §5 (operations), §6 (special values), §7 (status flags), and §9 (recommended operations).
* Mike Cowlishaw, [*General Decimal Arithmetic Specification*](https://speleotrove.com/decimal/), the source of the conformance vectors.
* Mary Payne and Robert Hanek, "Radian reduction for trigonometric functions" (ACM SIGNUM Newsletter 18:1, 1983), the argument reduction we use for `sin` and `cos` past the small angle regime.
* Jean Michel Muller and colleagues, *Handbook of Floating Point Arithmetic* (2nd edition, Birkhäuser 2018), for the proofs that the techniques actually deliver the precision they claim.
