# ferrodec

[![CI](https://github.com/ixmatus/ferrodec/actions/workflows/ci.yml/badge.svg)](https://github.com/ixmatus/ferrodec/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ferrodec.svg)](https://crates.io/crates/ferrodec)
[![docs.rs](https://docs.rs/ferrodec/badge.svg)](https://docs.rs/ferrodec)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/ferrodec.svg)](#license)

An IEEE 754 (2019) Decimal128 library for Rust, designed for two audiences: embedded targets that need decimal arithmetic without surprises, and general-purpose code that wants IEEE conformance with correctly rounded arithmetic and §9.2 transcendentals.

This repository hosts the ferrodec family of canonical pure-Rust IEEE 754 decimal types:

- **[`ferrodec`](https://crates.io/crates/ferrodec)**: Decimal128 (this README's subject). 34-digit precision, exponent range `10⁻⁶¹⁴³..=10⁺⁶¹⁴⁴`. The reference implementation; production-ready.
- **[`ferrodec-decimal32`](ferrodec-decimal32/)**: Decimal32. 7-digit precision, exponent range `10⁻¹⁰¹..=10⁹⁶`. Sized for embedded telemetry, small-ledger reporting, and footprint-sensitive applications.
- **[`ferrodec-decimal64`](ferrodec-decimal64/)**: Decimal64. 16-digit precision, exponent range `10⁻³⁸³..=10⁺³⁸⁴`. The natural sweet spot for financial general-ledger arithmetic and scientific aggregates that outgrow Decimal32's 7 digits without needing Decimal128's 128 bits.

Plus two workspace-internal crates that the three public crates share:

- **[`ferrodec-ieee`](ferrodec-ieee/)**: the shared IEEE 754-2019 metadata types (`Status`, `RoundingMode`, `IeeeClass`). All three sibling crates re-export from here, so values flow across precisions without conversion. See [ADR-0012](docs/decisions/0012-extract-ferrodec-ieee.md).
- **[`ferrodec-test-support`](ferrodec-test-support/)**: the IBM decTest harness scaffolding (parser, directive accumulator, expectation guard, run-suite driver). Workspace-internal only (`publish = false`); not part of any consumer's published surface. See [ADR-0013](docs/decisions/0013-conformance-harness-consolidation.md).

Each public sibling stands alone on crates.io with its own version cadence. They share the verification methodology documented in `docs/decisions/` and the workspace-level lint / MSRV / license discipline.

## How ferrodec is developed

This is an open disclosure of the development process so users can judge for themselves whether the resulting code
meets their bar.

**Authorship and collaboration.** Parnell Springmeyer is the author of record. ferrodec is developed in collaboration
with Claude, an AI coding agent from Anthropic. Parnell owns architecture, acceptance criteria, test and verification
strategy, and release boundaries. Claude drafts the implementation, writes and runs tests and verification harnesses,
and produces analysis under that direction. **Parnell does not review the generated code line by line.** Human
oversight operates at the level of design, strategy, and outcomes: does the architecture make sense, are the right
invariants being checked, does the verification strategy cover the risk surface, do the tests and proofs pass. Merges
to main are GPG signed by Parnell to attest to that level of review, not to an audit of every line.

**Provenance.** Implementations derive from primary sources: IEEE 754-2008 for decimal floating point semantics, the
Speleotrove decimal arithmetic specification for operation and rounding behavior, and published open algorithm work
with stated licenses for the harder pieces (packing conversions, division, correctly rounded transcendentals). The
agent is instructed to cite recalled sources rather than reproduce verbatim, to surface provenance uncertainty rather
than hide it, and to choose surface forms (identifiers, helper decomposition, file layout) fresh for idiomatic Rust
rather than copying from existing C reference implementations that serve as oracles for behavior.

These are instructions to the agent, not guarantees about every line of output. A verbatim reproduction or an unflagged
derivation could slip through. The project's defense against that is the instruction discipline above plus the human
reviewer's ability to notice architectural smells that suggest a problem upstream, not a clean room audit. If you spot
a passage that reads like a copy from a source it should not be copied from, please open an issue.

**Verification.** Correctness lives in the type system where it can, in formal proof harnesses (Kani) where the cost is
justified, in spec conformance vectors (the Speleotrove decTest suite) for end to end behavior, and in property and
example tests otherwise. CI runs the usual lints and the full test and verification suite; specific harness counts and
conformance counts change as the project evolves. Significant decisions are recorded as ADRs in the repo. `unsafe`
blocks carry a written justification at the call site.

**Scope.** ferrodec is a personal project consisting of a core crate and the ferrodec-decimal32, ferrodec-decimal64,
ferrodec-ieee, ferrodec-transcend, and ferrodec-multiword member crates in the same workspace, together with the
dev-only ferrodec-test-support crate; this disclosure covers all of them. The lead consumer is
Parnell's own embedded calculator firmware on STM32U class hardware; durability and quality are goals, but this is not
a funded library with a maintenance team behind it. The published versions on crates.io are yanked; the repository
remains public for users who want to read or fork the work.

**What this does not promise.** AI collaboration does not transfer responsibility. The author is accountable for what
ships under his name. The disciplines above narrow the failure surface; they do not eliminate it. In particular, this
process is most exposed to subtle bugs that a careful human reading of the code would catch but tests, types, and
formal verification would not. For correctly rounded decimal arithmetic that specifically includes rounding errors on
boundary cases the decTest suite did not cover, rounding errors on §9.2 transcendental boundary inputs the Arb
empirical worst-case search did not surface, or conformance regressions in operations no harness happened to
exercise. Issues are welcome and will be triaged as time allows; no SLA is offered. This README describes the project's
development process and is not a warranty; see the LICENSE file for the legal terms governing use.

## What ferrodec is

ferrodec implements the BID 128 (Binary Integer Decimal) format from IEEE 754:2019. The encoding gives 34 decimal digits of precision, an exponent range from 10⁻⁶¹⁴³ through 10⁺⁶¹⁴⁴, every IEEE special value (signed zero, signed infinity, quiet and signaling NaN), and the full classification surface. The crate is `no_std`, allocates nothing on its own, and compiles cleanly down to Cortex M0+ (`ARMv6` M, no floating point unit, no hardware divide).

Three design choices shape the library by default.

1. **Per operation status, never global flags.** Every operation returns `(Decimal128, Status)`. The `Status` records the IEEE flags raised by that one call: INVALID, `DIV_BY_ZERO`, OVERFLOW, UNDERFLOW, INEXACT. Callers compose flags however they like; ferrodec never reads or writes a thread local register.
2. **Methods, not operators (by default).** Callers spell each operation out as `a.add(b, rm)`, `x.sqrt(rm)`, `y.cos(rm)`, which keeps the `RoundingMode` and `Status` visible at every call site. For non-embedded users who accept the trade-off, the opt-in `ops` feature enables the conventional `+`, `-`, `*`, `/`, `%` operators (defaulting to `NearestEven` and discarding `Status`).
3. **Explicit rounding.** Every inexact operation takes a `RoundingMode` argument. The five IEEE 754:2019 directions are supported: `NearestEven` (the default), `NearestAway`, `TowardZero`, `TowardPositive`, and `TowardNegative`.

## Quick start

```toml
[dependencies]
ferrodec = "2"
```

The headline case is decimal arithmetic that rounds the way humans do.

```rust
# #[cfg(feature = "fmt")]
# fn main() {
use ferrodec::{Decimal128, RoundingMode};

let rm = RoundingMode::NearestEven;
let a = Decimal128::parse_str("1.1", rm).unwrap().0;
let b = Decimal128::parse_str("2.2", rm).unwrap().0;
let (sum, _status) = a.add(b, rm);
assert_eq!(format!("{sum}"), "3.3");
# }
# #[cfg(not(feature = "fmt"))]
# fn main() {}
```

Decimal addition gives the result a human would expect (`1.1 + 2.2 = 3.3`), without the `0.30000000000000004` artifact that binary floating point produces. The status flag records that the inputs were inexact representations of `1.1` and `2.2`, not that the addition itself was, and the `Display` output preserves the quantum that the input strings implied.

Constructing values without going through a string:

```rust
use ferrodec::Decimal128;

// 1.23 = 123 × 10^-2
let price = Decimal128::try_new(123, -2).unwrap();
let three_pence = Decimal128::try_new(3, -2).unwrap();
assert!(price.same_quantum(three_pence));
```

`try_new(coefficient, exponent)` takes the integer pair directly. No allocator, no parser, no `fmt` feature required: useful on the embedded floor where the parse path's working buffers are themselves a code size cost.

Inspecting the status flags:

```rust
use ferrodec::{Decimal128, RoundingMode};

let (q, status) = Decimal128::ONE.div(Decimal128::ZERO, RoundingMode::NearestEven);
assert!(q.is_infinite());
assert!(status.div_by_zero());
```

Every operation produces a per call `Status`, never a global flag word. Callers compose flags however they like; ferrodec never reads or writes a thread local register.

## Feature surface

ferrodec is feature gated so the embedded floor pays only for what it uses.

| Feature | Default | What it adds | Δ on `thumbv6m-none-eabi` |
|---------|---------|--------------|---------------------------|
| `fmt` | yes | `parse_str`, `Display` (uses `core::fmt::Write`, no `alloc`). The baseline below is `--features=fmt`. | 401 KB total |
| `exp-log` | no | `exp`, `exp2`, `ln`, `log2`, `log10`, `cbrt` | +84 KB |
| `trig` | no | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Pulls the 6 300-digit Payne-Hanek `2/π` table | +116 KB |
| `hyperbolic` | no | `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`. Implies `exp-log` | +22 KB over `exp-log` |
| `pow` | no | `pow`. Implies `exp-log` (`pow(x, y) = exp(y · ln x)`) | +15 KB over `exp-log` |
| `transcendentals` | no | Meta-feature pulling all four above. The pre-1.2 shape; existing dependents see no change. | +185 KB |
| `binary-float` | no | `to_f64`, `to_f32`, `from_f64`, `from_f32` (pulls in `fmt`) | small |
| `ops` | no | `core::ops` operator overloads (`+`, `-`, `*`, `/`, `%`, `Neg`, `*Assign`). Default rounding mode `NearestEven`; status discarded. `%` routes to `rem_near` on this format (IEEE 754-2019 §5.3.1) and to `rem_trunc` on the siblings (GDA truncated); ADR-0027 records the per-format choice. See *Why no `core::ops`* below. | tiny |
| `serde` | no | `Serialize` / `Deserialize` via the canonical decimal string. Helper module `ferrodec::serde_bid` for raw 128-bit BID serialization in binary formats. (pulls in `fmt`) | small |
| `num-traits` | no | `Zero`, `One`, `Bounded`, `Num`, `Signed`, `From|To Primitive`. Implies `ops`. | small (over `ops`) |
| `dpd` | no | `Decimal128::to_dpd_bytes` / `from_dpd_bytes` for IEEE 754:2019 Densely Packed Decimal byte-pattern interchange. Storage and arithmetic stay BID; this is a byte-level adapter for round-tripping with IBM decNumber, z/Architecture decimal-FP hardware, and the upstream `dqEncode` / `dqCanonical` conformance vectors. | +7 KB |
| `kani` | no | Compile the formal verification harnesses; off in normal builds | none in production |

(Sizes are the `libferrodec.rlib` delta in release mode at commit `1.3.0`. The actual `.text` section in a linked binary will be somewhat smaller. Numbers are illustrative; profile your own application before deciding.)

## What you can call

The public API divides into five clusters.

### Construction and conversion

The named constants: `Decimal128::ZERO`, `NEG_ZERO`, `ONE`, `NEG_ONE`, `TEN`, `MAX`, `MIN`, `MIN_POSITIVE`, `MIN_POSITIVE_NORMAL`, `INFINITY`, `NEG_INFINITY`, `NAN`, `SIGNALING_NAN`. Direct construction from integer parts: `try_new(coefficient, exponent)`. The integer round trip: `from_i32`, `from_i64`, `from_i128`, `from_u32`, `from_u64`, `from_u128`, plus the `to_i32` … `to_u128` family that the reverse direction provides. The raw bit pattern: `from_bits` and `to_bits` for the 128 bit BID encoding. With `binary-float` enabled: `from_f32`, `from_f64`, `to_f32`, `to_f64`. With `fmt` enabled: `parse_str` and `Display`. With `dpd` enabled: `to_dpd_bytes` and `from_dpd_bytes` for the IEEE 754:2019 DPD interchange byte pattern (a 16-byte big-endian value); arithmetic on the recovered `Decimal128` continues to use the BID kernels, so the codec is a pure interop adapter.

### Classification

`is_nan`, `is_signaling_nan`, `is_quiet_nan`, `is_infinite`, `is_finite`, `is_zero`, `is_normal`, `is_subnormal`, `is_sign_negative`, `is_sign_positive`, and `classify` (which returns `core::num::FpCategory`). The IEEE 754 §5.7.2 / §5.4.2 canonical pair: `is_canonical` and `canonicalize`.

### Sign and ordering

`abs`, `neg`, `copysign`, `signum`. For ordering, `partial_cmp` provides numeric comparison and returns `(Option<Ordering>, Status)`; `total_cmp` provides the IEEE 754:2019 totalOrder predicate over all bit patterns, including NaN payloads. `compare_total_magnitude(other)` does the same on `|self|` and `|other|`. The IEEE 754:2019 §9.6 selection operations are `min` and `max` (the `minimumNumber` / `maximumNumber` variant: a quiet NaN is the missing value, a signaling NaN raises `INVALID`) and `min_magnitude` / `max_magnitude` (the same decision on `|self|` versus `|other|`, deferring to `min` / `max` on an equal-magnitude tie; ADR-0028).

### Arithmetic

`add`, `sub`, `mul`, `div`, `fma`, `sqrt`, `rem_near`, `rem_trunc`. Each returns `(Decimal128, Status)` and takes a `RoundingMode`, except the two remainders, which are exact whenever they terminate. `rem_near` is IEEE 754-2019 §5.3.1 nearest-even (decTest `remaindernear`); `rem_trunc` is GDA / C99 `fmod` truncated (decTest `remainder`). The 1.x bare `rem` spelling was retired in 2.0; ADR-0027 records the rename.

### Rounding to integral

`floor`, `ceil`, `trunc`, `round` (ties away from zero), and `round_ties_even` give the conventional API. The IEEE 754:2019 §5.3 family lives alongside as `round_to_integral(rm)` and `round_to_integral_exact(rm)`; the second variant raises `INEXACT` for non integer inputs.

### Quantum operations

The IEEE 754:2019 §5.3 quantum surface: `quantize(target, rm)` rescales `self` to `target`'s quantum exponent; `same_quantum(other)` tests whether two values share an exponent; `scaleb(n, rm)` shifts the exponent by an integer; `logb()` returns `floor(log10(|x|))`; `next_up()` and `next_down()` step to the numerically adjacent representable value; `radix()` returns 10.

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

ferrodec promises correctly rounded results, meaning exactly the single nearest representable value at 34 digits (ties to even at `NearestEven`, the directed grid point at the four IEEE 754 directed modes), for the core IEEE operations (ADR-0021) and for every §9.2 transcendental across the supported domain (ADR-0032; supersedes ADR-0024's faithful contract). One behavioural caveat applies at the boundary.

* **`tan(x)` near the asymptotes** at odd multiples of π/2 returns ±∞. Note the absence of a `DIV_BY_ZERO` flag: `tan` produces a transcendental asymptote, not a literal IEEE division by zero. The ±∞ return is itself the correctly rounded result.

The trigonometric reduction handles the full Decimal128 magnitude range. `sin(10^15)` and `sin(10^3000)` round as accurately as `sin(0.5)` does, because argument reduction uses the algorithm of Payne and Hanek with a 6 300 digit table of 2/π. Inputs that fall within one ULP of an integer multiple of π/2 (the rounded value of π itself, for example) cancel down to a 33 digit residual; the windowed multiplication widens to U512 to recover the remaining 50 digits, so even those boundary points round to the correctly rounded value.

## Supported targets

ferrodec compiles for any target Rust 1.84 supports. Three targets exercise on every commit.

* `x86_64-unknown-linux-gnu` for Linux CI.
* `aarch64-apple-darwin` for macOS CI.
* `thumbv6m-none-eabi` (the Cortex M0+ floor: STM32U0 family, no FPU, no hardware divide, as little as 32 KB of RAM in the cheapest parts) for the embedded ship target.

`cargo build --target thumbv6m-none-eabi --no-default-features --features=transcendentals,binary-float` is part of the CI matrix.

## Verification

ferrodec leans on five overlapping verification stacks.

1. **Unit tests** (`cargo test`). The in-library unit suite plus per module integration suites and a doctest set on the public API. Counts move as the surface grows; the invariant is a green suite.
2. **Property tests** (proptest). The property files cover `add` / `sub` / `mul` / `div` / `sqrt` / `rem_near`, exp, ln, sincos, the inverse and hyperbolic functions, pow, the binary float conversions, and the addsub alignment edge case. Each cross checks against `astro-float`, a pure Rust arbitrary precision oracle, at the documented per function envelope.
3. **Conformance vectors** (`tests/conformance.rs`). The runner consumes every `dq*.decTest` file from Mike Cowlishaw's [General Decimal Arithmetic Testcases](https://speleotrove.com/decimal/dectest.html); the sibling crates do the same for their `dd*` / `ds*` files. Pass and skip counts move as dispatch arms are wired in; the invariant is zero failures. Two residual skip categories are by design. The non-IEEE rounding directives (`half_down`, `05up`) are the will-not-fix category from [ADR-0005](docs/decisions/0005-half-down-05up-wontfix.md). The General Decimal Arithmetic extension operations outside the IEEE 754-2019 mandatory set (`and` / `or` / `xor` / `invert`, `rotate`, `shift`, `reduce`, `divideInteger`, `compareSignaling`, `nextToward`) are the stated incremental path recorded in [ADR-0028](docs/decisions/0028-section-9-6-magnitude-min-max.md); the copy family and the §9.6 magnitude operations are dispatched. The `--features=dpd` build additionally exercises the DPD-encoded `dqEncode` / `dqCanonical` vectors.
4. **Formal verification** (Kani, behind `--features=kani`). The harnesses prove NaN propagation, sign rules, special value invariants, encode/decode round trips (BID and, with `--features=dpd`, DPD totality plus the special-value DPD round-trip), basic arithmetic identities for the IEEE special case dispatch paths, the IEEE 754:2019 §5.7.2 / §5.4.2 canonical predicates as a projection (`is_canonical` ⇔ `canonicalize`-fixed-point, idempotence), the §5.10 total-magnitude reflexivity / antisymmetry, `try_new`'s in-range / out-of-range dispatch, and `total_cmp` antisymmetry on the same-cohort same-sign finite-finite domain. The arithmetic-pipeline harnesses use bounded operand shims (`*_special_only_for_kani`) so CBMC need not reason about the alignment and rounding loops.
5. **Fuzz harness** (`fuzz/`, via `cargo install cargo-fuzz` and a nightly toolchain). Six libFuzzer targets: `parse` feeds arbitrary byte sequences through `Decimal128::parse_str` and asserts no panic plus a Display-then-parse round-trip; `arith` exercises `add` / `sub` / `mul` / `div` on arbitrary `(u128, u128)` pairs and asserts `a + 0 == a`, `a * 1 == a`, `a - a == 0` for finite `a`; `transcendentals` runs every transcendental kernel (`exp`, `ln`, `sin`, `cos`, `tan`, `pow`, `atan2`, the inverse and hyperbolic families, `sqrt`, `cbrt`) for panic-freedom on arbitrary inputs; `integral` checks idempotence and integer-ness of `floor`/`ceil`/`trunc`/`round`/`round_ties_even`/`round_to_integral`; `total_cmp` asserts reflexivity and antisymmetry of the §5.10 totalOrder predicate and `compare_total_magnitude` over arbitrary bit pairs; `encode` asserts `is_canonical` ↔ `canonicalize` fixed-point, idempotence, and classification stability across canonicalize. Run with `cargo +nightly fuzz run <target>` from the `fuzz/` directory.

For the transcendentals specifically, `docs/testing.md` is the
conceptual map: it explains the correlated failure surface that the
shared Extended kernel creates, why a structurally independent oracle
is the only mitigation, and what each verification layer proves and
does not prove. Read it before trusting or extending a transcendental.

## Performance

A tight feedback loop matters more than chasing microseconds, but the criterion benches in `benches/` exist so regressions surface quickly. Representative numbers from `cargo bench --bench core_ops` on a 2025-era Apple Silicon host (rustc 1.95.0 stable, release profile with thin LTO and one codegen unit):

* `add`: 5.8 µs across a 6-call inner loop (about 970 ns per call).
* `sub`: 31 µs across a 6×6 matrix (850 ns per call).
* `mul`: 31 µs (870 ns per call).
* `div`: 45 µs (1.25 µs per call).
* `sqrt`: 20 µs over five inputs (4.1 µs per call).
* `fma`: 415 µs across a 6×6×6 matrix (1.9 µs per call).

The `1.11.0` perf pass moved the headline operations 23 % to 27 % faster than `1.10.1`. See [`docs/decisions/0008-perf-results.md`](docs/decisions/0008-perf-results.md) for the per-bench delta and the full ADR-recorded methodology.

Run `cargo bench --features=transcendentals --bench transcendentals` for the math kernels, `cargo bench --features=fmt --bench conversions` for parse and format throughput, and `cargo bench --features=fmt --bench comparison` for `partial_cmp` / `total_cmp` shapes.

## Why no `core::ops` (and how to opt in)

Three reasons by default. First, every IEEE operation needs a `RoundingMode`; an `Add` impl cannot accept one without departing from the trait. Second, every operation produces a `Status` that callers must be free to inspect or compose; an `Add` impl cannot return one without departing from the trait. Third, the IEEE arithmetic identities that callers expect (`a + 0 == a`, `a × 1 == a`) sometimes hold only modulo cohort, not bit pattern; using `==` for IEEE numeric comparison would silently change the meaning of equality.

The default surface keeps both contracts visible at every call site: `a.add(b, rm)` returns `(Decimal128, Status)`, and you choose what to do with each.

For users who want the ergonomic shape and accept the trade-off, ferrodec ships an `ops` feature flag. Enable it and the `+`, `-`, `*`, `/`, `%` operators (plus `+=`, `-=`, etc., and unary `-`) become available on `Decimal128`. Each operator routes through the corresponding explicit method at `RoundingMode::NearestEven` and discards the per-operation `Status`. `%` routes to `rem_near` (IEEE 754-2019 §5.3.1 nearest-even) on this format and to `rem_trunc` (GDA truncated) on the siblings; the per-format choice is documented under ADR-0027. Embedded users on the default profile see no change; non-embedded users get `rust_decimal`-style ergonomics with one feature flag.

The `num-traits` feature transitively enables `ops` because `num_traits::Num` requires `Add + Sub + Mul + Div + Rem`.

The same reasoning leads us to implement `Eq` and `PartialEq` as bitwise equality. `partial_cmp` returns the IEEE numeric comparison; `total_cmp` returns the IEEE 754:2019 totalOrder predicate; `==` returns whether the two `u128` representations are identical. That trade keeps `Decimal128` usable as a `HashMap` key, predictable in tests, and trivially `const` comparable.

## Choosing between ferrodec and `rust_decimal`

`rust_decimal` is the established Rust decimal library and the right choice for many projects. Where ferrodec adds value is the IEEE 754 conformance, the precision width, and the verification posture; the cost is the smaller ecosystem and a more deliberate API by default. Honest comparison:

| | ferrodec | `rust_decimal` |
|---|---|---|
| Format | IEEE 754:2019 BID-128 | 96-bit fixed-point |
| Precision | 34 decimal digits | 28 decimal digits |
| Exponent range | 10⁻⁶¹⁴³ … 10⁺⁶¹⁴⁴ | 10⁻²⁸ … 10⁺²⁸ |
| Conformance | Full IEEE 754:2019 (NaN, ±∞, signaling NaN, all five rounding modes, total order, quantum ops, correctly rounded §9.2 transcendentals) | None: different model, no NaN/Inf, single banker's-rounding mode |
| Formal verification | Kani harnesses plus the full Speleotrove decTest conformance suite | None |
| `no_std` | Real (forbid unsafe, no alloc, fixed-size buffers) | Available with feature flag |
| Default API | Explicit `RoundingMode` + `(value, Status)` return | `core::ops` operators, banker's rounding |
| Ergonomic operators | Opt-in via `ops` feature (`NearestEven`, `Status` discarded) | Built-in |
| `serde` / `num-traits` | Behind feature flags | Built-in / via feature |
| Maturity | Younger; 1.x | Established, millions of downloads |

**Pick ferrodec when**: you need 34-digit precision, IEEE 754 conformance (NaN handling, multiple rounding modes, transcendentals), formal verification, or hard `no_std`. Financial systems with regulatory requirements; scientific calculators; embedded targets.

**Pick `rust_decimal` when**: you need fast, well-trodden, ecosystem-rich decimal arithmetic for typical money math; you're happy with 28 digits and banker's rounding; you want operators by default without thinking about it.

## Porting between the ferrodec formats

`ferrodec` (Decimal128), `ferrodec-decimal64`, and `ferrodec-decimal32` share an API shape but diverge in a few places a maintainer would otherwise rediscover the hard way. Every divergence is named here and in an architecture decision record rather than left implicit.

| Aspect | Decimal128 (`ferrodec`) | Decimal64 / Decimal32 siblings | Write portable code by |
| --- | --- | --- | --- |
| `rem_near` / `rem_trunc` / `%` | all three formats expose explicit `rem_near` (IEEE 754-2019 §5.3.1 nearest-even) and `rem_trunc` (GDA truncated); `%` routes to `rem_near` here | all three formats expose explicit `rem_near` and `rem_trunc`; `%` routes to `rem_trunc` on the siblings | calling the explicit `rem_near` or `rem_trunc` directly. ADR-0027 records why bare `rem` (asymmetric across the family in 1.x) was retired in 2.0 and why `%` keeps its per-format routing. |
| Cohort exponent | selected per the IEEE / GDA cohort rules | identical numeric value, but the cohort member is not guaranteed to match across formats or other GDA implementations | pinning the exponent with `quantize` before serializing, rendering, or comparing as a string |
| `Display` | General Decimal Arithmetic `toSci` rule (harmonized in 2.0 onto the rule the siblings already used; was an `f64::Display`-style boundary in 1.x). `value.fixed_preferred()` reproduces the 1.x integer-style rendering. | General Decimal Arithmetic `toSci` rule. `value.fixed_preferred()` ships on the siblings too as an additive 2.0 surface mirroring the parent's adapter. | comparing by numeric value, not by formatted string. ADR-0014 records the harmonization; ADR-0029 item 3 froze it into the 2.0 set. |
| Transcendentals | always available | gated behind the `exp-log` / `trig` / `hyperbolic` / `pow` sub-features | enabling the sub-features explicitly, not assuming a method exists without its feature |
| Precision and range | 34 digits, exponent 10⁻⁶¹⁴³ … 10⁺⁶¹⁴⁴ | 16 digits (Decimal64), 7 digits (Decimal32), narrower exponent ranges | widening through the decimal string: a value exact in a narrower format is exact in a wider one, never the reverse |

The numeric value an operation produces is stable across all three formats and against any conforming GDA implementation. What is not stable is the cohort member every format selects within that value. The `Display` rendering harmonized onto GDA `toSci` across the family in 2.0; the spelling of the remainder is the explicit `rem_near` / `rem_trunc` everywhere, with `%` documented per format. Pin what you serialize or compare with `quantize`; the architecture decision records carry the rationale.

## Internals worth knowing

Code that uses ferrodec rarely needs to know what is inside, but two pieces show up in error messages and benchmark output.

* **The multiword stack.** The IEEE pipeline uses `U256`, `U384`, and `U512` (in `src/multiword/`) as wider intermediates. They mirror each other's surface for symmetry. None ever escapes the crate.
* **`Extended`** (in `ferrodec-transcend/src/extended.rs`). Transcendentals run their inner kernels at 50 digits of precision: a U256 backed `coef`, an `i32` exponent, and a sign. The kernel rounds once at the `Decimal128` boundary. The 50 digit envelope absorbs the cumulative error of typical 30 to 200 term Taylor series and clears every empirical Arb worst-case half-ULP margin by more than thirty orders of magnitude (ADR-0032), so the final result lands on the correctly rounded value.

## MSRV policy

ferrodec's minimum supported Rust version is **1.84**. The MSRV is held for at least six months after each Rust release; bumping it is a minor-version event, never a patch. Library consumers can pin a minimum ferrodec version against a known MSRV with confidence that a `cargo update` won't silently push their toolchain forward.

## License

Available under either MIT or Apache 2.0, at the user's option.

## Reading list

For background on the algorithms ferrodec uses, in roughly increasing order of subtlety:

* IEEE 754:2019, the standard itself, especially §3 (storage), §5 (operations), §6 (special values), §7 (status flags), and §9 (recommended operations).
* Mike Cowlishaw, [*General Decimal Arithmetic Specification*](https://speleotrove.com/decimal/), the source of the conformance vectors.
* Mary Payne and Robert Hanek, "Radian reduction for trigonometric functions" (ACM SIGNUM Newsletter 18:1, 1983), the argument reduction we use for `sin` and `cos` past the small angle regime.
* Jean Michel Muller and colleagues, *Handbook of Floating Point Arithmetic* (2nd edition, Birkhäuser 2018), for the proofs that the techniques actually deliver the precision they claim.
