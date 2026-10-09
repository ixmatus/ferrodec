# Deep review, 2026-10

Branch `review-2026-10` off `main` at `0a5be4c`; the review tip is named
in the closing commit. Companion material: the plan
(`docs/decisions/plans/2026-10-08-deep-review-plan.md`), the per wave
verification records, reader reports, briefs, Phase 1 artifacts and
reproducers under `docs/archive/review-2026-10/`. The June 2026 report
(`REPORT-rigorous-review-2026-06-09.md`) is the format precedent and
the baseline this review measures itself against.

## Method

Nineteen reading agents (Opus 5.5) in four waves of at most five, under
a binding brief (`briefs/common.md`): a baseline hash gate as the first
action, a private cargo target directory, read only on the repository,
an output schema with severity, confidence and closure per finding, a
hypothesis cap of five with the splitting observation named, and a
"why the stack missed it" paragraph for every P0 and P1. The
coordinator (Fable) held every verdict: every cited line was opened,
every P0 and P1 reproducer rerun from the reader's scratch into a log
committed under `reproducers/`, every capped hypothesis split, and
severity and closure wording rewritten. No agent claim entered this
report unverified.

The order of waves was chosen so that the correctness readers started
from a verified instrument trust table rather than five private
guesses: Wave 1 read the tests, oracles, tooling and documents and
produced the trust table and a contradiction list; Wave 2 read the
transcendental kernel; Wave 3 the format crates and substrates; Wave 4
read for architecture, clarity and the durability of the record. In
one wave no two readers owned a file; across waves a file was read
under different lenses by design. The coordinator re-derived the
budget proofs, the Newton precision table, the adjudicator and the
constant generators before any reader's version existed (Phase 1b),
and ran its own probes during Wave 3 (an independent Algorithm D
simulation; a pin falsification probe that broke one constant per
crate and recorded which test noticed).

Phase 0 first restored the record: push CI on `main` had been red since
2026-07-26 from four causes, all fixed on the branch with the battery
extended to fourteen lanes; six stale agent worktrees (21 GB) and eight
merged branches were removed after their uncommitted deliverables were
captured as the splice artifact. One regression was introduced in
Phase 0 (ladder budget items gated by `cfg` broke the crate's own tests
at default features) and was caught by reader C1 and fixed inside the
arc; the record says so.

Totals: 19 readers, 185 findings (Wave 1 110 across five readers
counting D2's thirty claim rows; Wave 2 31; Wave 3 54; Wave 4 49),
10 P0 classes, 33 P1, 0 refuted outright, one hypothesis refuted
(D2-H1), 3 cross reader duplicates on shared functions, 2 corrections
to the coordinator's own Phase 1b record. The reserve reader was not
needed.

### Roster

| reader | scope | method | blind spot named |
|---|---|---|---|
| C1 | transcendental test surface | reading, oracle probes, a reproducer for the one sided faithful compare | corpus adequacy judged by rows, not by sweep |
| C2 | arithmetic tests, oracles, conformance, Kani | 309,000 case oracle validation against libmpdec, decTest replays | shared GDA readings between oracle and code |
| C5 | tooling, workflows, campaign plumbing, fuzz | ran every CI selection the battery lacked; rebuilt the orphaned fuzz crates | nothing times a test |
| D2 | claims inventory (READMEs, testing.md, KNOWN_ISSUES, CHANGELOGs) | every checkable sentence against the Phase 1 numbers | sizes and benchmarks not rebuilt |
| D3 | ADR coherence 0050 to 0061 plus ten cited earlier ADRs | decision versus code, status propagation | ADR-0001 to 0049 outside its ten |
| A1 | exact and tie classifiers | a forwarding adapter running the `pub(crate)` classifiers against the real rounders; 339,012 direct and 323,000 API cases against an integer oracle | cohort and CLAMPED not compared |
| A2 | adjudicator and algebraic group | 720,000 oracle rows, 51,855 edge calls in six builds, an exhaustive Decimal32 `powi` floor | random operands far from boundaries |
| A3 | ladder, fixed rungs, certification chain | Arb true margins for the S2 corpus, a Decimal32 witness search, 13,593 predicate cases | positive values only in the predicate check |
| A4 | argument reduction, constants, dynamic rung | the 6,408 digit table digit by digit; deep inputs through all three reductions | tan at rung 2 for a deep odd input |
| A5 | per function kernels and the root wrapper layer | 112,014 input five mode differential against mpmath; §9.2.1 tables; a mutation harness | cases within 1e-40 of a boundary; rungs 2 and 3 not forced |
| B1 | root core arithmetic | 1.9 million structured cases against libmpdec and the oracle; 400,000 raw bit fuzz | GDA readings shared with libmpdec |
| B2 | the two siblings side by side | every mirror hunk classified; 150 million structured cases; a 14,331 row special value probe across formats | whole file rewrites are single hunks |
| B3 | the I/O boundary | threat model first; parse, integer, float, render and bits differentials against CPython | spellings GDA rejects and CPython accepts |
| B4 | `ferrodec-decimal` | 270,000 arithmetic and 10,843 transcendental cases; crafted operands for the Ziv cap; context extremes | random operands never hit an exact result |
| B5 | multiword and ieee | 3.6 million fixed width and 664,800 `DecBig` cases against Python integers; every generator at its cap against mpmath | inputs outside each operation's envelope |
| C3 | transcend clarity | `git log -L` on the extended trio; dead code by stripping allows in a copy; an in crate real format test | the generic coefficient trait cost argued, not prototyped |
| C4 | format crates clarity | every allowed clippy lint re enabled and counted; function lengths; `expect` grouping; examples and benches built and run; seven one copy fixes from `git log -S` | 48 of the asked 60 comments sampled |
| D1 | architecture | 102 thumbv6m feature cells, clippy per selection, MSRV per crate, rustdoc JSON API table, three probe crates | sizes with LTO off, single run timings |
| D4 | registry and provenance | all 53 entries audited; 26 captures opened; 140 vendored files byte compared; packaging listed; the generated text traced | venues and versions marked RECALLED where no capture was opened |

## Verdict

The arithmetic the crates were built for holds: about 170 million
structured cases across the three formats and the arbitrary precision
crate agree with libmpdec and the validated exact oracle in value,
cohort and flags, the classifiers and the adjudicator are clean, the
escalation predicate is sound, and every constant generator is exact
at its cap. The review found its defects at the edges the instruments
could not see: saturation gates borrowed from a neighbour, seeds that
round through the destination format, an interchange encoding checked
only against itself, exact results that a Ziv bracket cannot decide,
and a certification chain that measured margins with an instrument
coarser than the quantity. Ten P0 classes are open, all runtime
witnessed with committed reproducers. The published claims that fall
are named below; the ones that stand are named too.

On architecture: the crate family is sound in its posture (every
published crate builds `no_std` for thumbv6m at every feature, the
substrate types are closed, the dependency graph is workspace only
plus two optional, default feature free crates) and weak in its
seams. The transcendental seam (`DecimalFormat`) states preconditions
in prose, carries a dead parameter, and has no exponent range, which
is why the gates are literals per format; the `serde` feature makes
every `no_std` binary need an allocator; the siblings' transcendental
features imply `binary-float` for a reason the manifest itself records
as gone; and the published `ferrodec-transcend` surface is far wider
than its three consumers use, with `Extended`'s invariant bypassable
through public fields. On clarity: the code is pragmatic and mostly
well written where the algorithm is the reason for its length, and
expensive where the same thing exists three times. The extended trio
and the three format crates have each let a fix land in one copy and
not the others, seven times for the formats and once for the trio,
and each such miss produced a defect this review found. On the
record: the registry's vendored bytes, hashes and captures are sound;
its claims about status, coverage and linkage are not, and a clause
misattribution with its source in an ADR title has propagated to 64
sites and into the generated registry text.

The three crate question (ADR-0011), as the coordinator reads the
facts: the mirrors cost 17,906 and 18,758 changed lines against the
root and seven one copy fixes with a P0 among them, every special
value divergence sits in per crate hand written dispatch, and the
behaviours that genuinely differ by width are the storage types, the
array lengths and the exactness driven signatures, which stable Rust
cannot derive from a const `P` but a format trait with associated
types can carry (the seam is already half of it). The dispatch,
parsing, formatting, NaN rules and conversions want one
implementation now whatever the storage story; the arithmetic cores
can stay per width behind that trait. That is a direction, not a
decision; the decision is Parnell's and the ADR-0011 trigger (a
fifth crate) fired long ago (D1-7).

## Confirmed critical findings (runtime witnessed)

Each entry names the witness, the mechanism at the line, the instrument
that should have caught it and why it did not, and the recorded
closure. Beads carry the same text.

### 1. The BID bit layout is not IEEE 754-2019 BID (all three formats)

`src/bid.rs:1-28` and `pack_finite` (:166-176) place the two high
exponent bits at 126..125, the three coefficient MSBs at 124..122 and
a twelve bit exponent continuation at 121..110: the DPD combination
field shape with a binary coefficient. §3.5.2's binary encoding puts
the fourteen bit biased exponent contiguously at 126..113 and the
significand MSBs at 112..110. `1` encodes as `0x2208…01`; IEEE, Intel's
library and GCC's `_Decimal128` use `0x3040…01`; Decimal64 and
Decimal32 (`0x2238…01` and `0x22500001` against `0x31C0…01` and
`0x32800001`) share the transcription. The specials agree; finite
values agree only where the middle exponent bits happen to equal the
coefficient MSBs. Arithmetic, strings and DPD are internally consistent
and correct; `to_bits`, `from_bits`, `serde_bid` and the README's BID
claim are wrong against every other BID implementation (an IEEE `1`
decodes here as `5.19…E-1791`). Why missed: every BID instrument is self
consistent (round trips; the Kani harnesses prove encode and decode
inverse to each other), the only external bit pins are the four
specials, dqEncode is DPD, and June's "codecs clean" used the same
round trip. Closure (Parnell's decision, identity bearing): an ADR,
because persisted `serde_bid` data changes meaning; one layout module
pinned by §3.5.2 literals per format; the regression literal at
`tests/regression_fd_7nf.rs:77` rewritten. Reader B1-1; reproducer
`W3-B1/bits`.

### 2. Public API panics on finite inputs through the Newton seed

`Extended::recip` and `sqrt` (extended.rs:778-830, with twins in
`extended2.rs` and `extended_dyn.rs`) seed Newton's iteration by
rounding the operand through the destination format and asking it for
a reciprocal or square root; when that leaves the range the seed is an
infinity and `from_format` panics. `asinh` and `acosh` for `|x| ≥
1E3073` (Decimal128), `1E193`, `1E50`; `atan2` when `1/x` overflows;
`atan2_pi` on all three formats (`atan2_pi(1E-6141, 1E-6176)` at
Decimal128). Found by the fuzz target the arc revived (no fuzz crate had
built since 2026-05-09) and widened by the coordinator. The same seed
is the mechanism of finding 5 and of the Decimal32 budget premise
(fd-k9t): a subnormal seed keeps five digits and two Newton steps reach
1e-20 where the budget assumes 1e-46; an operand above `MAX` seeds
`1/∞ = 0` and `recip` returns 0 silently. Why missed: the fuzz crates
were orphaned; the property suites' faithful oracle is one sided; S2
never swept Decimal32 or `atan2`. Closure, one for the class: the three
Newton primitives scale the operand to `[1, 10)` (recip) or by an even
power of ten (sqrt) before `to_format`, as `pow::working_reciprocal`
already does, and derive the step count from the format's precision as
`extended_dyn::newton_steps` does. Readers C5-1, C5-2, A5-3, A5-6,
A2-2, A4-6; reproducers `P1B-2`, `W2-A5-gates`.

### 3. `Decimal128::fma` under the directed modes on an exact power of ten product

`fma(10, 1E33, -1E-102)` under TowardZero and TowardNegative returns
`10^34 − 10` where the exact value lies strictly between `10^34 − 1` and
`10^34`, so the 34 nines are correct (libmpdec and a rational check
agree). `src/ops/fma.rs:791-804` drops one digit and `kept.sub(1)`
ignores the binade. 3,366 of 43,830 family cases; Decimal128 only;
directed modes only. Its flag sibling: an exact product of `10^6145`
with a far opposite sign addend routes through the overflow gate and
raises OVERFLOW where `MAX` with INEXACT is correct (B1-2, P1). Why
missed: the property strategy draws uniform coefficients and never an
exact power of ten product at a far gap; Kani is special value only.
Closure: a binade aware decrement at the drop; regression rows for the
family; a property arm at the gate edges. Reader C2-1; reproducer
`P1B-3`.

### 4. `sinh` and `cosh` return infinity for finite results

hyperbolic.rs:172 and :221 saturate on `exp`'s overflow gate, but
`e^x/2` overflows `ln 2` later: for `|x|` in `(14150, 14150.0785]` at
Decimal128, `(887, 887.1884]` at Decimal64, `(224, 224.0439]` at
Decimal32, every mode returns `±∞` (`±MAX` toward zero) for a finite
result; exhaustive at Decimal32, 4,390 of 4,390 cells wrong. Why
missed: S2 swept exactly this band but discards every infinite, zero
or subnormal production result before the mirror runs (A5-7); the
frozen corpora stop below the band; `sinh`/`cosh` have no directed
rows. Closure: a gate at `ln(2·MAX)` per format (the siblings supply
888 and 225 or a trait gate); rows at the gate and at `ln(2·MAX)` in
every mode. Reader A5-1; bead fd-b6zt; reproducer `W2-A5-gates`.

### 5. Decimal32 `atan2` returns wrong values through the subnormal seed

For `x` in `(1E96, MAX32]` the reciprocal seed is subnormal with five
digits and two Newton steps reach about 1e-20 relative, where the
budget prices 1e-46; six witnesses (`atan2(6304256E86, 9523351E90)`
under NearestEven returns `0.00006619787`, Arb gives `…7875000…04`, so
`…788`), fourteen wrong input and mode pairs, identical in the
`unbounded-ladder` build. Why missed: S2 excludes Decimal32 and never
swept `atan2`; the Decimal32 exhaustive program covers unary functions
only. Closure: finding 2's class fix. Reader A3-1; bead fd-h3jk;
reproducer `W2-A3-atan2-d32`.

### 6. `atan2` has no anchor seam for tiny exact ratios

`atan2(1E-6000, 1)` returns `1E-6000` in all five modes at Decimal128
where TowardZero must return the predecessor (as `atan(1E-6000)` does);
Decimal32 `atan2(2E-60, 1)` and Decimal64 `atan2(6E-300, 3)` the same.
inverse_trig.rs:403-418 has no ADR-0051 anchor for a tiny exact ratio,
so the 110 digit value lands exactly on `q` and rung 2 delivers it; the
`unbounded-ladder` build gets the Decimal128 case right, so the two
builds disagree. Closure: the anchor seam (the side theorem `atan(q) <
q`); a `ladder_audit` panic on exact landings. Reader A3 lead; bead
fd-gu75.

### 7. The Decimal128 `exp` underflow gate sits inside the `1E-6176` region

`src/math/format_impl.rs:150` derives the boundary as "≈ −14220.85"
(the `ln 2` added with the wrong sign; true `ln(½·10^-6176)` =
−14221.4587) and :163-165 sets the gate at 14221, so for `x` in
`(−14221.4587, −14221)` the nearest modes return +0 where `1E-6176` is
correct; `exp2`, `exp10`, `pow`, `powr` inherit; the sibling gates are
safe. Why missed: S2 discards zero results (A5-7); the directed test
pins a point past the true boundary. Closure: the constant 14222 and a
gate table with one test against `ln(k·MAX)` and `ln(½·MIN_SUBNORMAL)`
per format. Reader A5-2; bead fd-mq5e.

### 8. `Decimal64::fma` on the overlap path

`fma(3000000000000001, 2500000000000001, -1.00000001)` under
NearestEven returns `7.500000000000006E+30`; the exact value is
`7500000000000005499999999999999.99999999`, so `…005` is correct.
ferrodec-decimal64/src/ops/fma.rs:332-340 aligns the addend to the
working quantum by truncation and ORs its dropped digits into a
positive sticky, so on effective subtraction the residue points the
wrong way and an apparent tie rounds up. Ten hits in 15 million
structured cases; Decimal32's overlap path is unreachable; the root's
buffer path is separate code (lead: the witness generator at 34
digits). Why missed: the property strategy needs the sixteen guard
digits of the difference to sit on a boundary, about once per 1e16
draws; the fd-9fi regression pinned effective addition only. Closure:
subtract the truncated side's ceiling and keep the sticky, as the sub
ULP path already does. Reader B2-1; bead fd-ja4w; reproducer `W3-B2`.

### 9. `ferrodec-decimal` `power` rounds exact and midpoint results wrong

`pow(4, 0.5)` under Down returns `1.99999999` after 2.4 s at the Ziv
cap; `pow(1.5625, 0.5)` at precision 2 under HalfUp returns `1.2` for
the tie `1.25`; `pow(10, 10001)` under Down returns
`9.99999999E+10000` and under HalfEven the right value with a spurious
Inexact; `pow(-1, 100000)` under Ceiling returns `2`. `transc/pow.rs`
has no exactness test for a non integer exponent (:187), declines the
exact integer path when `digits · n > 20000` (:157), and excludes `x =
−1` from the `|x| = 1` shortcut (:113), so `ln_kernel` receives the
value its precondition forbids and a sticky zero rounds `1` up; the
Ziv bracket cannot decide an exact or midpoint value and
`faithful_round` at the cap rounds the low side. The 55 decTest `power`
rows the conformance runner rescues with a one ulp band are all of
this class. Why missed: the `pow_oracle` is the same `power` at a wider
precision; random exponents never give exact results; the band accepts
any flags and cohort. Closure: exactness from the mathematical result
before the general path; the band narrowed to inexact rows. Reader
B4-1, B4-2, B4-3; bead fd-3ses; reproducer `W3-B4`.

### 10. The interop bridge panics on NaN payloads

`to_decimal128`, `to_decimal64` and `to_decimal32` render the arbitrary
precision NaN with its full payload through `Display` and hand the
string to the fixed format's `parse_str`, whose payload limits are 33,
15 and 6 digits; `interop.rs:40/47/54` `expect` a valid string.
`parse_str` and `add` at precision 40 produce such values. Closure:
truncate to the target's capacity or bridge the parts directly. Reader
B4-7; bead fd-29nz.

### Refuted during verification

- D2-H1 (the `tan` asymptote returns `±∞`): the coordinator's
  continued fraction probe shows the reduced fraction is never zero at
  Decimal128 (minimum `1.317·10^-38`), so the branch is unreachable;
  the documentation that says `±∞` stands as a P3.
- No reader finding was refuted outright in any wave. Three line
  citations were corrected (D3-3, D3-4, and B1's "atan2_pi not
  reproduced" table row, which was the coordinator's probe choice);
  two of the coordinator's own Phase 1b statements were corrected by
  readers (the P1B-1 overrun is 1.6× in predicate ulps, not 8×; the
  P1B-2 Decimal128 `atan2_pi` row).

## Confirmed P1 findings (spec divergence or unsound argument, no wrong value witnessed, or a wrong value under a contract the crate chose)

Grouped by the structure that produced them; each id points at the
verification record with the rerun and the closure.

**The Newton seed premise, as an argument.** The ladder's error
itemizations charge 15 units for `recip` and `sqrt` at every format
(ladder.rs `HYPOT`, `POWI_INT`, `RSQRT`), but at Decimal32 rung 1 the
two Newton steps from a seven digit seed reach 8.5e-26 and 4.0e-26
(A2-2, measured); ADR-0060's "smaller formats are strictly easier"
assumes format independent rung error, so its Decimal32 `hypot` floor
is not discharged (A3-6); the dynamic rung carries the same seed
(A4-6); the primitives are `pub` with a prose precondition (A5-6).
`powi` holds after the fact by an exhaustive Decimal32 enumeration two
readers ran independently (minimum distance 1.5e-15 at `c = 1867094`).
One closure with finding 2.

**The cancellation depth premise.** `argred.rs` and `ladder.rs` assume
at most 33 leading zeros in the Payne and Hanek fraction; the true
Decimal128 worst case is 37.88 digits at
`9308532438209917461067659354862169E4639` (coordinator, P1B-1). The
rung 2 trig window term exceeds its budget by 1.6× in predicate ulps
on that input (A4's unit correction of the coordinator's 8×); the
dynamic rung's p = 220 budget is exceeded, 15,130 against 14,000 ulp,
measured (A4-1); the argred tests use a depth 33.55 input and call it
the deepest (A4-2). No wrong value (probability about 10^-180).
Closure: one depth constant per format pinned by a committed probe.

**The S2 certification chain.** The sin, cos and tan Decimal128
margins are distances measured on the rung 1 mirror, whose own error
(about 1e-4 ULP) exceeds the margins it labels; the README's "deepest
certified margin 1.8·10^-11 ULP" input has a true margin of 2.33e-5
ULP (A3-2; coordinator's mpmath agrees); only 55 of 2,816 trig
survivors truly lie inside the threshold. Every S2 output remains an
Arb certified correct value in all five modes; what falls is the near
boundary claim for those three groups and the "hardest 50" ranking.
The campaign certifier never checks flags (A3-5, P2). Closure: rung 2
or Arb margins in the sampler and freezer; the documents restated.

**The Tier 2 figure.** ADR-0059 states the expected number of Tier 1
exceptions over a function's domain as "about 10^-36"; the model gives
`4·B₂·10^-38` per unary domain (4e-34 for sin) and, for `pow`, `powr`,
`atan2`, `atan2_pi`, an expected count of order 1 to 10^7, not small
(A3-3, coordinator's arithmetic agrees; settles D3-13). The per call
restatements are off by 31 to 35 orders in the safe direction.
Closure: an ADR-0059 addendum in one form, propagated.

**Special value dispatch written eleven times.** Root `pow(±0, −∞)`
raises DIV_BY_ZERO where §9.2.1 reserves it for finite negative `y`
(A5-4, all three formats through separate copies); `pow(sNaN, ±0)` is
`1` with INVALID on the root, `1` with no flag on Decimal64 and a
quiet NaN by the strict §7.2 reading (A5-5, C2-3: a reading for
Parnell); sibling `atan2(qNaN, sNaN)` raises no INVALID (C2-2);
sibling `abs_with_status`/`neg_with_status` clear or flip a NaN's
sign against decTest ddabs525/526 and ddmns031/033 (B2-2); sibling
zero rows of `cbrt`, `asin`, `atan`, `sinh`, `tanh`, `asinh`, `atanh`
return a canonical zero where the root keeps the quantum (B2-7), and
the kernels' own zero rows of `powi`, `rootn`, `compound` drop the
§9.2.2 preferred quantum on every format (A2-1); root `sin`, `tan`,
`sin_pi`, `tan_pi` drop it on every format. Every divergence sits in
hand written per crate dispatch; none in the shared kernels' tables.
Closure: one special value dispatcher and one exact pack with a
preferred quantum, used by every row.

**Gates and envelopes.** `fma` raises OVERFLOW on an exact `10^6145`
product with a far opposite sign addend where `MAX` with INEXACT is
correct (B1-2); `scaleb` applies a GDA range envelope under an IEEE
heading on the root while the siblings pass infinity through (B1-3,
B2-5: one rule for three crates, Parnell's choice); `divide_integer`
in `ferrodec-decimal` never checks `emax` (B4-9); `clamp = 1` is
ignored by `quantize`, `reduce` and `divide_integer` there, and
`reduce` reports CLAMPED while returning the unclamped value (B4-10).

**The I/O boundary.** Parsing `"1E-1000001"` costs 2.6 s of CPU on
Decimal64 and Decimal32 because the ADR-0057 sentinel exponent reaches
a per digit drop loop; reachable through `FromStr` and serde (B3-1,
security relevant). The `Engineering` adapter under `{:.N}` pads a
correctly rounded three digit mantissa with zeros that claim precision
it does not have (B3-2). `from_f64` converts the shortest round trip
string, so on the siblings a value whose shortest string exceeds the
precision is rounded twice and under a directed mode on every format
the result can lie on the wrong side; ADR-0056 says the hazard is gone
(B3-4: the contract is the ADR's, its claim is false). The sibling
parsers accept NaN payloads in the non canonical gap the root rejects
since fd-aqs.13 (B3-5). `serde_bid` on Decimal128 cannot read its own
JSON (B3-6). `to_f64` never raises UNDERFLOW on a subnormal binary
result on any crate, and `ferrodec-decimal`'s ignores `rm`, flags
exact results INEXACT and drops the NaN sign (B5-2, B3-3, B4-8).

**`ferrodec-decimal`'s contexts and residuals.** `Context` validates
nothing; at the extremes the type admits (`emin = i32::MIN`,
precision above `2^31`) a dozen `as i32` sites wrap to wrong values
with a clean status, and `fma` at `precision = u32::MAX` panics at
arith.rs:251 and :272 (B4-5, B4-6; the Wave 1 lead). The bounded Ziv's
cap is reachable with a crafted 20,000 digit operand, returns a wrong
rounding indistinguishable from a decided result, and ADR-0040's three
claims about it are false (B4-4). `pow(2, 1E+2147483647)`, twelve
characters, peaks at 1.9 GB resident before overflowing (B4-11, the
ADR-0053 class). `inv_pi_digits` panics inside its documented cap
(B5-1).

**Instruments and the record.** The faithful oracle's compare is one
sided, so every `assert_faithful` consumer accepts any result below the
truth (C1-1; an astro-float `abs_cmp` defect worth reporting upstream).
ADR-0033's backstop argument proves faithful rounding only and is
still presented as load bearing after S1 refuted it (D3-1).

## Verification infrastructure findings

What the stacks could not see, by instrument. The instrument trust
tables live in `readers/C1.md` and `readers/C2.md`.

- **One sided oracle (C1-1).** `cmp_approx` compares with sign; a
  TowardPositive one ulp low mutant of `sinh` passes every property,
  frozen vector and directed suite on all formats; only the anchor
  bands and the root S2/planted suites turn red (A5-8). Eleven
  functions have no directed rows below Decimal128 (C1-5).
- **Campaign plumbing.** S2 discards every infinite, zero or subnormal
  production result before the mirror (A5-7), hid findings 4 and 7;
  measured trig margins on the rung 1 mirror (A3-2); the certifier
  checks values, never flags (A3-5); the `.prov`, `.json`, s1 `.tsv`
  and `margin_lockstep.txt` files are unhashed (C1-19, C2-9, C5-19).
- **Conformance runners.** Ten Decimal64 files are never dispatched
  (1,167 vectors, among them ddAbs and ddMinus: B2-2) and the d64
  NaN comparison is lenient (C2-5 to C2-7: B2-3 hides behind it); the
  root skips 99 Conversion_syntax rows on a false premise (C2-8);
  the `ferrodec-decimal` `power` band accepts any flags and cohort and
  every row it rescues is finding 9 (B4-12); `rescale` rows fall
  through a catch all `Skip` arm. Decimal32 has no arithmetic decTest,
  so the coordinator's wrong quantize table entry passed all 53 suites.
- **Property strategies.** Uniform coefficients never draw the exact
  power of ten product (finding 3), the sixteen guard digit boundary
  (finding 8), an exact `power` result (finding 9), or a zero (the
  root suites exclude zeros; A2-1, B2-7); the `pow_oracle` is the same
  `power` at a wider precision; `property_to_f64` uses the routine it
  tests.
- **Self consistent checks.** BID round trips and the Kani encode and
  decode inverse proved the crate agrees with itself (finding 1); the
  Kani special table harnesses pin live behaviour including C2-2,
  C2-3, B2-2, B2-3, B2-7 and put every zero at exponent 0.
- **Pins.** Budget values unpinned (C1); `TEN` unpinned (B1-6); no
  `inv_pi` mpmath pin, `KARATSUBA_THRESHOLD` and `GUARD` unread
  (B5-8); the π family certificate asserts at the wrong unit (A4-3);
  tests that compare a copy of a constant with itself (C1-12);
  `argred` tests at depth 33.55 (A4-2); the polish test fed 56 digits
  where 50 exist (A2-2); the `ln` kernel error bound is an untested
  premise of the Ziv bracket (coordinator probe: `LN_GUARD = 0` passes
  every suite).
- **Selection.** `ci.yml` runs on push and PR to `main` only, so a
  branch gets no CI without a PR (C5-5); `force_rung3` and `telemetry`
  run in no workflow; the multiword tests run none of `DecBig`'s
  without `--all-features`; no lane builds a single feature under `-D
  warnings` (A1-1: 46 dead items under `trig-pi`); `thumbv8m` is in the
  toolchain file and never built; the fuzz crates had not built since
  2026-05-09 (C5-3) and found two P0 rows in under fifteen seconds once
  they did.
- **Shared target directories.** A `git archive` copy of the workspace
  built into the same target directory reuses unit hashes and ran copy
  code under the repo's name once (A2); the brief now forbids it.

## Design and documentation tier

Engineering (P2) and documentation (P3) findings verified by reading,
grouped by the structure that produced them. The per finding rows with
closures live in the four verification records.

**Three copies of one thing.** The extended trio (27 functions in
three files, four behavioural divergences, two carrying an open defect,
one doc fix landed in one copy; C3-1) and the three format crates
(seven fixes reconstructed from `git log -S` that landed in one or two
copies, each with the defect it produced: the NaN payload bound, the
full drop fast path, the generic `pow_special_cases`, the `PRECISION`
derivation, the H9 CLAMPED fix, the directed residue helper, the sNaN
priority rule; C4's receipts). Eleven hand written special value
dispatchers (root 8, d64 30, d32 30 named functions) diverge where the
shared kernels do not (B2, C4-15); two `propagate_nan2` with two rules
in one crate (C4-7); six parsers of one grammar (C4 note 2); `PRECISION`
defined twice per crate and `U128_DIGIT_CAP` eight times. This is the
evidence base for the ADR-0011 question (one generic crate or three),
which is a decision for Parnell; the coordinator's synthesis is in the
Verdict.

**A prose fact where a type could stand.** `from_format`'s `expect`
("NaN / Inf are dispatched at the public-API boundary") byte identical
in three copies and false by fd-0o9 (C3-8); the Newton primitives'
"caller must ensure" (A5-6); `Class::Finite` carrying raw integers
beside the newtypes that would prove them, so 42 `expect("… from
classify_bits")` re validate (C4-4); 39 `expect("finite")` after per
operation dispatch (C4-5); `Context` with public fields that no
constructor can validate (C4-14, a major version decision); `Work`'s
"only fold site" invariant with three other fold sites (C4-11);
`mantissa_bits: u32` where an enum of two formats fits (B5-5); budget
units as bare `u128` (A3). The remediation's type level items come from
this list.

**Literals that should be one constant.** The cancellation depth `33`
names two quantities in `argred.rs`, one false (the depth, folded into
`FRAC_DIGITS = 76` and `FRAC2_DIGITS = 143`) and one true (the
coefficient overlap), so a naive replacement would corrupt the window
offsets (C3-2); the Newton steps `2`/`3` and the 15 unit charge; two
exponent window constants for one role and four `2^α 5^β` assemblies
that already cost one shipped defect (C3-5); `exp`'s gate borrowed by
`sinh`, `cosh` and the `expm1` family (A5-1, C3's literal table); d32
`k > 6` and `16 − d` where d64 derives (B2-10); `MAX_PARSED_DIGITS` per
crate as a literal.

**Dead weight and stale wiring.** Three file wide `#![allow(dead_code)]`
hide six dead production items in the shipped build (C3-4); ten
"reached at M8" allows suppress nothing since M8 landed (C3-3); `exact.rs`
(3,800 lines) compiles under `trig-pi` for one 20 line helper (A1-1, C3
note 2); `TYPE_MASK`, `EC_*`, `COEFFICIENT_FIELD_LIMIT` dead in all three
`bid.rs` under "consumed by arithmetic modules" (C4-16); three dead
parameters in `fma_sub_ulp` behind four `let _` lines and a rustdoc
bullet describing a path retired in May (C4-6); 230 plan milestone
labels in rustdoc defined only in a plan file (C3-12).

**The lint policy.** Of the 24 workspace allowed pedantic lints, five
hide debt (the cast lints over about 820 library sites whose bounds a
`Context` does not validate; `missing_panics_doc`, which would have
flagged the P0 interop panics and the `fma` panic; `too_many_lines`;
`must_use_candidate` for multiword), four silence nothing anywhere
(`float_cmp`, `single_match_else`, `assertions_on_constants`,
`trivially_copy_pass_by_ref`), and 21 local allows duplicate workspace
ones; the rest earn their place (C4-1 to C4-3). Thirteen functions
exceed 120 lines; five are long because the algorithm is; the rest mix
decode, alignment, rounding and fix narrative (C4's table).

**Documents.** The claims inventory (D2-1 to D2-30: README overview,
verification list, `docs/testing.md` Part I, the sibling KNOWN_ISSUES
and CHANGELOG numbers, the registry) and the ADR coherence findings
(D3-2 to D3-17: the index stopping at 0054, eleven one sided
supersede marks, thirteen ADRs citing plans outside the tree, the `%`
story, the "exhaustively verified" anchors without ADR-0050's scope
note) are the documentation tier's backbone. On top of them every
wave listed rustdoc that states a number or a state the code does not
compute: 28 paragraphs in `ferrodec-transcend` (21 marked by Wave 2,
7 by C3), the Newton rustdoc false differently in each copy (C3-7),
the `sticks_to` separation argument drifted per copy and citing the
refuted margin (C3-6), the sibling wrapper docs still saying
"faithful" (D2, B2-9), the gate comments with the wrong sign (A5-10),
the binary conversion docs (B3-13), 25 comments attributing CLAMPED to
IEEE §7.4 (B5-7, C4), the `bigconst` doc merge (B5-6), the root shim
test that documents a guard it does not perform (C3-9), the example
that demonstrates the width defect (C4-12). Two ADRs invalidated each
other's premises without citation (ADR-0030's "≤ 398" and ADR-0057's
sentinel: C4-8), and ADR-0057's "byte identical parser" is false by 87
changed lines.

**Architecture (D1).** The feature matrix: every narrow selection
builds, and `exp-log`, `hyperbolic`, `trig-pi` and their `trig` pairs
fail `-D warnings` on all four crates through transcend dead code
(D1-2); `serde` requires an allocator in `no_std` binaries against the
`no-std::no-alloc` category (D1-1); the siblings' implied `binary-float`
(D1-3); the seam's prose preconditions, dead `status` parameter and
missing exponent range (D1-4); the transcend public surface and
`Extended`'s public fields (D1-5); the MSRV holds for libraries and
not for the dev graph (D1-6); ADR-0011's trigger fired (D1-7);
`PRECISION` twice per crate (D1-8); `telemetry` reading zero on
targets without 64 bit atomics (D1-9). The invariant table (23 rows)
and the API consistency table are in the Wave 4 record; 33 Decimal32
tests run in no lane.

**The record (D4).** The §7.4 CLAMPED attribution is a class of 64
sites in 46 files with its source in ADR-0048's title (D4-1); the C23
clause numbers in the registry are wrong against the archived draft
(D4-2); two entry pairs register one source each with contradicting
verification status (D4-3); the failure museum says "Open" for June's
fixed defects and lacks fd-clc (D4-4); the decTest coverage gaps omit
the BID bit blind spot behind finding 1 and the absence of any
Decimal32 arithmetic vectors (D4-5); six published crates package no
licence text and the packages redistribute the "All rights reserved"
decTest files and the docs archive (D4-6, Parnell's decision); the
verification map lacks every ADR-0059 instrument (D4-7); linkage is
checked one way and padded (D4-8, D4-9); the GDA specification is
archived as a title page (D4-10); the Cowlishaw DPD summary, the Intel
BID library and the oracle versions have no entries (D4-11, drafted);
the disclosure's named failure mode does not name the two classes the
review's disciplines structurally miss (D4-12, diff proposal only).

## Found clean

Adopted with the methods and blind spots named in the records.

- The exact integer oracle itself on 309,000 random and 2,265
  constructed cases against libmpdec (C2).
- Decimal128 add, sub, mul, div, sqrt, divide_integer, rem, rem_near,
  quantize, logb, next*, integral, reduce, the comparisons, rotate,
  shift, logical and NaN propagation on about 1.9 million structured
  cases, zero results, the clamp edge family, 400,000 raw bit fuzz
  cases without a panic (B1); the siblings' add, sub, mul, div, sqrt,
  rem_near on 15 million structured cases each per format, the C2-1
  shape at both widths, quantize and scaleb inside the envelope,
  every `POW10` table sized for its index, constants pinned by value
  (B2).
- Finite parsing, toSci and toEng, formatter totality on 90,000 raw
  patterns, integer conversions per §5.8, the DPD codec on all 1,024
  declets, `to_f64` values, `Neg` on NaN (B3).
- The exact and tie classifiers on 339,012 direct and 323,000 API
  cases, the splice check of the merged `exact.rs` (A1); the
  adjudicator's five deciders, `round_adjudicated`, the algebraic
  group's values on 720,000 rows and 51,855 edge calls in six builds
  (A2); the escalation predicate on 13,593 cases, the three margin
  definitions at normal width, the certifier's value path, the
  `extended` versus `extended2` mirror (A3); the 6,408 digit `2/π`
  table, the window arithmetic in all three readers, `reduce_wide`
  equals `reduce_dyn`, `DynArena` discipline (A4); the §9.2.1 tables
  apart from `pow`, every anchor threshold, negate after round, the
  `pow` fast path, the root rustdoc theorems, 112,014 inputs in five
  modes against mpmath (A5).
- `ferrodec-decimal` arithmetic on 270,000 structured cases over nine
  precisions and eight modes, the transcendentals on 10,843 random
  cases, the ADR-0053 guards on every alignment path, the rounding
  core, exact detection in `exp`, `ln`, `log10` (B4).
- The five fixed width types on 3.6 million cases, `DecBig` on 664,800
  plus 221,398 Algorithm D divisions with the add back forced, every
  constant generator exact at its cap, `should_round_up`,
  `for_negation`, the `Status` algebra, both substrate crates on
  thumbv6m (B5).
- The coordinator's own: the Algorithm D add back witness fires and is
  correct; `MIN_POSITIVE_NORMAL` is pinned by value; `for_negation`
  is pinned by twenty root tests.

## Why the stacks missed the confirmed bugs

Three shapes account for all ten P0 classes. A constant or gate
borrowed from a neighbour and derived once in prose (findings 4, 7;
the certificate's unit, the depth literal): no test read the value
against its derivation. A premise stated as a comment where a type
could have carried it (findings 2, 5, 6, 8, 10: a seed "the algebra
keeps in range", a residue that "cannot pull magnitude back", an
anchor seam present on `atan` and absent on `atan2`, a string "that
is always valid"): the first reader to measure the premise found it
false. An instrument that agreed with itself (findings 1, 3, 9: BID
round trips, a property strategy that never draws the structured
case, an oracle built from the function under test): clean results
spoke only to what the checker could see. The June review named the
third shape and missed finding 1 by it.

## Remediation program

To be filed as a beads epic at review close, in this order (small and
shippable first, then premise repairs, then instruments, then
documents), one child per confirmed finding and per P1 to P3 finding
that carries a closure, with `discovered-from` links to the review
epic:

1. **Constants and gates.** The Decimal128 `exp` underflow gate
   (finding 7); the `sinh`/`cosh` gate per format (finding 4); the
   Decimal32 `x / ∞` CLAMPED constant (B2-6); the d32 literals derived
   from `PRECISION` (B2-10); `TEN` and the other unpinned constants
   pinned by value (B1-6, B5-8); a gate table with one test against
   `ln(k·MAX)` and `ln(½·MIN_SUBNORMAL)` per format.
2. **Premise repairs, one per class.** The Newton primitives own their
   range and step count (findings 2 and 5; fd-k9t; A2-2, A3-6, A4-6,
   A5-6); one depth constant per format (P1B-1, A4-1, A4-2, A3-2's
   tooling); the `atan2` anchor seam (finding 6); the fma binade class
   on the root drop, the root overflow gate and the Decimal64 overlap
   path (findings 3 and 8, B1-2); `power` exactness from the result,
   a validated `Context`, the interop bridge, `divide_integer` and
   `clamp = 1` in `ferrodec-decimal` (finding 9, 10, B4-4 to B4-11);
   the sibling parse drop loop (B3-1); `from_f64` and `to_f64`
   contracts (B3-3, B3-4, B4-8, B5-2); `inv_pi_digits` through the
   unchecked core (B5-1).
3. **The BID layout** (finding 1) as its own ADR and its own release
   decision: the fix changes the meaning of persisted `serde_bid`
   bytes, so it is a breaking change for that adapter however the
   crate versions are cut; Parnell decides the migration story before
   any code moves.
4. **Dispatch and cohorts.** One special value dispatcher and one
   exact pack with a preferred quantum for the three formats and the
   kernels (A2-1, A5-4, A5-5, C2-2, C2-3, B2-2, B2-3, B2-4, B2-5, B2-7;
   the `pow(sNaN, ±0)` and scaleb readings decided first).
5. **Instruments.** A two sided faithful compare and negative controls
   (C1-1); directed rows for the eleven NE only functions (C1-5); S2
   certifying non normal outputs, rung 2 margins, flag checks (A5-7,
   A3-2, A3-5); the ten d64 files dispatched under a strict NaN
   compare (C2-5 to C2-7); a Decimal32 arithmetic oracle suite and a
   value pin per table (C2-10, coordinator probe); the `power` band
   narrowed (B4-12); the fuzz crates in the workspace with timeouts
   (C5-3); per feature clippy, multiword `--all-features`,
   `force_rung3` in a workflow, the draft PR trigger (A1-1, C5); corpus
   hashes (C1-19); the kernel error bounds tested at `wp` (coordinator
   probe); the three format special value probe checked in as a test
   (B2 design note 4).
6. **Structure (Wave 4).** One generic Newton body over `ExtNum`,
   the named depth and overlap constants, one `2^α 5^β` assembly and
   one window constant, item level dead code gates, `exact.rs` split
   along its seams (C3); `Class::Finite` on the newtypes, a `kind()`
   view, one `propagate_nan2` per crate, the sibling `pow` table
   deleted, `PRECISION` once (C4, D1-8); `serde` without `alloc` and a
   lane that links a `no_std` binary (D1-1); the semver bearing items
   held for the next major (the sibling `binary-float` implication,
   private `Context` fields, a sealed seam carrying `E_MAX`/`E_MIN`,
   private `Extended` fields, namespaced test cfgs: D1-3, D1-4, D1-5,
   C4-14); the lanes (narrow clippy, MSRV per library, `cargo package
   --list` with licence text, thumbv8m, the 33 Decimal32 tests: D1-2,
   D1-6, D4-6).
7. **Documents and ADRs.** Addenda to ADR-0032, 0033, 0040, 0051, 0056,
   0058, 0059 (Tier 2 and margins), 0060 (Decimal32 note), 0027/0029
   (`%`), 0055 (`to_f64` INEXACT); the README's BID, certified margin
   and CI sentences (the disclosure items as a diff proposal); the D2
   claims inventory; the ADR index past 0054; the rustdoc sites every
   wave listed; the registry: ADR-0048's title and the 64 §7.4 sites,
   the duplicate entry pairs, the failure museum, the coverage gaps,
   the verification map rows, the GDA PDF capture, the guard checking
   both directions, and the entries this arc adds (D4-1 to D4-11).

## Version cut readiness memo

Inputs gathered at `ba92f29`: crate versions ferrodec 4.1.0,
decimal64 4.1.0, decimal32 4.1.0, transcend 0.3.0 (untagged),
decimal 3.0.1, ieee 0.1.5, multiword 0.2.0 (untagged), campaign 0.1.0,
test-support 0.1.0; Unreleased sections of 175, 135, 115, 140, 13, 1
and 0 lines with no breaking, removed or renamed entry in any; tags
newest `ferrodec-decimal-v3.0.1`, the format tags at 4.1.0 (root as
`v4.1.0`, the naming change D3-14 records). The standing proposal
(4.2.0 lockstep for the formats, transcend 0.4.0, multiword 0.3.0, ieee
unchanged) is consistent with the changelogs as written. Against it:
ten P0 classes are open, two of them (findings 1 and 8) in code the
formats ship today under NearestEven, and finding 1's fix is a
breaking change for `serde_bid` consumers whatever the version number
says. The coordinator's recommendation is to cut after remediation
items 1 to 4 land, with the BID ADR deciding whether the format crates
go to 5.0.0 or carry the layout fix as a bug fix with a documented
migration; publishing remains Parnell's. `ferrodec-decimal`'s empty
Unreleased section will not stay empty. Before any publish, the six
crates that package no licence text (D4-6) need `license-file` or
LICENSE symlinks and `include` lists, and the decTest redistribution
through crates.io needs Parnell's decision.
