# ferrodec

[![CI](https://github.com/ixmatus/ferrodec/actions/workflows/ci.yml/badge.svg)](https://github.com/ixmatus/ferrodec/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ferrodec.svg)](https://crates.io/crates/ferrodec)
[![docs.rs](https://docs.rs/ferrodec/badge.svg)](https://docs.rs/ferrodec)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/ferrodec.svg)](#license)

ferrodec is a family of decimal math libraries for Rust. The family spans the three fixed-width IEEE 754-2019 interchange formats, for embedded targets and general-purpose code that want decimal arithmetic without surprises, and an arbitrary-precision type for code that needs unbounded digits. All of them implement the same General Decimal Arithmetic semantics, so a value carries consistent meaning across the family.

- **[`ferrodec`](https://crates.io/crates/ferrodec)**: Decimal128 (this README's subject). 34-digit precision, exponent range `10⁻⁶¹⁴³..=10⁺⁶¹⁴⁴`. The reference implementation; production-ready.
- **[`ferrodec-decimal64`](ferrodec-decimal64/)**: Decimal64. 16-digit precision, exponent range `10⁻³⁸³..=10⁺³⁸⁴`. The sweet spot for financial general-ledger arithmetic and scientific aggregates that outgrow Decimal32 without needing 128 bits.
- **[`ferrodec-decimal32`](ferrodec-decimal32/)**: Decimal32. 7-digit precision, exponent range `10⁻¹⁰¹..=10⁹⁶`. Sized for embedded telemetry, small-ledger reporting, and footprint-sensitive applications.
- **[`ferrodec-decimal`](ferrodec-decimal/)**: arbitrary-precision General Decimal Arithmetic. `no_std` but `alloc`-required (the coefficient is a growable heap integer), the workspace's needs-an-allocator tier. It implements the full numerical and miscellaneous operation surface of the specification the fixed formats derive from; the API is settled and the performance pass is done, so it is at `1.0`.

Four workspace-internal crates support them:

- **[`ferrodec-ieee`](ferrodec-ieee/)**: the shared IEEE 754-2019 metadata types (`Status`, `RoundingMode`, `IeeeClass`), re-exported by every format so values flow across precisions without conversion. See [ADR-0012](docs/decisions/0012-extract-ferrodec-ieee.md).
- **[`ferrodec-multiword`](ferrodec-multiword/)**: the fixed-width and growable integer primitives the formats compute on, including the `DecBig` coefficient backend that `ferrodec-decimal` is built on.
- **[`ferrodec-transcend`](ferrodec-transcend/)**: the shared correctly-rounded transcendental kernel the fixed formats use for the §9.2 functions.
- **[`ferrodec-test-support`](ferrodec-test-support/)**: the IBM decTest harness scaffolding (parser, directive accumulator, expectation guard, run-suite driver). Workspace-internal only (`publish = false`); not part of any consumer's published surface. See [ADR-0013](docs/decisions/0013-conformance-harness-consolidation.md).

Each public crate stands alone on crates.io with its own version cadence, and they share the verification methodology documented in `docs/decisions/` and the workspace-level lint / MSRV / license discipline. This README covers the family at a glance and then documents the ferrodec Decimal128 crate in full.

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

**Scope.** ferrodec is a personal project consisting of a core crate and the ferrodec-decimal, ferrodec-decimal32,
ferrodec-decimal64, ferrodec-ieee, ferrodec-transcend, and ferrodec-multiword member crates in the same workspace,
together with the dev-only ferrodec-test-support crate; this disclosure covers all of them. The lead consumer is
Parnell's own embedded calculator firmware on STM32U class hardware; durability and quality are goals, but this is not
a funded library with a maintenance team behind it. The published versions on crates.io are yanked; the repository
remains public for users who want to read or fork the work.

**What this does not promise.** AI collaboration does not transfer responsibility. The author is accountable for what
ships under his name. The disciplines above narrow the failure surface; they do not eliminate it. In particular, this
process is most exposed to subtle bugs that a careful human reading of the code would catch but tests, types, and
formal verification would not. For correctly rounded decimal arithmetic that specifically includes rounding errors on boundary cases the decTest
suite did not cover, rounding or boundary errors in the arbitrary precision ferrodec-decimal type on operands wider
than the u128 ground truth oracle reaches that the sampled libmpdec differential did not draw, misrounds on
Decimal64 or Decimal128 transcendental boundary inputs if one of the escalation ladder's two auditable premises fails:
an unsound per function error budget or a gap in the input side exact and tie classification. The ladder replaced the
earlier sampled-search discharge after the project's own falsification campaign refuted it on high decade Decimal128
trigonometric inputs; under the statistical model stated in the accepted design records, the expected residual for
default builds is around one in 10^36 calls, builds with the unbounded rung carry no such residual, and the Decimal32
transcendentals and square root remain exhaustively verified. The process is also exposed to conformance regressions in operations no harness happened to exercise. Issues are welcome and will be triaged as time allows; no SLA is offered. This README describes the project's
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
ferrodec = "3.3"
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
| `exp-log` | no | `exp`, `exp2`, `exp10`, `ln`, `log2`, `log10`, `exp_m1`, `exp2_m1`, `exp10_m1`, `ln_1p`, `log2_1p`, `log10_1p`, `cbrt`, `rootn`, `rsqrt`, `compound`, `hypot` | +84 KB |
| `trig` | no | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Pulls the 6 300-digit Payne-Hanek `2/π` table | +116 KB |
| `trig-pi` | no | `sin_pi`, `cos_pi`, `tan_pi`, `asin_pi`, `acos_pi`, `atan_pi`, `atan2_pi` (IEEE 754-2019 `sinPi` … `atan2Pi`). Standalone: does not pull `trig` or the Payne-Hanek table, because the `x mod 2` reduction is exact decimal arithmetic | +137 KB alone; +62 KB over `trig` |
| `hyperbolic` | no | `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`. Implies `exp-log` | +22 KB over `exp-log` |
| `pow` | no | `pow`, `powi`, `powr`. Implies `exp-log` (`pow(x, y) = exp(y · ln x)`) | +15 KB over `exp-log` |
| `transcendentals` | no | Meta-feature pulling all five above. The pre-1.2 shape; existing dependents see no change. | +185 KB |
| `unbounded-ladder` | no | The ADR-0059 unbounded escalation rung: no exception set, working precision widens at run time until each rounding is decided. Requires an allocator; meaningful only alongside a transcendental cluster. Default and no-alloc builds are unchanged. | needs `alloc`; ~+190 KB pre-link |
| `binary-float` | no | `to_f64`, `to_f32`, `from_f64`, `from_f32` (pulls in `fmt`) | small |
| `ops` | no | `core::ops` operator overloads (`+`, `-`, `*`, `/`, `%`, `Neg`, `*Assign`). Default rounding mode `NearestEven`; status discarded. `%` routes to `rem_near` on this format (IEEE 754-2019 §5.3.1) and to `rem_trunc` on the siblings (GDA truncated); ADR-0027 records the per-format choice. See *Why no `core::ops`* below. | tiny |
| `serde` | no | `Serialize` / `Deserialize` via the canonical decimal string. Helper module `ferrodec::serde_bid` for raw 128-bit BID serialization in binary formats. (pulls in `fmt`) | small |
| `num-traits` | no | `Zero`, `One`, `Bounded`, `Num`, `Signed`, `From|To Primitive`. Implies `ops`. | small (over `ops`) |
| `dpd` | no | `Decimal128::to_dpd_bytes` / `from_dpd_bytes` for IEEE 754:2019 Densely Packed Decimal byte-pattern interchange. Storage and arithmetic stay BID; this is a byte-level adapter for round-tripping with IBM decNumber, z/Architecture decimal-FP hardware, and the upstream `dqEncode` / `dqCanonical` conformance vectors. | +7 KB |
| `kani` | no | Compile the formal verification harnesses; off in normal builds | none in production |

(Sizes are the `libferrodec.rlib` delta in release mode, measured at the 1.3.0 release; the `trig-pi` row is measured at the ADR-0061 landing. The ADR-0059 escalation ladder adds about 83 KB of pre-link `.text` + `.rodata` on `thumbv6m` to any build with transcendental features — the 110 digit mirror kernel and its wide reduction; ADR-0059 §Outcome records the measurement. The actual `.text` section in a linked binary will be somewhat smaller. Numbers are illustrative; profile your own application before deciding.)

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
| Exponential | `exp`, `exp2`, `exp10`, `exp_m1`, `exp2_m1`, `exp10_m1` |
| Logarithm | `ln`, `log2`, `log10`, `ln_1p`, `log2_1p`, `log10_1p` |
| Power and root | `pow`, `powi`, `powr`, `cbrt`, `rootn`, `rsqrt`, `hypot`, `compound` |
| Trigonometric | `sin`, `cos`, `tan`, `sin_pi`, `cos_pi`, `tan_pi` |
| Inverse trigonometric | `asin`, `acos`, `atan`, `atan2`, `asin_pi`, `acos_pi`, `atan_pi`, `atan2_pi` |
| Hyperbolic | `sinh`, `cosh`, `tanh` |
| Inverse hyperbolic | `asinh`, `acosh`, `atanh` |

## Accuracy

ferrodec promises correctly rounded results, meaning exactly the single nearest representable value at 34 digits (ties to even at `NearestEven`, the directed grid point at the four IEEE 754 directed modes), for the core IEEE operations (ADR-0021) and for the §9.2 transcendental surface.

For the transcendentals the discharge is the ADR-0059 escalation ladder, which replaced the earlier fixed precision argument after the project's own falsification campaign refuted it: certified misround witnesses on high decade Decimal128 trigonometric inputs, exactly where the statistical model said the sampled evidence was thin. Those witnesses now round correctly and replay as a pinned regression gate. The ladder's construction: inputs whose true result is exact or on a nearest mode tie are classified arithmetically before any kernel runs; results that hug a grid point asymptotically are decided by side theorems (ADR-0051); everything else evaluates at 50 digits and delivers only when the result clears a per function error budget against every rounding boundary, escalating to a 110 digit rung otherwise. The claim is tiered honestly: unconditionally, every result lies within the top rung's quantified bracket; correct rounding holds by construction conditional on two auditable premises (budget soundness and classification completeness); and the expected residual under the statistical model is about 10^-36 per call for default builds. The opt-in `unbounded-ladder` feature removes even that: its rung widens the working precision at run time until the rounding is decided, so such builds carry no exception set. ADR-0059 and its Outcome section carry the full record, including the measured costs. The §9.2 surface additions that postdate the ladder (`ln_1p`, `log2_1p`, `log10_1p`, `exp_m1`, `exp2_m1`, `exp10`, `exp10_m1`, and the algebraic group `powi`, `powr`, `rootn`, `compound`, `rsqrt`, `hypot`; ADR-0059 Track D) run on it from their first release, each with its own input side classification, anchor seams, and budget; the algebraic group additionally carries ADR-0060's Liouville floors, which bound how close any non classified true value can sit to a rounding boundary, together with that ADR's exact integer adjudicator on the residual path: within the tabulated operand ranges (`powi` for `-6 <= n <= 6`, `rootn` for `2 <= |n| <= 6`, `compound` for `|n|` times the width of the exact `1 + x` at most 196, `rsqrt` and `hypot` everywhere) a near boundary verdict is decided by an exact integer comparison rather than delivered on a margin, so correct rounding there is unconditional in every build; their evidence base is the certified corpus with exact per bucket pins plus the MPFR cross check, not the exhaustive Decimal32 program, which predates them. The pi scaled family (`sin_pi`, `cos_pi`, `tan_pi`, `asin_pi`, `acos_pi`, `atan_pi`, `atan2_pi`; ADR-0061) runs on the same ladder with a structural advantage the radian family cannot have: its argument reduction (`x mod 2`) is exact decimal arithmetic at every magnitude, its exact input sets are closed under Niven's theorem (integers and half integers, plus quarter integers for `tan_pi`, are the only rationals whose results are representable), and no other input can round to a nearest mode tie in any of the three formats, so for this family the classification completeness premise is discharged by theorem rather than by enumeration.

One behavioural caveat applies at the boundary.

* **`tan(x)` near the asymptotes** at odd multiples of π/2 returns ±∞. Note the absence of a `DIV_BY_ZERO` flag: `tan` produces a transcendental asymptote, not a literal IEEE division by zero. The ±∞ return is itself the correctly rounded result.

The trigonometric reduction handles the full Decimal128 magnitude range. `sin(10^15)` and `sin(10^3000)` round as accurately as `sin(0.5)` does, because argument reduction uses the algorithm of Payne and Hanek with a 6 408 digit table of 2/π (sized for the 110 digit rung's wide window; the unbounded rung computes its window at whatever depth its precision demands). Inputs that fall within one ULP of an integer multiple of π/2 (the rounded value of π itself, for example) cancel down to a 33 digit residual; the windowed multiplication carries enough width that even those boundary points deliver the correctly rounded value, escalating to the wide reduction when the 50 digit rung's bracket cannot decide the rounding. The pi scaled variants sidestep this machinery entirely: reducing `x mod 2` is exact in the format's own base, so `sin_pi` carries no reduction table and no reduction error term at any magnitude.

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
6. **Exhaustive worst-case correctness gate for Decimal32 §9.2 transcendentals** ([ADR-0033](docs/decisions/0033-worst-case-margin-completeness.md), `ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`). Every canonical Decimal32 input for each of the 18 unary §9.2 transcendentals (roughly 42 billion total) was walked offline through a certified two-tier Arb filter, recording the per-function worst-case input + proven correctly rounded output under `tests/vectors/transcend/exhaustive/`. The default-on test asserts the kernel reproduces the proven value at every function's tightest known input: all 18 §9.2 functions exactly correctly rounded (the §5 sqrt gate in the next item shares this test, bringing it to 19 of 19). The companion `--features=mpfr-gate` cross-validation (`ferrodec-test-support/tests/mpfr_gate.rs::mpfr_cross_validates_exhaustive_worst_cases`) confirms MPFR agrees bit for bit on every row, 0 disagreements. The exhaustive guarantee holds for Decimal32 only; Decimal64 and Decimal128's canonical input cardinalities (~10^18 and ~10^36) are beyond exhaustive reach, so those formats keep the sampled-corpus minimum as the binding empirical margin under the ADR-0033 corpus-integrity discipline. Four candidates of the f(1) = 0 family (`ln(1)`, `log2(1)`, `log10(1)`, `acosh(1)`) sit at the format's underflow boundary where the certified Arb ball spans 0 at every precision; the kernel short circuits each to 0 exactly, so the residual is an oracle-side limitation, not a kernel defect.

7. **Exhaustive sqrt correctness gate for Decimal32** ([ADR-0034](docs/decisions/0034-empirical-coverage-extension.md), the same harness as the gate above). The IEEE 754 §5 mandatory squareRoot extends the exhaustive Decimal32 program from the §9.2 recommended transcendentals to a mandatory operation. Every canonical non-negative Decimal32 input (1.728 billion) was walked offline through the same two-tier Arb filter; the worst-case half-ULP margin is 1.25e-8 at the largest seven-digit coefficient, proven correctly rounded and replayed by the default-on test (19 of 19 with the §9.2 set) and confirmed by the `--features=mpfr-gate` cross-check (0 disagreements). sqrt has no Table Maker's Dilemma residual: unlike the f(1) = 0 logarithmic candidates above, sqrt's exact cases (`sqrt(0)`, `sqrt(1)`, perfect squares) round exactly, so the exhaustive Decimal32 unary surface that admits the proof shape, the 18 §9.2 transcendentals plus §5 sqrt, is now complete. Decimal64 and Decimal128 sqrt keep the proptest envelope against `astro-float`; their canonical input cardinalities are beyond exhaustive reach.

8. **Exhaustive identity sweep for Decimal32** ([ADR-0034](docs/decisions/0034-empirical-coverage-extension.md), `ferrodec-decimal32/tests/identity_exhaustive.rs`, on-demand). A pure-Rust walk over the full 2^32 Decimal32 encoding space closes the identities the bounded Kani harnesses ([ADR-0015](docs/decisions/0015-kani-scope-policy.md), [ADR-0016](docs/decisions/0016-kani-harness-shim-routing.md)) cannot reach. Total order reflexivity (`total_cmp(x, x)`) holds for every one of the 2^32 encodings, across every cohort and NaN payload, beyond the same-cohort finite-finite domain Kani proves. The 3.84 billion canonical finite values additionally satisfy, with zero violations, the Display-then-parse round trip bit for bit (a path neither Kani nor the existing fuzz sampling established exhaustively), the `x + 0` value identity through the general add path, and the `next_up`/`next_down` successor inverse. The test is `#[ignore]`d so the multi-second walk stays out of the default suite while still compiling under CI; run it explicitly with `--release -- --ignored`.

9. **Escalation ladder witnesses and differentials** ([ADR-0059](docs/decisions/0059-correctly-rounded-decimal128-lane.md)). The S1 falsification corpus — 1 819 Arb-certified Decimal128 trig misround witnesses against the pre-ladder kernel (sin 643, cos 570, tan 606) — replays on every run as a pinned gate with exact per-file counts (`tests/transcend_campaign_s1.rs`). Three build configurations keep the ladder from rotting: `--cfg force_escalate` routes every guarded delivery through the 110-digit rung and demands byte identity with the full pinned corpus, `--cfg force_rung3` does the same through the `unbounded-ladder` dynamic rung, and `--cfg ladder_audit` panics on any top-rung residual ambiguity. The budget audit harness asserts rung 1's observed error stays under a tenth of each trig budget over the witness bands, and the runtime constant generators are pinned against mpmath oracles at four depths with algebraic cross-identities.
10. **Planted rung-2-forcing corpus and pinned escalation telemetry** (ADR-0059 S3, `tests/vectors/transcend/planted/`). 36 operations x 6 constructed Decimal128 inputs whose true results sit at chosen distances from a rounding boundary: control rows one decade above the rung 1 escalation threshold (must not escalate), entry and deep rows one to three decades below it (must). Every row is Arb-certified and replays bit-exact; with the test-only `telemetry` feature, exact per-file rung 2 entry counts are pinned over both the planted and the sampled corpora, so a budget or predicate drift moves a pin in one direction or the other. The sibling formats carry the complementary pin: their escalation thresholds sit below what their coefficient lattices can express, so their entire corpora assert zero natural escalations. A weekly `verification.yml` workflow re-runs the force-escalate, ladder-audit, adjudicator, differential, MPFR, and campaign-smoke lanes off the push path.

For the transcendentals specifically, `docs/testing.md` is the
conceptual map: it explains the correlated failure surface that the
shared Extended kernel creates, why a structurally independent oracle
is the only mitigation, and what each verification layer proves and
does not prove. Read it before trusting or extending a transcendental.
ADR-0033 is the latest tightening: for Decimal32 the correlated
failure surface is provably empty across every canonical input
modulo the four f(1) = 0 candidates the kernel handles correctly
via short circuit.

## Performance

A tight feedback loop matters more than chasing microseconds, but the criterion benches in `benches/` exist so regressions surface quickly. Representative numbers from `cargo bench --bench core_ops` on a 2025-era Apple Silicon host (rustc 1.95.0 stable, release profile with thin LTO and one codegen unit):

* `add`: 5.8 µs across a 6-call inner loop (about 970 ns per call).
* `sub`: 31 µs across a 6×6 matrix (850 ns per call).
* `mul`: 31 µs (870 ns per call).
* `div`: 45 µs (1.25 µs per call).
* `sqrt`: 20 µs over five inputs (4.1 µs per call).
* `fma`: 415 µs across a 6×6×6 matrix (1.9 µs per call).

These are the standing measured numbers since the dedicated perf pass (recorded in [`docs/decisions/0008-perf-results.md`](docs/decisions/0008-perf-results.md), which moved the headline operations 23 % to 27 % faster); the kernels have not changed since, but the numbers are host specific, so reproduce them with `cargo bench` on your target rather than relying on the absolute values.

Run `cargo bench --features=transcendentals --bench transcendentals` for the math kernels, `cargo bench --features=fmt --bench conversions` for parse and format throughput, and `cargo bench --features=fmt --bench comparison` for `partial_cmp` / `total_cmp` shapes.

The ADR-0059 correctness ladder carries a measured cost on the transcendentals, accepted correctness-first: on typical inputs the boundary guard adds 0.7 % to 6.3 % depending on the function, and on full-range random Decimal128 trigonometric inputs (where roughly 3 % of calls, 6 % for `tan`, escalate to the 110-digit rung) the averages are sin +31 %, cos +32 %, tan +64 % against the pre-ladder kernel. The `unbounded-ladder` feature adds nothing measurable until its rung is entered, which is a ~10^-36 per call event. ADR-0059 §Outcome records the methodology; the tightening target is the rung-1 reduction bound, not the pad.

The arbitrary-precision sibling carries its own performance story: a measured pass sped its high-precision transcendental kernels by 2.7x to 5.0x, with the before-and-after table in [`ferrodec-decimal/README.md`](ferrodec-decimal/) (and ADR-0043, ADR-0044, ADR-0046).

## Why no `core::ops` (and how to opt in)

Three reasons by default. First, every IEEE operation needs a `RoundingMode`; an `Add` impl cannot accept one without departing from the trait. Second, every operation produces a `Status` that callers must be free to inspect or compose; an `Add` impl cannot return one without departing from the trait. Third, the IEEE arithmetic identities that callers expect (`a + 0 == a`, `a × 1 == a`) sometimes hold only modulo cohort, not bit pattern; using `==` for IEEE numeric comparison would silently change the meaning of equality.

The default surface keeps both contracts visible at every call site: `a.add(b, rm)` returns `(Decimal128, Status)`, and you choose what to do with each.

For users who want the ergonomic shape and accept the trade-off, ferrodec ships an `ops` feature flag. Enable it and the `+`, `-`, `*`, `/`, `%` operators (plus `+=`, `-=`, etc., and unary `-`) become available on `Decimal128`. Each operator routes through the corresponding explicit method at `RoundingMode::NearestEven` and discards the per-operation `Status`. `%` routes to `rem_near` (IEEE 754-2019 §5.3.1 nearest-even) on this format and to `rem_trunc` (GDA truncated) on the siblings; the per-format choice is documented under ADR-0027. Embedded users on the default profile see no change; non-embedded users get `rust_decimal`-style ergonomics with one feature flag.

The `num-traits` feature transitively enables `ops` because `num_traits::Num` requires `Add + Sub + Mul + Div + Rem`.

The same reasoning leads us to implement `Eq` and `PartialEq` as bitwise equality. `partial_cmp` returns the IEEE numeric comparison; `total_cmp` returns the IEEE 754:2019 totalOrder predicate; `==` returns whether the two `u128` representations are identical. That trade keeps `Decimal128` usable as a `HashMap` key, predictable in tests, and trivially `const` comparable.

## Porting between the ferrodec formats

`ferrodec` (Decimal128), `ferrodec-decimal64`, and `ferrodec-decimal32` share an API shape but diverge in a few places a maintainer would otherwise rediscover the hard way. Every divergence is named here and in an architecture decision record rather than left implicit.

| Aspect | Decimal128 (`ferrodec`) | Decimal64 / Decimal32 siblings | Write portable code by |
| --- | --- | --- | --- |
| `rem_near` / `rem_trunc` / `%` | all three formats expose explicit `rem_near` (IEEE 754-2019 §5.3.1 nearest-even) and `rem_trunc` (GDA truncated); `%` routes to `rem_near` here | all three formats expose explicit `rem_near` and `rem_trunc`; `%` routes to `rem_trunc` on the siblings | calling the explicit `rem_near` or `rem_trunc` directly. ADR-0027 records why bare `rem` (asymmetric across the family in 1.x) was retired in 2.0 and why `%` keeps its per-format routing. |
| Cohort exponent | selected per the IEEE / GDA cohort rules | identical numeric value, but the cohort member is not guaranteed to match across formats or other GDA implementations | pinning the exponent with `quantize` before serializing, rendering, or comparing as a string |
| `Display` | General Decimal Arithmetic `toSci` rule (harmonized in 2.0 onto the rule the siblings already used; was an `f64::Display`-style boundary in 1.x). `value.fixed_preferred()` reproduces the 1.x integer-style rendering. | General Decimal Arithmetic `toSci` rule. `value.fixed_preferred()` ships on the siblings too as an additive 2.0 surface mirroring the parent's adapter. | comparing by numeric value, not by formatted string. ADR-0014 records the harmonization; ADR-0029 item 3 froze it into the 2.0 set. |
| Transcendentals | always available | gated behind the `exp-log` / `trig` / `trig-pi` / `hyperbolic` / `pow` sub-features | enabling the sub-features explicitly, not assuming a method exists without its feature |
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
