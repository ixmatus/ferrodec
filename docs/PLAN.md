# ferrodec — IEEE 754 Decimal128 for embedded calculators

## Context

`ferrodec` is a fresh crate (only `LICENSE` + a one-line `README.md` exist today). The
goal is a `no_std`-friendly, zero-runtime-dep Rust implementation of IEEE 754-2019
**Decimal128** intended for use in an embedded scientific calculator. Correctness is
the dominant concern: the design must be amenable to formal verification with
**Kani** for the parts where that is tractable, with property tests covering the rest
against an arbitrary-precision oracle (**astro-float**).

The constraints below were settled with the user in plan-mode Q&A:

| Decision              | Choice                                                                                                   |
| --------------------- | -------------------------------------------------------------------------------------------------------- |
| Storage encoding      | **BID** (Binary Integer Decimal) — coefficient is a 113-bit unsigned binary integer                      |
| Op scope (v1)         | IEEE 754 core (+ − × ÷ fma sqrt rem cmp class) **plus** calculator transcendentals (exp/log/sin/cos/pow) |
| Exception flags       | **Per-op `Status` returned** — no global / thread-local state                                            |
| Runtime deps          | Zero. `core` only. Dev-deps unrestricted.                                                                |
| Correctness — core    | Correctly rounded (0.5 ULP) per IEEE 754                                                                 |
| Correctness — transc. | Faithfully rounded (≤ 1 ULP), documented per function                                                    |
| Test oracle           | **astro-float** (pure-Rust arbitrary-precision FP) as dev-dep                                            |
| Targets               | 64-bit hosts (dev/CI) **and** 32-bit MCUs down to **STM32U0 / Cortex-M0+** (ARMv6-M)                     |
| API surface           | Method-only; ops return `(Decimal128, Status)`; no `core::ops` overloads                                 |
| Conversions           | i/u 32/64/128, f32/f64, strings (parse + format)                                                         |
| Toolchain             | Stable Rust, MSRV pinned ~1.84                                                                           |
| Kani depth            | Encode/decode round-trip, classification, add/sub, compare, **and** multiplication                       |

The Cortex-M0+ floor is the most consequential constraint: ARMv6-M has only
`MULS` (32×32→32, no `UMULL`), no hardware divide, no FPU, no `cmpxchg` /
`ldrex`/`strex`, and tight RAM. That rules out atomics in the API, makes `u128`
LLVM lowerings expensive, and forces us to write hot multi-precision arithmetic
in terms of `u32` limbs.

## Scope (v1)

**In:** Decimal128 type, BID layout, all five IEEE rounding modes, classification,
add/sub/mul/div/fma/sqrt/IEEE-rem, total/partial compare, integer ↔ Decimal128,
f32/f64 ↔ Decimal128, string parse + format (scientific & engineering forms,
NaN/Infinity literals), exp/log/ln/log2/log10/sin/cos/tan/atan/pow, Kani
harnesses for the listed ops, proptest oracle suite, vendored IEEE/Intel BID
conformance vectors, criterion benches on 64-bit hosts.

**Out (v1):** Decimal32 / Decimal64 conversions, DPD ↔ BID conversions, signaling
NaN payload preservation across all ops (we propagate, but we don't optimize),
inverse-trig beyond `atan` (asin/acos derivable from atan), hyperbolic
functions, `core::ops` operator overloads, embedded-target benchmarks, `serde`
integration. These are listed in `FUTURE.md`-class follow-ups.

## Crate layout

Single crate `ferrodec`, no workspace. Feature flags:

```toml
[features]
default = ["fmt"]
fmt = []                # string parse + format (no alloc; uses core::fmt::Write)
transcendentals = []    # exp/log/sin/cos/pow (code-size cost — opt-in)
binary-float = []       # f32/f64 conversions (opt-in for soft-float-only targets)
kani = []               # cfg-gates Kani harness modules (off in normal builds)
```

`#![no_std]`, `#![forbid(unsafe_code)]` at the crate root for v1 (we revisit
`unsafe` only behind a feature flag if benchmarks force it).

```
src/
  lib.rs                public API + re-exports
  status.rs             Status (5 IEEE flags, packed in u8) + RoundingMode
  decimal.rs            Decimal128 newtype, constants (ZERO/ONE/NAN/...)
  bid.rs                BID bit layout: pack/unpack (sign, biased_exp, coeff_113)
  classify.rs           is_nan / is_inf / is_zero / classify / FpCategory
  cmp.rs                partial_cmp / total_cmp / min / max
  multiword/
    mod.rs              Re-exports; chooses u32-limb vs u128 path by cfg
    u113.rs             4×u32 ops: add, sub, shift, compare, leading_zeros
    u256.rs             8×u32 (or 4×u64 on 64-bit) ops for mul/div intermediates
    mul.rs              schoolbook 113×113 → 226 bits, ARMv6-M-friendly
    div.rs              Knuth Algorithm D (long division), no hw-divide path
  ops/
    addsub.rs           IEEE add/sub with rounding + Status
    mul.rs              IEEE multiply
    div.rs              IEEE divide
    fma.rs              IEEE fused-multiply-add (single rounding step)
    sqrt.rs             IEEE sqrt (Newton + final correctly-rounded fixup)
    rem.rs              IEEE remainder (signed, exact)
  convert/
    int.rs              from/to i32/i64/i128/u32/u64/u128
    binary.rs           f32/f64 conversion (feature: binary-float)
    parse.rs            str → Decimal128, fixed buffer, no alloc
    format.rs           Decimal128 → core::fmt::Write, fixed buffer
  math/                 feature: transcendentals
    consts.rs           PI, E, LN2, LN10, … precomputed at full precision
    reduce.rs           argument reduction (Cody-Waite or Payne-Hanek for sin/cos)
    exp.rs
    log.rs
    sincos.rs
    pow.rs
  verify/               cfg(kani)
    encode.rs           round-trip pack/unpack
    classify.rs         disjoint categories, NaN/Inf bit patterns
    addsub.rs           commutativity, identity, NaN/Inf propagation
    cmp.rs              total order axioms (reflexivity, antisymmetry; bounded transitivity)
    mul.rs              commutativity, identity, zero, NaN/Inf

tests/
  conformance/          vendored IEEE / Intel BID test vectors
  property/             proptest harnesses
benches/                criterion (64-bit only)
```

## Type & API skeleton

```rust
#![no_std]

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct Decimal128(u128);   // BID-encoded bits, little-endian-of-bits

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct Status(u8);
impl Status {
    pub const INVALID:   Self;     pub const DIV_BY_ZERO: Self;
    pub const OVERFLOW:  Self;     pub const UNDERFLOW:   Self;
    pub const INEXACT:   Self;
    pub const fn merge(self, other: Self) -> Self;       // bitor
    pub const fn invalid(self) -> bool;                  // ...one accessor per flag
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RoundingMode {
    NearestEven, NearestAway,
    TowardZero, TowardPositive, TowardNegative,
}

impl Decimal128 {
    pub const ZERO: Self;  pub const NEG_ZERO: Self;
    pub const ONE:  Self;  pub const NAN: Self;
    pub const INFINITY: Self;  pub const NEG_INFINITY: Self;

    pub const fn from_bits(bits: u128) -> Self;
    pub const fn to_bits(self) -> u128;

    // classify (no Status)
    pub const fn is_nan(self) -> bool;
    pub const fn is_signaling_nan(self) -> bool;
    pub const fn is_infinite(self) -> bool;
    pub const fn is_finite(self) -> bool;
    pub const fn is_zero(self) -> bool;
    pub const fn is_normal(self) -> bool;
    pub const fn is_subnormal(self) -> bool;
    pub const fn is_sign_negative(self) -> bool;
    pub const fn classify(self) -> core::num::FpCategory;
    pub const fn signum(self) -> Self;
    pub const fn abs(self) -> Self;
    pub const fn copysign(self, sign: Self) -> Self;

    // arithmetic — all return (value, Status)
    pub fn add(self, rhs: Self, rm: RoundingMode) -> (Self, Status);
    pub fn sub(self, rhs: Self, rm: RoundingMode) -> (Self, Status);
    pub fn mul(self, rhs: Self, rm: RoundingMode) -> (Self, Status);
    pub fn div(self, rhs: Self, rm: RoundingMode) -> (Self, Status);
    pub fn fma(self, b: Self, c: Self, rm: RoundingMode) -> (Self, Status);
    pub fn sqrt(self, rm: RoundingMode) -> (Self, Status);
    pub fn rem(self, rhs: Self) -> (Self, Status);

    // compare (Status only on signaling-NaN inputs)
    pub fn partial_cmp(self, rhs: Self) -> (Option<core::cmp::Ordering>, Status);
    pub fn total_cmp(self, rhs: Self)   -> core::cmp::Ordering;
    pub fn min(self, rhs: Self) -> (Self, Status);
    pub fn max(self, rhs: Self) -> (Self, Status);
}
```

Conversions live in `impl Decimal128` blocks gated by feature where appropriate
(`from_f64` etc. behind `binary-float`; `parse_str`/`fmt_*` behind `fmt`).
Transcendentals live behind `transcendentals` and follow the same `(Self, Status)`
shape, taking a `RoundingMode`.

## Multi-precision arithmetic strategy for ARMv6-M

The 113-bit BID coefficient fits in **4×u32 limbs** (top limb uses 17 bits).
Multiplication intermediate is up to 226 bits → **8×u32 limbs**. Division of a
226-bit dividend by a 113-bit divisor uses Knuth Algorithm D over u32 limbs
(no hardware divide on M0+).

Implementation policy:
1. Public API and 64-bit-host fast paths use `u128` arithmetic directly.
   `rustc` lowers this efficiently on x86_64/aarch64.
2. Hot inner kernels in `multiword/` are written as `[u32; N]` ops with
   `u32::widening_mul` (stable since 1.83) — avoiding the LLVM `__multi3` libcall
   that `u128` mul lowers to on 32-bit ARM. `cfg(target_pointer_width = "64")`
   selects the `u128` path; `"32"` selects the limb path.
3. We do **not** ship hand-written assembly in v1. `widening_mul` plus inline
   assembly only if a benchmark on a Cortex-M0+ board justifies it later.
4. No use of `core::sync::atomic::*` anywhere in the library — keeps
   ARMv6-M (no `cmpxchg`) supported with no caveats.
5. Stack budget: every public op fits in ≤ 256 bytes of stack (target,
   verified by `cargo +stable rustc -- -Z print-stack-sizes` on a 32-bit
   target build behind a developer script). No heap, no alloc.

## Test plan

### Tier 1 — Kani formal verification (`cargo kani`)

Each harness uses bounded symbolic `kani::any()` inputs with `kani::assume(...)`
to constrain to non-pathological domains where useful. Target: full proof set
runs in **≤ 30 minutes** on a developer laptop, parallelised in CI.

| Harness                            | Property                                                              |
| ---------------------------------- | --------------------------------------------------------------------- |
| `verify_pack_unpack_roundtrip`     | `unpack(pack(s, e, c)) == (s, e, c)` for valid (sign, biased exp, 113-bit coeff) |
| `verify_classify_disjoint`         | exactly one of `is_nan / is_inf / is_zero / is_normal / is_subnormal` |
| `verify_signaling_nan_distinct`    | `is_signaling_nan` ⇒ `is_nan ∧ ¬is_quiet_nan`                         |
| `verify_add_commutative_finite`    | `add(a,b,rm) == add(b,a,rm)` when neither is NaN                      |
| `verify_add_identity_zero`         | `add(a, +0, rm) == a` when `a` is finite, non-NaN                     |
| `verify_sub_self_is_zero`          | `sub(a, a, rm).0 == +0` (sign by rm) when `a` finite                  |
| `verify_nan_propagation_addsub`    | NaN in ⇒ NaN out, INVALID flag set if input is sNaN                   |
| `verify_inf_arithmetic`            | `Inf + finite = Inf`; `Inf − Inf` ⇒ NaN + INVALID                     |
| `verify_total_cmp_reflexive`       | `total_cmp(a, a) == Equal`                                            |
| `verify_total_cmp_antisymmetric`   | `total_cmp(a,b) == reverse(total_cmp(b,a))`                           |
| `verify_mul_commutative_finite`    | `mul(a,b,rm) == mul(b,a,rm)` when neither is NaN                      |
| `verify_mul_identity_one`          | `mul(a, 1, rm) == a` when `a` finite, non-NaN, no overflow            |
| `verify_mul_zero`                  | `mul(a, 0)` ⇒ ±0 (or NaN if `a` is ±Inf, with INVALID)                |

Multiplication harnesses likely need bounded exponent ranges to finish in CI
budget; when they do, the bound is documented next to the harness.

### Tier 2 — Property tests (`proptest`, `cargo test`)

* **Round-trip**: `parse_str(format_str(d)) == d` for every finite `d`.
* **Round-trip**: `Decimal128::from_i128(d.to_i128_exact()?) == d` when integer-valued.
* **Closure**: ops on valid bit patterns produce valid bit patterns (no traps).
* **Order**: `total_cmp` is consistent with `partial_cmp` where the latter is `Some`.
* **vs astro-float (core ops)**: bit-exact correctly-rounded match for +, −, ×, ÷, sqrt, fma across all rounding modes. Random + boundary inputs (subnormals, near-overflow, exact-half ties).
* **vs astro-float (transcendentals)**: `|ferrodec(x) − astro_float(x)| ≤ 1 ULP` over the documented domain of each function, plus boundary inputs.
* **Algebraic identities** (modulo NaN): `a − a == 0`, `a × 1 == a`, `a / 1 == a`, `a × 0 == 0` (finite a), `(−a) + a == 0`, `min(a,b) == −max(−a,−b)`.

### Tier 3 — IEEE / Intel BID conformance vectors

Vendor the relevant subset of:

* **Mike Cowlishaw's `decTest` suite** (`speleotrove.com/decimal/dectest.html`) — covers add/sub/mul/div/sqrt/fma/cmp/rounding edge cases.
* **Intel's BID test vectors** (from `intel-decimal-floating-point-math` source tree) — bit-exact regression vectors for every op and rounding mode.

A small `tests/conformance/runner.rs` parses each vector file and invokes the
matching op; failures print the offending vector so triage is fast.

### Tier 4 — Benchmarks (`criterion`, 64-bit hosts only)

Track regressions on add / sub / mul / div / sqrt / parse / format / exp / log /
sin. Embedded-target benchmarks deferred to a follow-up.

## CI

`.github/workflows/ci.yml` runs:
1. `cargo fmt --check` + `cargo clippy --all-features -- -D warnings`
2. `cargo build --no-default-features` + each feature combination
3. `cargo build --target thumbv6m-none-eabi --no-default-features` (Cortex-M0+ floor)
4. `cargo build --target thumbv8m.main-none-eabi --no-default-features` (Cortex-M33, STM32U5)
5. `cargo test --all-features` (proptest + conformance + unit)
6. `cargo kani --enable-stable` over the verify harness set (separate job, longer timeout)
7. `cargo bench --no-run` (compile only on PRs; full bench on `main` post-merge)

## Implementation phases (suggested order)

1. **Foundations** — `Decimal128` newtype, `Status`, `RoundingMode`, BID
   pack/unpack, classification, `total_cmp` + `partial_cmp`, IEEE constants.
   Land first Kani harnesses for round-trip + classification.
2. **Add / Sub** — alignment, subtract-cancellation, rounding step, Status flags.
   Kani harnesses + proptest vs astro-float.
3. **Multiply** — schoolbook 113×113 over u32 limbs; rounding; Kani harness
   (likely with bounded exponent).
4. **Divide / Sqrt / FMA / Rem** — Knuth-D division, Newton-with-correct-rounding-fixup
   sqrt, single-rounding fma, exact IEEE remainder.
5. **Conversions** — int (exact / rounded), f32/f64 (correctly rounded),
   string parse + format with fixed buffers.
6. **Conformance suite** — vendor decTest + Intel vectors, runner, fix all
   regressions found.
7. **Transcendentals** (feature-gated) — argument reduction + minimax / Taylor
   polynomial cores, validated faithfully-rounded vs astro-float.
8. **Polish** — benches, CI matrix, docs, `cargo doc` examples.

## Critical files (will be created)

* `Cargo.toml` — package metadata, feature flags, dev-deps (`proptest`, `astro-float`, `criterion`, `kani-verifier`).
* `src/lib.rs`, `src/decimal.rs`, `src/bid.rs`, `src/status.rs`, `src/cmp.rs`, `src/classify.rs`.
* `src/multiword/{mod,u113,u256,mul,div}.rs`.
* `src/ops/{addsub,mul,div,fma,sqrt,rem}.rs`.
* `src/convert/{int,binary,parse,format}.rs`.
* `src/math/{consts,reduce,exp,log,sincos,pow}.rs` (feature `transcendentals`).
* `src/verify/*.rs` (cfg(kani)).
* `tests/conformance/runner.rs`, `tests/property/*.rs`.
* `.github/workflows/ci.yml`, `.cargo/config.toml`, `rust-toolchain.toml`.

## Reuse from ecosystem (dev-only)

* `astro-float` — oracle for property testing.
* `proptest` — generators + shrinking.
* `kani-verifier` — `cargo kani` driver.
* `criterion` — benches.
* `anyhow` / `thiserror` — **not used**; the library returns `Status`, dev tests
  use bare `assert!`.

## Verification — how to convince yourself end-to-end

1. `cargo build --no-default-features` succeeds on host.
2. `cargo build --target thumbv6m-none-eabi --no-default-features` succeeds — proves
   the Cortex-M0+ floor compiles with no atomics, no FPU, no division.
3. `cargo test --all-features` — proptest + conformance vectors pass.
4. `cargo kani --enable-stable` — every harness in `src/verify/` proves.
5. `cargo bench` — baseline numbers recorded.
6. Optional: flash a Cortex-M0+ devkit (e.g. STM32U073) with a smoke binary that
   performs `(355 / 113).sqrt()`, dumps the result over RTT, and confirms the
   bit pattern matches the host build.
