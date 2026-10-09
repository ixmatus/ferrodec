# Wave 3 verification: the format crates

Coordinator's verdicts on the Wave 3 readers (B1, B2, B3, B4, B5),
launched at `ba92f29`. Every cited line below was opened by the
coordinator and every P0 or P1 `reproduced` finding was rerun from the
reader's scratch (`reproducers/W3-*`); severity and closure wording are
the coordinator's. Reader reports are saved verbatim under `readers/`.
Verdict vocabulary as in Waves 1 and 2.

## Coordinator probes run during the wave

- **Algorithm D, independent read.** `DecBig::div_rem`
  (decbig.rs:383-474) follows Knuth 4.3.1 D at radix 10^9: D1
  normalisation keeps `n` limbs, the D3 loop repeats while
  `q̂ ≥ B` or `q̂·v₂ > r̂·B + u` and `r̂ < B`, D4 borrows in `i64`,
  D6 adds back and discards the carry, D8 unnormalises with a debug
  checked zero residue. A Python simulation of the same algorithm on
  the in tree witness (decbig.rs:731) fires the add back exactly once
  with the correct quotient and remainder; 2,000,000 random three limb
  divisors fired it zero times. Reader B5's instrumented counter
  agrees (below).
- **Pin falsification probe** (one constant per crate, detached
  worktree, own target directory; `scratchpad/logs/pins_*.log`):
  - root `MIN_POSITIVE_NORMAL` at `PRECISION − 2`: caught by
    `decimal::tests::min_positive_normal_pins_to_one_e_minus_6143` and
    three `classify` tests. June's finding 1 is closed by a value pin.
  - d64 `ops/quantum.rs` `POW10_U64[7]` = 10,000,001: caught by
    `dectest_conformance` only (51 other suites green). No unit or
    property test reads the quantize table; the vendored ddQuantize
    rows are the sole pin.
  - d32 `ops/quantum.rs` `POW10_U64[7]`: not caught, and not
    informative: at `PRECISION = 7` the pad reaches index 6 at most, so
    index 7 is dead. Rerun at index 6 (`1_000_000` → `1_000_001`, the
    pad a one digit coefficient takes to seven digits): **not caught**
    by any of the 53 Decimal32 suites. Decimal32 has no arithmetic
    decTest file (C2 trust table), so the quantize pad table has no pin
    at all. Coordinator engineering finding (P2, instrument): a value
    pin per `POW10_*` table and one `quantize` example per pad width on
    Decimal32 (`quantize(1E+6, 1E+0)` must give `1000000`). The same
    index 6 mutant on Decimal64 is caught by `dectest_conformance`
    alone, as index 7 was.
  - multiword add back branch, rerun with `--all-features`: caught by
    `div_rem_fires_algorithm_d_add_back` and
    `isqrt_large_perfect_square`. The witness works; the earlier green
    run was the feature gate. Whether any CI or battery lane runs the
    multiword tests without `--all-features` is D1's and C5's
    question.
  - multiword add back branch disabled for `top = −1` (`if top < -1`):
    `cargo test -p ferrodec-multiword` stayed green because `decbig`
    and its witness compile only under the `alloc` feature (the probe
    omitted `--all-features`; rerun queued as part 4), while the
    downstream `ferrodec-decimal` `power_matches_high_precision_oracle`
    and `dectest_conformance` caught it. A battery or CI lane that runs
    the multiword tests without `--all-features` runs none of the
    `DecBig` tests (handed to C5's record and D1).
  - decimal `LN_GUARD = 0` (the `ln` kernel's internal guard digits,
    ln.rs:40 and :138): not caught by any of the eleven
    `ferrodec-decimal` suites. Interpretation: the bounded Ziv driver
    (strategy.rs:47-61) starts at 8 guard digits and doubles, so a
    kernel whose error exceeds its stated `LN_ERR` bound at `wp` only
    changes a delivered value on inputs within about `10^-8` relative
    of a rounding boundary, which random property inputs never are.
    The stated kernel error bound is therefore an untested premise of
    the bracket test; `power within 1 ulp` in the conformance runner
    does not see it either. Coordinator engineering finding (P2,
    instrument): a test that measures `ln_kernel`'s error at `wp`
    against mpmath on structured inputs and pins it under `LN_ERR`,
    for each kernel that `finish` consumes. Handed to B4's verdicts.
  - ieee `for_negation` identity: caught by its own unit test and by
    twenty root transcendental tests across nine suites (`cbrt`,
    `powi`, `pow`, `atan2`, `asin`, `atan`, the frozen Arb vectors),
    so the directed mode reflection is well pinned (contrast C1-5's
    gap, which is about mid range directed rows below Decimal128).

## B5: `ferrodec-multiword` and `ferrodec-ieee`

Eight findings, seven reproduced; no P0. The reader's panic, flag and
release edge reproducers were rerun by the coordinator with identical
output; every cited line says what the reader says.

| id | verdict | ruling | note |
|---|---|---|---|
| B5-1 | confirmed | P1 (public panic inside the crate's contract; not reachable from the format APIs) | `inv_pi_digits(n)` panics for `n` in 99,977..=100,000 because bigconst.rs:482 calls the public checked `pi_digits(n + 24)`; the other two stage generators use the unchecked internal cores. Rerun: 99977 and 100000 both panic "100001/100024 digits requested, outside the supported range". The lead in extended_dyn.rs:132-134 is confirmed by line: `DynArena::new` admits `prec + CONST_GUARD_DIGITS + 2 ≤ MAX_DIGITS` on the premise that `pi_digits(prec + 7)` is the deepest call, while `inv_pi` (:339-342) reaches `prec + 29`; latent because the Ziv driver stops at 56,320. Closure: route through `pi_at_scale`; a test at `MAX_DIGITS` for every generator; the `DynArena` guard restated. |
| B5-2 | confirmed | P1 (spec divergence, flags only) | ferrodec-ieee/src/binary.rs:39-40 returns INEXACT alone for a subnormal binary result; §7.5 default handling raises UNDERFLOW when the result is tiny and inexact, and the reader's digit count argument (a binary64 subnormal needs ≥ 715 decimal digits, binary32 ≥ 88) shows every such conversion from a 34 digit or narrower decimal is inexact, so the arm is always tiny and inexact. Rerun: `1E-310 → 1e-310 Subnormal inexact=true underflow=false` on Decimal128; all three siblings share the function (d64 convert/binary.rs:82, :163; d32 :82, :179). The rustdoc at binary.rs:19-25 describes the implemented behaviour, so this is a design divergence, not drift. Closure: `UNDERFLOW | INEXACT` in that arm; flag tests for `to_f64`/`to_f32` at `1E-310`, `4.9E-324`, `1E-40`; the doc restated. |
| B5-3 | confirmed | P2 (instrument) | The add back branch has one directed witness (`n = 3`, `d = 1`) whose test checks the value only; the reader's counter shows random operands never reach it (0 of 683,645 limbs) and a Knuth family forces it 21,398 times, all correct against Python. The coordinator's own simulation agrees on the witness. Closure: the family for `d = 1` and `d > 1` in the test, with the branch made observable. |
| B5-4 | confirmed | P2 | u256.rs:267-271 guards `isqrt` at 255 and 256 bits with a `debug_assert` whose message gives a false reason (every `U256` root fits `u128`); release returns `(0, self)` for `2^255` (rerun). No in tree caller exceeds 234 bits. Closure: a saturating seed makes the function total. |
| B5-5 | confirmed | P2 | Debug only preconditions return silent wrong values in release: `div_rem_u128(_, 0)` (u256.rs:186) an all ones quotient; `DecBig::sub` (decbig.rs:229) a wrapped value (`5 − 7 = 999999998`); the public `decimal_is_binary_exact` (ferrodec-ieee/src/binary.rs:95) overflows `odd *= 5` for `mantissa_bits ≥ 126` and returns a wrong `true`. All three rerun. Closure: `assert!` where the cost is nil, `# Panics` sections, and a `{F32, F64}` parameter for the ieee function (design note 3). |
| B5-6 | confirmed | P3 | bigconst.rs:437-487: the `1/π` doc block sits inside the `inv_ln10_digits` doc comment, so `inv_ln10_digits` (:487) carries no rustdoc and `inv_pi_digits` carries ln 10's; lib.rs:33-34 lists eight constants where there are nine. Confirmed by line; the `missing_docs` run accepted on the reader's output. Doc. |
| B5-7 | confirmed | P3 | status.rs:21 attributes CLAMPED to "IEEE 754-2019 §7.4" (the overflow clause); the same text is generated into `docs/references/registry-status-flags.md:36` from `references_integrity.rs:137`, and the registry prose (:27) counts "six IEEE 754-2019 §7 flags". CLAMPED is the GDA condition (ADR-0048). Doc plus the registry generator. |
| B5-8 | confirmed | P2 (instrument) | `inv_pi_digits` has no `*_PINS` table (bigconst.rs:508-555 pins the other eight generators at 50/120/220/500 digits); `KARATSUBA_THRESHOLD` (decbig.rs:56) and `GUARD` are read by no test. Closure: an `INV_PI_PINS` table from the oracle script; the Karatsuba tests derive their widths from the constant. |

Found clean, adopted: the five fixed width types on 3,613,327 random
and 5,707 directed cases against Python integers (zero mismatches;
blind spot: inputs outside each operation's stated envelope, where
wrap is by design); `DecBig` on 664,800 cases plus the 221,398
Algorithm D divisions; every `bigconst` generator exact against mpmath
at its own cap (100,000 digits; 99,976 for `inv_pi`), with the series
and perturbation lemmas re-derived; the `should_round_up` Kani harness
non vacuous (blind spot: how callers compose `round_digit` and
`sticky`, and the carry at `10^p`, which is format code);
`for_negation`; the `Status` algebra (bit values are de facto frozen by
the campaign output and the registry pin); both crates build for
thumbv6m with and without `alloc`. The brief's premises that the fixed
width types carry shifts, Algorithm D or cross width comparison do not
hold: division there is bit serial `div_rem_u128` and `div_rem10`.

Leads from "least sure", dispositioned: `to_f64`/`to_f32` ignore the
rounding mode argument (§5.4.2 honours it) goes to B3; the UNDERFLOW
doc's "after rounding" tininess rule and the formats' detection
choice for binary conversions go to B3 and the report's document
tier; `mul_pow10` asserting in release on `U256` and wrapping silently
on the wider types (design note 2) goes to C3/C4.

## B1: the root `ferrodec` core

Six findings, four reproduced; the reader built a 1.9 million case
structured differential against libmpdec and the validated oracle. Every
cited line says what the reader says; the bits, fma, scaleb and NaN
reproducers were rerun by the coordinator with identical output.

| id | verdict | ruling | note |
|---|---|---|---|
| B1-1 | confirmed | **P0** (wrong value on the interchange surface; identity bearing closure) | `src/bid.rs:1-28` documents and `pack_finite` (:166-176) implements a layout with the two high exponent bits at 126..125, the three coefficient MSBs at 124..122 and a 12 bit exponent continuation at 121..110: the DPD combination field shape with a binary coefficient. IEEE 754-2019 §3.5.2's binary encoding puts the 14 bit biased exponent contiguously at 126..113 and the significand MSBs at 112..110. Rerun: `1` encodes as `0x2208…01` where IEEE BID is `0x3040…01` (the Intel and GCC pattern); `10`, `1E1`, `1.23` likewise; `1E-6176` and `MAX` agree by coincidence of bit values. Decimal64 (`0x2238…01` vs `0x31C0…01`) and Decimal32 (`0x22500001` vs `0x32800001`) share the transcription. Arithmetic, strings and DPD are internally consistent; `to_bits`, `from_bits`, `serde_bid` and the README's BID claim are wrong against every other BID implementation (an IEEE `1` decodes here as `5.19…E-1791`). Why missed: every BID instrument is self consistent (round trips, the Kani encode/decode inverse), the only external bit pins are the four specials, which both layouts share, and dqEncode is DPD. Bead filed (P0). Closure is Parnell's decision: an ADR (persisted `serde_bid` data changes meaning), one layout module pinned by §3.5.2 literals per format, the regression literal at tests/regression_fd_7nf.rs:77 rewritten. |
| B1-2 | confirmed | P1 (flag) | Rerun: five rows `F,0,9999…9999,6111 20` (INEXACT, OVERFLOW) where the oracle gives `16` (INEXACT). fma.rs:514-518 routes an exact product of `10^6145` with a far opposite sign addend (77 or more decades) through the overflow gate; the true value lies strictly between `MAX` and `MAX + ulp`, so rounding toward zero magnitude gives `MAX` with no overflow (§7.4). A sibling of P1B-3 at a different line (flag, not value); P1B-3's closure does not reach it. Closure: deliver `±MAX` with INEXACT when `cab` is a power of ten at `10^(E_MAX+1)`, `sab ≠ sc` and the mode lowers the magnitude; regression rows with sign and mode mirrors; a property arm at the gate edges (design note 3). |
| B1-3 | confirmed | P1 (spec divergence, documented; policy decision) | Rerun: `scaleb(1, 12357)`, `scaleb(∞, 12357)`, `scaleb(1, i32::MAX)` return NaN with INVALID. quantum.rs:204-213 applies the GDA envelope before the infinity pass through, citing libdfp; the rustdoc heads the method "IEEE 754-2019 §5.3.2 scaleB" (the section is §5.3.3), under which `scaleB(x, N)` is the rounded exact product for every `N` and `scaleB(∞, N) = ∞`. libmpdec agrees with the crate because it implements the same GDA gate, so no oracle in tree decides it. Pairs with B2-5 (the siblings pass infinity through, so the three crates disagree). Closure: one rule for three crates, chosen by Parnell (IEEE: drop the gate; GDA: keep it and relabel the method); fix the section number either way. |
| B1-4 | confirmed | P2 | Rerun: `add(NaN[10^33], 1)` returns the non canonical payload unchanged; `total_cmp(NaN[10^33], NaN1)` is `Greater` while `canonicalize` says the payload is 0. nan_propagate.rs:21-27 copies `bits & T_MASK`; cmp.rs:127 compares raw payloads. Reachable only through `from_bits`. Closure: canonicalize in `nan_from` and compare canonical payloads. |
| B1-5 | confirmed | P3 | classify.rs:220 and :257 call the GDA `abs`/`minus` behaviours "§5.5.1 compliant"; §5.5.1's sign bit operations are quiet. Doc. |
| B1-6 | confirmed | P3 | `TEN` (decimal.rs:131) is read by no test by value. Test. |

Found clean, adopted: add, sub, mul, div, sqrt, fma, divide_integer,
rem, rem_near, quantize, scaleb (inside the envelope), logb, next*,
integral, reduce, the comparisons, rotate, shift, logical, NaN
propagation, zero results (65,500 cases), the clamp edge family
(222,870 cases), all against libmpdec with CLAMPED compared and the
oracle for sqrt; 400,000 raw bit fuzz cases with debug assertions on,
no panic; the constant pin table (every public constant pinned by value
except `TEN`, and the layout constants pinned only for the specials,
which is B1-1); the cast bound table (140 casts, every one bounded by a
visible invariant or a caller range, with round.rs:87's `excess as i32`
bounded only by callers). Blind spots: libmpdec shares any GDA reading
with the crate (B1-3 is the instance); the oracle models no CLAMPED.

Design notes adopted into the report: a shared layout module with an
external witness (note 1); a `docs/` table of root methods that follow
GDA where IEEE differs (note 4: scaleb, abs/neg_with_status, minNum
semantics, Division_impossible).

## B2: the Decimal64 and Decimal32 siblings, side by side

Ten findings, eight reproduced; the reader classified every mirror
hunk, ran a 150 million case structured differential against the
oracle and libmpdec, and a 14,331 row special value probe across the
three formats. Every cited line says what the reader says; the fma
witness, the probe rows and the num-traits reproducer were rerun by
the coordinator.

| id | verdict | ruling | note |
|---|---|---|---|
| B2-1 | confirmed | **P0** (shipped wrong value, nearest modes included) | Rerun: `Decimal64::fma(3000000000000001, 2500000000000001, -1.00000001)` under NearestEven returns `7.500000000000006E+30`; the exact value is `7500000000000005499999999999999.99999999` (coordinator's rational check), so `…005` is correct in NE, NA, TZ and TN. ferrodec-decimal64/src/ops/fma.rs:332-340: the overlap path aligns `c` to the working quantum by truncation and ORs its dropped digits into a positive sticky, so on effective subtraction the residue points the wrong way. Ten hits in 15 million structured Decimal64 cases; Decimal32's overlap path is unreachable (the reader's shift bound). The root's 110 digit buffer path is separate code (lead for the epic: the reader's witness generator scaled to 34 digits). Bead filed (P0). Closure: subtract the truncated side's ceiling and keep the sticky, as the sub ULP path's `h2_borrow_and_extend` already does; regression rows in all five modes; a property arm with `k·10^15 + 1` coefficients. |
| B2-2 | confirmed | P1 | Probe rerun: `abs_with_status(-NaN7)` is `-NaN7` on Decimal128 and `NaN7` on the siblings; `abs(sNaN3)` is `NaN3` with INVALID on the root and `NaN` on the siblings; `neg(NaN12)` flips the sign on the siblings. decTest ddabs525/526 and ddmns031/033 (quoted) side with the root; d64 classify.rs:189-215 returns `Self::NAN` or `self.abs()`. Why missed: ddAbs and ddMinus are among the ten d64 files the runner never dispatches (C2-6). Closure: propagate as the root does; dispatch the files under the strict NaN comparison. |
| B2-3 | confirmed | P2 (payload; flag right) | Probe rerun: `logical_and(NaN12, 0)` returns `NaN12` with INVALID on the siblings and the default `NaN` on the root; decTest ddand861 and dqinv861 expect the default NaN. Ruled P2 as C2-4 was: the flag is right, only the payload differs, and the lenient d64 `check()` hides it. |
| B2-4 | confirmed | P2 | Probe rerun: `powr(NaN12, sNaN3)` returns `NaN3` on the root (sNaN priority) and `NaN12` on the siblings; d64 transcend_impl.rs:131-137 is first NaN wins, and the trait doc (format.rs:98) states that rule, contradicting the root's `propagate_nan2`. Closure: sNaN priority in the trait contract and both impls. |
| B2-5 | confirmed | P2 (pairs with B1-3) | Probe rerun: `scaleb(∞, i32::MAX)` is NaN with INVALID on the root and `∞` on the siblings; d64 quantum.rs:302-332 runs the special cases before the envelope. Under IEEE §5.3.3 the siblings are right; under GDA the root is. One rule for three crates, Parnell's choice. |
| B2-6 | confirmed | P2 (flag) | Probe rerun: `0 / ∞` is `0E-101` with no flag on Decimal32 and `0E-qmin` with CLAMPED on the other two; d32 div.rs:215 has `Status::OK` where d64 div.rs:174 has `Status::CLAMPED` citing dddiv788; libmpdec at precision 7 raises Clamped. Constant. |
| B2-7 | confirmed | P1 (A2-1 class) | Probe rerun: `asin(0E+3)` is `0E+3` on the root and `0` on the siblings; `cbrt`, `sinh` and the other four named functions the same; `exp_m1` and `atan_pi` keep the quantum on all three. d64 trig.rs:349-356 returns `Decimal64::ZERO`. The reader's lead that root `sin`, `tan`, `sin_pi`, `tan_pi` drop the quantum on all three formats is confirmed by the probe (`sin(0E+3)` = `0` everywhere) and joins A2-1's remediation child. |
| B2-8 | confirmed | P2 | Rerun: `<Decimal64 as FromPrimitive>::from_i128(10^20)` is `None` (Decimal32 too) while the root returns `Some(1E+20)` and the inherent `from_i128` is exact; the sibling impls define only `from_i64`/`from_u64` (grep), so num-traits' defaults route through `i64`. Closure: the two overrides. |
| B2-9 | confirmed | P3 | Module docs and feature comments stating the hyperbolic functions are correct "up to the true overflow boundary" (A5-1 falsifies), the mode independent `+∞ + OVERFLOW` delivery, the `−916.98` figure and the subnormal band. Doc. |
| B2-10 | confirmed | P2 | d32 pow.rs:30 `k > 6`, sqrt.rs:140 `16 - d`/`15 - d` as literals where d64 derives from `PRECISION`; d32 quantum.rs:339-345 `unwrap_or(ZERO)` where d64 keeps `expect`. Constant and control flow. |

Found clean, adopted: add, sub, mul, div, fma (apart from B2-1),
sqrt, rem_near on 15 million structured cases per operation per
format against the oracle; CLAMPED inclusive checks against libmpdec;
quantize and scaleb; the C2-1 shape at both widths (8.7 million cases,
clean, so the sibling fma's power of ten rounding is right and B2-1
is a different mechanism); the exp family subnormal and overflow
deliveries against mpmath; the gate constants per format (the table
is adopted: every overflow gate errs high and safe, the d128
underflow gate is A5-2, and the siblings must supply at least 887.19
and 224.05 for the A5-1 fix); every `POW10_*` table sized for its
maximal index; constants pinned by value; the panic hotspots read with
the one non obvious bound derived; special value dispatch agreeing
across formats everywhere outside the table. The divergence table and
the dispatch order table are adopted into the report; the one
classification blind spot is that whole file rewrites (addsub, div,
fma, mul, rem, sqrt, round) are single hunks whose equivalence rests
on the differential.

Design notes adopted: one special value dispatcher per format pair
(every divergence the probe found sits in hand written per crate
dispatch, none in the shared kernels); the three format probe checked
in as a test with a width allow list.

## B4: `ferrodec-decimal`

Fourteen findings, all reproduced; the coordinator reran the reader's
script minus its two multi gigabyte cases with identical output
(`reproducers/W3-B4/repros_coordinator_rerun.out`). Every cited line
says what the reader says.

| id | verdict | ruling | note |
|---|---|---|---|
| B4-1, B4-2, B4-3 | confirmed | **P0** (one class, three witnesses) | Rerun: `pow(4, 0.5)` under Down, Floor and ZeroFiveUp returns `1.99999999` after 2.4 s at the Ziv cap (HalfEven `2.00000000` in 164 µs); `pow(1.5625, 0.5)` at `p = 2` under HalfUp returns `1.2` (true `1.25`, tie); `pow(10, 10001)` under Down returns `9.99999999E+10000` and under HalfEven the right value with a spurious Inexact; `pow(0.1, 999999)` under Ceiling `1.00000001E-999999`; `pow(-1, 100000)` under Ceiling and Up returns `2`, under Floor `pow(-1, 100001)` returns `-2`. Mechanism, confirmed by line: `general_power` (pow.rs:187) has no exactness test; `integer_power` (:157) declines when `base_digits · n > 20000`; :113 excludes `x = -1` from the `|x| = 1` shortcut, so `ln_kernel` receives the value its precondition forbids and a sticky zero rounds `1` up. The bracket in `finish` (strategy.rs:47-61) cannot decide an exact or midpoint value, and `faithful_round` at the cap rounds the low side. The crate's contract (lib.rs:20-23) says `power` is correctly rounded with the context's mode. Bead filed (P0). Closure: exactness from the mathematical result before `general_power`; the 55 decTest rows rescued by the band pinned exactly (B4-12). |
| B4-4 | confirmed | P1 (the ADR-0040 residual is false as stated) | Rerun: a crafted 20,000 digit operand (`ln(1.000000005)` to 20,000 digits) drives `exp` and `ln` to the cap in 1.7 and 2.1 s and returns `1.00000000` where the true value sits above the midpoint (`1.00000001` is correct), with `Status(16)`, identical to a decided result. ADR-0040's "astronomically unlikely", "never silently claimed correct" and "wrong by at most one ulp" are each falsified (B4-3 is off by 10^8 ulp). The cap is bounded (12 kernel runs, about 2.5 s at small precision, 6.6 s at `p = 5000`), so this is not a hang, but a wrong result indistinguishable from a right one. Closure: a distinguishable outcome (a status bit or a result type) or a decided bracket with a stated resource bound; ADR-0040 rewritten. |
| B4-5, B4-6 | confirmed | P1 (one type invariant; P0 by the letter) | Rerun: `1E-2147483648 × 0.1` at `emin = i32::MIN` returns `1E+2147483647` with a clean status; `sqrt(2)` at `p = 2^31 + 1` returns `1.41E+2147483648`; `1/3` at `p = u32::MAX` returns `0`; `fma` at `p = u32::MAX` panics at arith.rs:251 and :272 (the Wave 1 lead). `Context::new` (context.rs:137) validates nothing; GDA itself caps precision and exponents at 999,999,999, and the wrap sites are the `as i32` casts in round.rs:43-171, next.rs:177-184, divrem.rs:74, sqrt.rs:76, strategy.rs:79. Ruled one P1 because every witness needs a context outside GDA's own range, which the type admits; the closure is a validated `Context` (constructor returning `Option`, precision ceiling, `emin − (p − 1) ≥ i32::MIN`). |
| B4-7 | confirmed | **P0** (panic from the public API on ordinary input) | Rerun: `to_decimal128` of a NaN with a 40 digit payload and `to_decimal32` of `NaN1234567` panic at interop.rs:40 and :54 (`expect("arbitrary Display is a valid decimal string")`); `parse_str` accepts such NaNs and `add` at `p = 40` produces one. Bead filed (P0). Closure: truncate the payload to the target's capacity, or bridge parts directly. |
| B4-8 | confirmed | P1 | Rerun: `to_f64(1)` reports INEXACT (D3-6, confirmed); `to_f64(1E-310)` no UNDERFLOW (B5-2's class, through the same ieee function or its own path); `to_f64(1E+400)` under TowardZero gives `inf` though the doc says `rm` governs the edges; `-NaN` loses its sign. The parameter is `_rm` (to_float.rs:41). Closure: exactness by round trip, mode aware edges, the NaN sign, or the doc corrected. |
| B4-9 | confirmed | P1 | Rerun: `divide_integer(11E+9, 1)` at `p = 16`, `emax = 9` returns `11000000000` with a clean status; libmpdec gives Infinity with Overflow. divrem.rs:126-130 checks digits against precision only. Reachable only when `emax < p − 1`. Control flow. |
| B4-10 | confirmed | P1 | Rerun: `reduce(1000E+381)` at `p = 16`, `emax = 384`, `clamp = 1` returns `1E+384` with CLAMPED set while libmpdec folds to `1.000000000000000E+384`; `quantize(1E+370, 1E+380)` returns `0E+380` where libmpdec gives `0E+369` with Clamped. quantize.rs:124-134 strips after clamping without re clamping; quantize.rs:62 packs `eb` directly. Control flow. |
| B4-11 | confirmed | P1 (resource; ADR-0053 class; security relevant) | Rerun: `pow(2, 1E+2147483647)`, a 12 character operand, peaks at 1.91 GB resident before overflowing to Infinity in 0.52 s; pow.rs:349 `integer_magnitude` materialises `coeff · 10^exp`. Bounded by the `i32` exponent, so not unbounded, but a short attacker supplied operand costs gigabytes. Closure: decide `n > EXACT_DIGIT_CAP` from `digits + exp` before materialising. |
| B4-12 | confirmed | P2 (instrument) | conformance.rs:469-484 passes any flags and cohort once two finite values are within one ulp; the reader's instrumented replay lists 55 `power` rows that pass only through the band, every one an exact integer power with a flag or cohort error (B4-2, B4-3), none the reference rounding disagreement the runner, ADR-0040 and `pow_oracle.rs` say the band exists for. Closure: the band only when both results carry Inexact; flags compared otherwise. |
| B4-13 | confirmed | P2 | Debug build `i32` overflow panics on legal operands (quantize.rs:50, :53, :91, pow.rs:336, next.rs:184); release wraps and four of five give the right answer. Accepted on the reader's dev build output and the lines. Type: compute in `i64`. |
| B4-14 | confirmed (against libmpdec) | P2 | arith.rs:325-329 keeps `precision` payload digits under `clamp = 1` where libmpdec keeps `precision − 1`. The GDA text was not checked by either party. Constant. |

Found clean, adopted: `DecBig` normal form as the crate uses it; the
arithmetic differential against libmpdec (270,000 structured cases
over nine precisions, all eight modes, both clamp settings) clean
outside the B4-9, B4-10, B4-14 classes; the transcendental
differential against mpmath (10,843 random cases to `p = 10,000`)
clean, with the blind spot that random operands never hit an exact
result or a midpoint; the decTest replay at the pinned counts; the
ADR-0053 guards on every alignment path (the table is adopted; the
one unguarded path is B4-11); the rounding core for in range
contexts; `from_float`; exact case detection in `exp`, `ln`, `log10`
complete for rational operands. The 99 skips, by class, each with its
reason true of the code today; the `rescale` rows fall through a
catch all `Skip` arm that would also hide a misspelt operation
(engineering P3 for the runner). The "whole GDA specification" claim
(lib.rs:17) against a `Status` with no Rounded, Subnormal or
Lost_digits goes to D2's successor list as P3.

Design notes adopted: a validated `Context` type (notes 1); a `Work`
zero that cannot carry a positionless sticky (note 2); a three state
Ziv outcome (note 3); exactness from the result, not from input caps
(note 4).

## B3: the I/O boundary on all three formats

Fifteen findings, fourteen reproduced; the reader wrote the threat
model first and ran differentials against CPython `decimal` for parse
(40,000 strings), integer conversion (180,000), float conversion
(18,000 and the midpoint aimed `from_f64` set), rendering (24,000), and
90,000 raw bit patterns through every formatter. Every cited line says
what the reader says; the cost, format, float, payload and serde
reproducers were rerun by the coordinator.

| id | verdict | ruling | note |
|---|---|---|---|
| B3-1 | confirmed | P1 (resource; security relevant) | Rerun: `"1E-1000001"` parses in 2.57 s of CPU on Decimal64 and 2.59 s on Decimal32 (0.00 s on Decimal128). The ADR-0057 sentinel exponent (`−2·10^9`, d64 parse.rs:392) reaches `drop_excess_digits` (d64 round.rs:204-222), whose loop runs once per dropped digit with no exit once the coefficient is zero. Reachable through `parse_str`, `FromStr`, `from_str_radix` and serde `Deserialize`; the parse rustdoc (parse.rs:160-164) promises "cost bounded by input length, not by input value". Values and flags are right. Lead for the epic: every other caller of the sibling `round_and_pack_finite` with an unclamped exponent inherits it. Closure: an O(digits) drop; a timed test per sibling. |
| B3-2 | confirmed | P1 (rendered precision misrepresented) | Rerun: `98765E-20` under `{:.2}` on the `Engineering` adapter prints `988.00E-18` on all three formats. format.rs:284 quantizes to `N + 1` significant digits (the scientific layout's rule) and :645-650 pads the two or three digit engineering mantissa with zeros, so the string claims five significant digits of which two are fabricated; the value is the correct three digit rounding, so no number is wrong, but the precision contract (`N` fractional digits of the engineering mantissa) is not met and ADR-0058:65's premise ("the precision path already quantizes the mantissa for that layout") is false. Closure: quantize to `eng_exp − N` after choosing the engineering exponent; pins per format. |
| B3-3 | confirmed (duplicate of B5-2) | P1 | Same function (ferrodec-ieee/src/binary.rs:39), same closure; the reader adds the smallest normal boundary case and the ADR-0056 consequences paragraph to the doc work. One remediation child. |
| B3-4 | confirmed | P1 (contract; the ADR's claim is false) | Rerun: `from_f64(0x43c957633e24d15b)` on Decimal64 under NearestEven gives `3.652074584957696E+18` where the exact binary value `3652074584957695488` rounds to `…695`; Decimal32 `1.999102E-9` where the exact value `1.9991014999…E-9` rounds to `…101`; `from_f64(0.1, TowardPositive)` returns `0.1` below the true value with a clean status on every format. d64 binary.rs:244-257 and root :265 parse the shortest round trip string (`{x:e}`), which is itself a nearest rounding; ADR-0056:49-55 says the double rounding hazard "is gone" and the value "rounds once". Ruled P1 rather than P0: ADR-0056 deliberately defines `from_f64` as the shortest string's conversion (decision 1, `from_f64(0.1) = 0.1`), so these are the documented contract's consequences stated falsely, and the IEEE `convertFormat` reading is a decision for Parnell. Closure: an exact `m·2^e` conversion (design note 3) or an ADR-0056 addendum stating the contract for strings longer than `P` and for the directed modes. |
| B3-5 | confirmed | P1 | Rerun: Decimal64 accepts `NaN1000000000000000` and Decimal32 `sNaN1048575`, minting non canonical NaNs that `is_canonical()` rejects and that propagate through arithmetic; the root rejects the 34 digit analogue (parse.rs:615 `>= MAX_CANONICAL_NAN_PAYLOAD`, the fd-aqs.13 fix) while the siblings test `> T_MASK` (d64 parse.rs:622). June's finding 3 was fixed on one of three copies. Closure: the root's bound and its regression test ported. |
| B3-6 | confirmed | P1 (a documented adapter that cannot read its own output) | Rerun: `serde_bid` on Decimal128 serializes `1` as the JSON integer `45235290231555418299757684020165476353`, which serde_json hands back as `f64` to a visitor with no `visit_f64` (serde_impls.rs:146 `deserialize_any`), so every Decimal128 value fails to round trip in JSON; Decimal64 and Decimal32 round trip because their bits fit `u64`. Ruled P1 above the reader's P2: the adapter is unusable in its most common format and the test file documents the gap instead of covering it. Closure: a string form in human readable mode. |
| B3-7 | confirmed | P2 | Rerun: a `from_bits` NaN with payload `2^110 − 1` prints `NaN1298074214633706907132624082305023`, which `FromStr` rejects; format.rs:266-271 writes the raw trailing field where `canonicalize` and the DPD encoder treat it as zero. Closure: print the payload only when canonical. |
| B3-8 | confirmed | P2 | Rerun: `{:>12}` and `{:+}` print `12345` unpadded on every format and adapter. Closure: render into the stack buffer and `pad_integral`. |
| B3-9 | confirmed | P2 (values for D3-4) | `3 % 2` is `−1` on Decimal128 (`rem_near`) and `1` on the siblings (`rem_trunc`); `f64 %` truncates. The policy and the ADR-0027/0029 amendment are Parnell's. |
| B3-10 | confirmed | P2 | `ToPrimitive::to_i64(2.7)` is `3` (num_traits_impls.rs:26 `RM = NearestEven`) where `2.7f64.to_i64()` is `2`. Constant or doc. |
| B3-11 | confirmed (overlaps B4-8 for `ferrodec-decimal`) | P2 | `to_f64(rm)` ignores `rm` on every crate; the rustdoc says so, §5.4.2 does not. Control flow or drop the parameter. |
| B3-12 | confirmed (B2-8 plus the `signum` row) | P3 | The siblings' `signum(-sNaN)` drops the sign where the root keeps it. Joins B2-8's child. |
| B3-13 | confirmed (NaN case reproduced; rest by reading) | P3 | Six stale or false doc sites at the float boundary (root `to_f64` "bit-exactly" for NaN, the d64 module doc describing retired paths, ADR-0056's failure (3) versus the `expect`, the serde test header, the d32 DPD exponent bound). Doc. |
| B3-14 | confirmed | P3 | `serde_bid` integer visitors sign extend `i64`/`i128` and zero extend the narrower types; JSON `-1` becomes a non canonical `-sNaN` on two formats and an error on the third. Control flow. |
| B3-15 | confirmed | P3 | `Sum` folds from `+0E+0` (iter.rs:19-23): `sum([-0])` is `+0` where `f64` gives `−0`, and `sum([1E+5])` loses the cohort. Constant. |

Found clean, adopted: finite parsing on all three formats (value,
cohort, saturation, zero clamping, specials, syntax errors) against
libmpdec with the one class of mismatch being B3-5; toSci and toEng
without precision; formatter totality on 90,000 raw bit patterns (the
32 panic sites in format.rs unreachable by that evidence plus reading);
integer conversions per §5.8 on 180,000 cases; the DPD codec on all
1,024 declets at both positions against a table derived from IEEE 754
Table 3.3; `to_f64` values correctly rounded; `Neg` on NaN; the
num-traits predicates. Blind spots the reader named: CPython accepts
spellings GDA rejects (excluded by rule); the Subnormal and Rounded
flags are not modelled; serde formats other than JSON and bincode not
run; `{:.N}` with `N ≥ 2^31` (a debug overflow at format.rs:212,
reachable only on the 1.84 to 1.86 MSRV window) not run.

Leads dispositioned: the other callers of the sibling
`round_and_pack_finite` with unclamped exponents (B3-1's class) go to
the remediation child; non canonical NaN propagation through
arithmetic is B1-4; the three design notes on a strict parser, an exact
float kernel and a padded `Display` go to C4 and D1.

## Wave 3 summary

- P0, four new classes: the BID bit layout on all three formats
  (B1-1), the Decimal64 `fma` overlap path (B2-1), `ferrodec-decimal`
  `power` on exact and midpoint results (B4-1, B4-2, B4-3), the interop
  NaN payload panic (B4-7). Ten P0 beads are open across the arc.
- P1, nineteen: `fma` OVERFLOW at the `10^(Emax+1)` binade (B1-2), the
  scaleb envelope (B1-3 with B2-5), sibling `abs`/`neg` NaN handling
  (B2-2), sibling zero row cohorts (B2-7), the Ziv cap residual
  (B4-4), the unvalidated `Context` (B4-5, B4-6), `to_f64` on
  `ferrodec-decimal` (B4-8), `divide_integer` without `emax` (B4-9),
  `clamp = 1` ignored (B4-10), `pow` materialising `10^exp` (B4-11),
  the sibling parse cost (B3-1), the engineering precision (B3-2),
  `from_f64`'s contract (B3-4), the sibling NaN payload gap (B3-5),
  `serde_bid` JSON (B3-6), `inv_pi_digits` at its cap (B5-1), UNDERFLOW
  on subnormal binary results (B5-2 with B3-3).
- Refutation rate: no finding refuted; three duplicates across readers
  (B3-3/B5-2, B3-11/B4-8, B3-12/B2-8), which is the ownership rule
  working as designed on a shared function; two severities raised by
  the coordinator (B3-6, and B2-7 to A2-1's level), two lowered (B2-3,
  B4-5/6 to one P1 class), one P0 proposal reclassified as P1 on the
  crate's own contract (B3-4).
- Coordinator probes: the Algorithm D add back witness is genuine; the
  Decimal32 quantize pad table has no pin; `LN_GUARD` is absorbed by
  the Ziv guard so the kernel error bound is an untested premise;
  `for_negation` is well pinned; `MIN_POSITIVE_NORMAL` is pinned by
  value.
- June's verdicts revisited: the arithmetic pipelines stay clean under
  about 170 million structured cases; "codecs clean" was wrong for BID
  (the June instrument was the same self consistent round trip); the
  add back branch is now covered by one genuine witness.

## Handed to Wave 4

- C3: the three `extended*.rs` copies and the one fix that landed in
  one of them; the literal `33`; the `2^α 5^β` assemblies; `exact.rs`
  under `trig-pi`; the budget units; public `parse_str`/`recip`/`sqrt`.
- C4: `fma`'s four regimes and comment proven gates (root and d64);
  the eleven special case dispatchers; the sibling `drop_excess_digits`
  loop; the string bridges at the float and interop boundaries;
  `Context` and the `as i32` sites; `Work`'s sticky zero; the one ulp
  `power` band; d32 literals versus d64 derivations; the clippy allow
  list against 1,216 `as` casts.
- D1: three `bid.rs` layouts; the num-traits, `%`, `ToPrimitive`,
  `to_f64(rm)`, `serde_bid`, `Sum` and scaleb asymmetries as the API
  consistency table's seed; the multiword `--all-features` test
  selection; the `DecimalFormat` NaN contract; the GDA versus IEEE
  policy choices as an undocumented table.
- D4: the CLAMPED attribution in the generated registry text; the
  `bigconst` doc merge; entries for Knuth, Lauter and Lefèvre, Niven,
  Payne and Hanek, astro-float's `abs_cmp`, the oracle versions; the
  README's BID claim as a disclosure diff proposal.
- Reserve, if used: the root `fma` overlap analogue at 34 digits (B2's
  witness generator scaled), the root `sin`/`tan` zero rows, and the
  other callers of the sibling `round_and_pack_finite`.

