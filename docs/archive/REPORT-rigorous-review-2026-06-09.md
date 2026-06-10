# Rigorous codebase review, 2026-06-09

> **Archival note.** One shot engagement artifact, archived under
> `docs/archive/` per repository convention. State at review time:
> `main` = `3b9f063`, clean tree, all three fixed format crates at
> 3.3.0, `ferrodec-decimal` at 1.0.1, `cargo test --workspace` green.
> The remediation backlog this report produced lives in the beads
> tracker under the `fd-` prefix; durable outcomes of that backlog
> (ADR amendments, fixes) supersede this snapshot.

## Method

Six parallel review agents covered coherent slices: `ferrodec-multiword`,
the `ferrodec-transcend` Extended kernel, the root Decimal128 crate, the
Decimal64 and Decimal32 siblings, `ferrodec-decimal`, and the
verification infrastructure itself. Each agent read `docs/testing.md`
and the governing ADRs before judging code, and was required to state,
for every finding, why the existing oracle stack would not have caught
it. Every critical claim was then independently re-verified at runtime
in a scratch crate against the path dependencies, with CPython's
`decimal` (libmpdec, correctly rounded) and mpmath at 100 to 120 dps as
oracles. One reviewer claim was refuted on execution and discarded; all
findings below survived adversarial verification.

## Verdict

The architecture is coherent, the arithmetic surfaces are genuinely
closed by the exact oracle and decTest, and the layered verification
stack is honest about its blind spots. The confirmed bugs cluster
exactly where `docs/testing.md` names the residual frontier:
transcendental behavior near the additive anchors 0 and 1, directed
rounding modes, named constants no oracle reads, and pathological
contexts decTest never exercises. ADR-0032 and ADR-0033's claim of
correctly rounded transcendentals on every canonical input is falsified
in specific bands.

## Confirmed critical findings (runtime witnessed)

### 1. `MIN_POSITIVE_NORMAL` is wrong on all three fixed formats

The constant encodes `1E-33` (Decimal128), `1E-15` (Decimal64), and
`1E-6` (Decimal32) instead of the documented `1E-6143`, `1E-383`, and
`1E-95`. The formula at `src/decimal.rs:155` (mirrored in both
siblings) places `BIAS - PRECISION + 1` in the biased exponent field,
which equals `-E_MIN` rather than `E_MIN + BIAS`; the correct biased
value is `PRECISION - 1`. Two reviewers found this independently. The
`is_normal` and `is_subnormal` predicates are correct; only the
constant is wrong. No test pins the constant's numeric value: the
existing assertions check only `is_normal()`, which the wrong value
also satisfies, and no oracle ever reads a named Rust constant.

### 2. `Decimal64::quantize` rejects representable pads of 10 to 15 digits

`1 quantize 1E-10` should produce `1.0000000000` with `OK`; it produces
`(NaN, INVALID)`. The `POW10_U64` table at
`ferrodec-decimal64/src/ops/quantum.rs:34` holds ten entries, the
Decimal32 size; Decimal64 legitimately needs pads up to 15 (any
`digits + pad <= 16`). Confirmed for every pad in 10..=15. The vendored
ddQuantize corpus never exercises a successful pad above 9, so the
zero fail pin could not see it; quantize is outside the exact oracle,
the cross precision check, and the finite Kani surface.

### 3. Anchor absorption in the transcend kernel falsifies ADR-0032 in bands

The root disease: formulas that hand `1 ± tiny` (or `x` with
`x^2` below resolution) to a 50 significant digit representation turn
the kernel's `1e-49` relative error model into `1e-49` absolute.
Oracle confirmed:

- `atanh` and `asinh` near 0: arguments below `~5e-51` relative return
  `+0` outright (witness: Decimal32 `atanh(1E-95)` returns `0`; the
  correct result is the argument itself, a normal representable
  value). In the mid band, Decimal128 results are wrong by up to
  3e8 ULP (witness: `atanh(9.876543210987654321098765432109876E-25)`
  returns `...245389420` where the correct 34 digit result is
  `...432`); Decimal64 `atanh(1.536385851986827E-44)` returns
  `1.536390000000000E-44`, wrong at the sixth digit.
- `ln` and `log10` just below 1 at Decimal128: the decade recombination
  `(taylor + 3 ln 2) - ln 10` cancels catastrophically. Wrong by up to
  ~4e7 ULP for messy coefficients (witness:
  `ln(0.9999999999999999999999987654321098)` returns
  `-1.2345678902000000000000008E-24` where libmpdec's correctly
  rounded value is `-1.234567890200000000000000762078938E-24`). Clean
  inputs of the form `1 - 10^-k` happen to round correctly, which is
  why spot checks pass. `pow` inherits the defect through
  `y * ln(x)` (last digit misrounds confirmed against libmpdec).
- `acos` near 1 at Decimal128: `1 - x^2` squared at 50 digits loses
  relative accuracy; wrong by up to 1.5e6 ULP (witness:
  `acos(0.9999999999999999999999987654321098)` off by 1.3e6 ULP).
  `asin` violates the ADR-0032 proof envelope (errors of 0.4 to 0.5
  ULP measured) without an observed misround.

The fix pattern already exists in tree: `acosh` switches to a `log1p`
path for `x - 1 < 0.01` (`ferrodec-transcend/src/hyperbolic.rs`), and
the exact factoring `(1-x)(1+x)` avoids the squaring loss.

### 4. Directed rounding is broken on the transcendental surface in four ways

- **Small arguments.** When `f(x) = x + c x^3 + ...` and the correction
  sits below 50 digit resolution, the kernel's value is exactly `x`
  and the handoff (`Extended::to_format`, `pre_sticky` hardwired
  `false`) carries no enclosure, so directed modes land on the wrong
  grid point by 1 ULP. Witness: Decimal32 `sin(1E-40)` at
  TowardNegative returns `1E-40`; correct is `9.999999E-41`. Affects
  sin, cos, tan, asin, atan, sinh, tanh, asinh, atanh, exp, exp2 near
  0 on all formats. NearestEven is unaffected.
- **Exact results.** The 50 digit approximation of an exact result can
  sit on the wrong side of it. Witness: `exp2(3)` at TowardNegative
  returns `7.999999` (Decimal32); `log2(1024)` at TowardNegative
  returns `9.999999999999999999999999999999999` (Decimal128).
- **Mode blind overflow and underflow gates.** `exp_from_extended` and
  `saturate_extreme` (`ferrodec-transcend/src/exp.rs`) return
  `(Inf, OVERFLOW)` and `(0, UNDERFLOW)` without consulting the
  rounding mode. Witness: `exp(14151)` at TowardZero returns `+Inf`
  where IEEE 754-2019 §7.4 requires Nmax; `exp(-14222)` at
  TowardPositive returns `0` where the correct result is `1E-6176`.
  The sinh and cosh saturation path handles directed overflow
  correctly, so the right plumbing exists in tree.
- **Negation after rounding.** `asin(±1)`, `atan(±Inf)`, and the
  `atan2` quadrant constants round the magnitude under the caller's
  mode and then negate. Witness: `asin(-1)` returns `-...751` at
  TowardNegative and `-...752` at TowardPositive; the two are exactly
  swapped. `cbrt` already fixed this class with `rm.for_negation()`.

Additionally `tanh(100)` at TowardZero returns `1` where the correct
result is 34 nines (the saturation path ignores the mode), and exact
`exp2`, `log2`, `log10` results raise spurious INEXACT (witness:
`log10(1000)` returns `3` with INEXACT set). ADR-0047's Context claim
that only cbrt and pow can land on exact results is false for these
three; its `exact.rs` machinery is the natural extension point.

### 5. `ferrodec-decimal`: exponent cast wrap and allocation ordering

- `combine_finite` (`ferrodec-decimal/src/arith.rs`) computes
  `(ea - min_e) as u32`. For fma the product exponent is
  `ea + eb`, up to ±4.3e9, so the gap to the addend's exponent reaches
  6.4e9, above `u32::MAX`, and the cast wraps: silently wrong finite
  results from in range operands. Plain add and subtract gaps fit
  `u32` exactly and are safe.
- `quantize` (`quantize.rs:42`) and `integer_divide` (`divrem.rs:193`)
  materialize `10^(exponent gap)` before the precision validity check,
  so operands like `1E+2147483647 quantize 1E-2147483648` demand a
  multi gigabyte allocation before returning the NaN they were always
  going to return. The add path's equivalent exposure is documented;
  these two are not.
- `Context` never enforces `precision >= 1` despite its documentation;
  precision 0 silently drops every digit. The principled fix is a
  `NonZeroU32` precision type.

decTest cannot see any of this: its contexts are small and its
exponents bounded.

### Refuted during verification

The root crate reviewer claimed an fma directed rounding defect at a
power of ten cohort boundary (`fma(1E+17, 1E+18, -1E-100)` at
TowardZero) with a hand traced witness. The library returns the correct
value on that witness; the finding was discarded. The lesson stands:
reviewer hand traces require execution before belief.

## Verification infrastructure findings

- **Comment stripping eats quoted operands.** `strip_comment`
  (`ferrodec-test-support/src/conformance.rs:39` and the root copy)
  cuts at the first `--` anywhere in the line. Six vendored
  adversarial parse vectors (`toSci '--1'` and `toSci '1E--1'` in
  ddBase, dsBase, and base) tokenize to `None` and vanish from every
  bucket, including the skip tally. The library's own parse of these
  strings is correct; the harness simply never asserts it.
- **CI lanes.** No CI lane runs the `ferrodec-decimal` test suite (the
  27591 vector conformance run), the `ferrodec-multiword` tests, or
  the `ferrodec-transcend` unit tests; the root and Decimal64 `dpd`
  Kani harnesses are gated on a feature no CI lane enables (only the
  Decimal32 lane passes `dpd`). The README's claim that CI runs the
  full test and verification suite is not currently true.
- **Metamorphic tautology creep.** The `acosh` identity probes only
  inputs on the standard `ln(x + sqrt(x^2 - 1))` branch, where both
  sides share the `ln` kernel; ADR-0025 kept the identity on the
  strength of the `log1p` path, which the probe set
  (`x >= 1.5`) never enters.
- **The trig reduction constant is 38 digits, not 80.** The Payne and
  Hanek residual multiplies `PI_OVER_TWO_COEF_38`
  (`ferrodec-transcend/src/argred.rs:819`), 38 truncated digits; the
  module header and ADR-0032 say 80. The in code bound (at most
  `1e-3` ULP, called negligible) is larger than the Decimal128 cos
  sampled worst margin (`4.051e-4` ULP), so the written derivation
  does not discharge ADR-0032's obligation for Decimal128 trig; the
  measured truncation (`6.3e-5` ULP) does, but only against sampled
  margins, and the truncation bias is one sided and coherent across
  formats, the exact correlated shape `docs/testing.md` warns about.
- **Coverage and pinning.** The Decimal128 crate vendors 32 dq files;
  dqBase, dqCopy family, dqRemainder, dqToIntegral, dqPlus, dqMinMag,
  and dqMaxMag are absent even though the Decimal64 suite vendors
  their dd equivalents (and the harness `Plus` implementation would
  mishandle sNaN if dqPlus ever landed). The transcend frozen corpus
  is pinned by a floor (`vectors.len() > 500`) rather than per
  function and mode counts, and is not covered by any SHA256SUMS
  manifest (ADR-0042 scopes itself to decTest).
- **Non binding Kani covers.** The `kani::cover!(false, ...)` arms in
  `src/verify/encode.rs` do not fail verification when unsatisfiable;
  the harnesses prove less than their doc comments claim. The arms
  should be asserts.
- Stale harness prose post ADR-0032 (faithful contract wording in
  `frozen.rs` and `differential.rs`, a 2 step differential band where
  the contract now implies 0), and a double counted delta in the
  conformance mismatch diagnostic.

## Design and documentation tier

- `from_f64` semantics diverge silently: the parent renders shortest
  round trip (so `from_f64(0.1)` gives `0.1`, `OK`) while the siblings
  render `{:.17e}` and re-round (giving `0.1000000000000000`,
  INEXACT, with a structural double rounding hazard). No ADR records
  the divergence.
- `to_f64` and `to_f32` raise INEXACT unconditionally, including on
  exact conversions (`Decimal128::ONE.to_f64` returns INEXACT): the
  invented flag class ADR-0047 fixed for cbrt and pow.
- `serde_bid::deserialize` uses `deserialize_any`, which
  non self describing formats (bincode) reject at runtime; the module
  doc advertises bincode.
- `parse_str` accepts NaN payloads in `[10^33, 2^110)`, minting values
  for which `is_canonical()` is false: the only arithmetic entry path
  that can produce a non canonical bit pattern.
- The `cmp.rs` module header states the §5.10 NaN ordering backwards;
  the implementation and the decTest pin are correct.
- `ferrodec-multiword`: the Algorithm D add back branch has zero test
  coverage anywhere in the repository (fire probability ~2e-9 per
  quotient limb on random operands). A reviewer validated it with a
  line faithful Python mirror against ~560k adversarial trials, zero
  mismatches; the directed witness (dividend limbs
  `[0, 0, 500000000, 499999999]`, divisor `[1, 0, 500000000]`, little
  endian) deserves to be committed as a unit test.
  `DecBig::from_ascii_digits` is the lone public door past the normal
  form invariant, guarded only by `debug_assert`.
- The Decimal64 DPD codec's "Kani verified" phrasing overstates: the
  Kani harnesses prove declet decode totality and special value round
  trips; the bijection rests on exhaustive declet level unit tests
  plus a sampled proptest, as the module doc itself records honestly.
- Assorted doc nits: d32 prose left in d64 files (NaN payload field
  width, format buffer header), Form B range arithmetic in `bid.rs`
  comments, a dangling pointer to a nonexistent
  `tests/property_addsub.rs` in sibling verify modules, a stale
  pre fd-d47 alignment description in the d64 addsub header.

## Found clean

`ferrodec-multiword`'s carry, borrow, and division kernels (Algorithm D
re-derived independently and mirror fuzzed); `ferrodec-ieee`'s rounding
kernel, read line by line, with its Kani equivalence proof judged sound
and non vacuous; BID and DPD codecs and canonicalization; total order
and the comparison surface; the ADR-0048 CLAMPED rework and its
operand reparse residual predicate; the addsub, mul, div, sqrt, rem,
and fma machinery on all three formats (modulo the items above); the
bounded Ziv loop in `ferrodec-decimal` (terminates, honest about its
faithful fallback); the exact oracle's independence and its soundness
gate; ADR-0042 fixture integrity (all four manifests re-verified byte
for byte); the Kani inventory generally (no vacuous assumes found in
sampled harnesses; the ADR-0015/0016 scope policy is honored); and the
`consts.rs` 55 digit constants (re-derived against mpmath, all within
`5e-55`).

## Why the stack missed the confirmed bugs

Verified per finding, not assumed: the Arb corpus uses coefficients of
at most 8 digits and its per function decades stop well short of the
anchor bands; the astro-float suites assert the faithful (at most
1 ULP) contract, so every 1 ULP directed misround passes by
construction; the exhaustive Decimal32 sweep searched NearestEven half
ULP margins on the Arb side and executed the kernel at only the 18
worst margin inputs; decTest has no transcendentals and never reads
named constants; the differential is off by default with coefficients
of at most 6 digits; and the cross precision check routes both sides
through the same shared kernel, so format independent kernel errors
correlate: precisely the "one failure observed through many windows"
hazard `docs/testing.md` documents.

## Remediation program

Filed as a beads tree (umbrella plus slices) at review close, in this
order: the two constant and table fixes first (small, shippable);
the `ferrodec-decimal` cast and allocation guards; the localized
directed mode fixes; the anchor absorption rework with its own ADR
amending ADR-0032/0033 and a near anchor regression band suite
(anchor decades crossed with all five modes on all three formats);
an enclosure channel across the `to_format` seam; the exactness
extension to exp2, log2, log10; the verification infrastructure
batch; and the design and documentation residue.
