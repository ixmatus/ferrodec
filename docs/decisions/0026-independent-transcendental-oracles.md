# ADR-0026: Independent transcendental oracles (Arb frozen vectors, MPFR gate, mpmath differential)

- **Status**: accepted
- **Date**: 2026-05-17

## Context

ADR-0024 placed the entire transcendental surface of all three decimal
crates on one shared `ferrodec-transcend` Extended kernel under the faithful
contract of ADR-0021. ADR-0025 added metamorphic identities as the backstop
in the regions the fixed astro-float oracle cannot reach. Both ADRs are
correct as far as they go and neither closes the gap this ADR exists to
close.

### The correlated-failure surface

The shared kernel is convivial and frugal precisely because it does not
reimplement every function. It derives most of them from a small set of
primitives. The kernel recon over `ferrodec-transcend/src/` establishes the
exact graph:

- **Primitives**, each computed directly by argument reduction plus a series
  (no other kernel function in the data path): `exp` (`exp.rs`, reduce by
  `ln 10`, Taylor), `ln` (`ln.rs`, decade plus halve/double, Taylor),
  `sin`/`cos` (`sincos.rs`, Payne-Hanek reduction in `argred.rs`, Taylor),
  `atan` (`inverse_trig.rs`, inversion plus `pi/4` shift, Taylor).
- **Derived**, computed as a composition of primitives: `exp2` is
  `exp(x·ln 2)`; `log2` and `log10` are `ln(x)·const`; `cbrt` is
  `exp(ln|x|/3)`; `pow` is `exp(y·ln|x|)`; `sinh`/`cosh` are `(e^x±e^-x)/2`;
  `tanh` is `sinh/cosh`; `asinh`/`acosh`/`atanh` are their `ln` forms; `tan`
  is `sin/cos`; `asin`/`acos`/`atan2` route through `atan`.

This topology has a property that governs everything below. A defect in a
primitive does not stay local. It propagates coherently into every function
derived from that primitive, with the same sign and a related magnitude,
because the derivative literally calls the defective primitive. `exp` and
`ln` are the worst case: eleven derived functions route through one or both
of them, so a single systematic bias in the Extended `exp` or `ln` core
would bias `exp2`, `log2`, `log10`, `cbrt`, `pow`, `sinh`, `cosh`, `tanh`,
`asinh`, `acosh`, and `atanh` in lockstep.

The consequence for testing is the load-bearing point. Any check whose two
sides both flow through the same primitive cannot fail when that primitive
is wrong, because the error enters both sides and cancels. ADR-0025 already
hit this and pruned the tautological identities (`log_b·ln b ≈ ln`,
`tanh ≈ sinh/cosh`, `exp2 == pow(2,x)`, the `asinh`/`atanh` ln-forms): each
compared the kernel against itself. The deeper instance is the oracle
itself. The astro-float faithful oracle is an independent computation, but
it is one fixed implementation at 256 bits, and ADR-0025 records that it is
unsound past a bounded argument magnitude (the `coef.ilog10()+exp > 15`
guard in `property_sincos.rs`; the magnitude-scaled precision in
`property_sincos_large.rs`; fd-3cd, fd-dfs). In the skipped decades the only
remaining backstop is the metamorphic suite, whose bounds ADR-0025 itself
states are not correctly-rounded-tight, so a sub-condition-number systematic
bias in the shared `exp`/`ln` core can still hide there. That residual is
the frontier this ADR addresses, and it cannot be addressed by any check
that shares the kernel's structure or by a single oracle of one fixed
precision. The only mitigation is verification against implementations that
share neither.

### Acceptance criterion

From the above, one criterion governs every artifact added under this ADR
and is treated as first-class, not advisory. Every check must be
structurally independent of the primitive it is meant to exercise: it must
route through neither the Extended kernel nor astro-float. A check that
fails this test does not count as independent evidence no matter how many
arguments it sweeps.

## Decision

Add three dev-only, provenance-tracked oracle roles. None is a default
build dependency, none enters the no_std library build, and the pure-Rust
default posture (`feedback_oracle_choice`, the README provenance section)
is preserved unchanged for everything a published artifact links. The three
roles exist as separate tiers because they answer different questions at
different rigor, and battle-testedness was the deciding axis: one oracle
cannot serve all three.

### Rigor tiers (load-bearing rationale, kept verbatim from the ratified strategy)

- **Arb/FLINT is a proof.** Arb computes certified ball enclosures. When
  the enclosure of the true value does not straddle the decimal half-ULP
  boundary, the correctly-rounded result is *established*, not sampled: the
  true value provably lies on one side, so exactly one representable value
  is correct and the kernel either equals it or is wrong. Arb is heavily
  battle-tested (SageMath, computational number theory, industrial CAS;
  Fredrik Johansson). It is the **primary frozen-vector generator**.
- **MPFR is independent gold-standard corroboration.** MPFR is the
  industrial gold standard for correctly-rounded arbitrary precision and
  exposes a ternary flag giving the exact sign of the rounding error, which
  is the precise instrument for probing faithful versus correctly rounded.
  Two independent gold references agreeing is the strongest acceptance
  criterion available short of a discharged proof, so the vector-accept
  rule is **Arb enclosure decisive AND MPFR agrees**.
- **mpmath is breadth, not proof.** mpmath is structurally independent of
  both the kernel and astro-float, so it still breaks the correlation, and
  it covers the whole special-function surface cheaply. Its adaptive
  precision is not certified, so it corroborates widely but proves nothing.
  It is characterised honestly as the least rigorous of the three.

### Role 1: Arb frozen vector corpus (proof tier, default-on)

An offline generator (`tools/gen_transcend_vectors.py`, `python-flint`,
deliberately not a workspace member and not a Cargo dependency) produces,
per transcendental and per format width, a corpus that includes a decimal
Table-Maker's-Dilemma worst-case search: arguments whose true value sits
pathologically near a decimal half-ULP. For each vector the generator
raises Arb working precision until the enclosure is decisive and records a
per-vector provenance note (final precision, ball radius, half-ULP margin,
"decisive"). The corpus is checked into `tests/vectors/transcend/` and read
by a default-on Rust test that asserts the kernel result equals the
checked-in correctly-rounded value. This test has no C-FFI and no oracle in
its data path; it is pure committed data and is the most durable artifact
of the engagement.

### Role 2: MPFR correctly-rounded gate (corroboration tier, gated)

`rug` (MPFR) becomes an optional dev-dependency behind a new `mpfr-gate`
feature, gated exactly as the existing `differential` feature is: never in
the default build, never in standard CI, never in the no_std library build,
local opt-in only. It independently recomputes every frozen vector,
completing the Arb-decisive-and-MPFR-agrees accept rule, and uses the
ternary flag to report the faithful-versus-correctly-rounded distribution
per function as evidence for the honest-level statement below. Wiring this
gate into a nightly CI lane is an explicit deferred follow-up, the same
status the testing-surface extension gave the differential harness; it is
not built here.

### Role 3: mpmath special-function differential (breadth tier, gated)

The existing Track-3 Python subprocess (`tools/diff_oracle.py`,
`ferrodec-test-support/src/differential.rs`, the `differential` feature)
extends to the special-function surface `decimal` lacks. mpmath computes at
high working precision and the decimal rounding is performed on our side,
so the harness, not the oracle, owns the rounding. Sweeps deliberately
reach the decades the fixed astro-float oracle skips. The work is ordered
`exp`/`ln` first, then `sin`/`cos`, then `atan` and the remaining derived
functions, matching the blast-radius order above.

### The earlier rug/MPFR rejection does not transfer

`feedback_oracle_choice` and ADR-0025 record a rejection of `rug`/MPFR.
That rejection was scoped to the *arithmetic* differential, where the
failure mode is specific: a binary-radix reference reached through a
decimal-to-binary-to-decimal conversion mishandles the decimal cohort and
the spec's preferred-exponent rules, so the reference disagrees with
correct decimal arithmetic for reasons that are not bugs in ferrodec. None
of that applies to a transcendental. A transcendental's true value is a
single real number, radix-independent; MPFR computes that real value to
high binary precision with a known error sign, and *we* perform the decimal
rounding ourselves from a sufficiently precise representation, exactly as
the mpmath role does. The conversion hazard that motivated the arithmetic
rejection is therefore absent here. The rejection stands for arithmetic and
is explicitly not extended to transcendentals.

### Provenance and licensing

Arb, FLINT, and MPFR are LGPL. The standing code-provenance rules and the
pure-Rust posture are nonetheless preserved, for two distinct reasons:

- **Arb/FLINT is a build tool, and its output is mathematical fact.** The
  generator runs offline and is never a Cargo dependency; Arb and FLINT do
  not link into any artifact, dev or shipped, and are not in the dependency
  graph a consumer resolves. Their output is the correctly-rounded value of
  a transcendental at a chosen argument, which is a mathematical fact, not
  copyrightable expression: any correct arbitrary-precision library yields
  the identical digits, which is exactly why MPFR can cross-validate them.
  The checked-in corpus is data with recorded provenance, the same standing
  the decTest vectors already have. This is the compiler analogy: a
  GPL-family build tool that emits facts does not impose its license on the
  facts or on a project that commits them.
- **`rug`/MPFR is dev-only and gated, the posture already accepted for
  astro-float.** It is an optional dev-dependency behind a feature that is
  off by default, absent from CI, and absent from the no_std build. No
  published crate and no embedded target ever links it. This is the exact
  containment already granted to `astro-float` and `num-bigint` for the
  ADR-0021 oracle; the only new fact is the C-FFI build tax on a developer
  who opts into `mpfr-gate` locally, weighed against the ternary flag,
  which is the only available instrument that distinguishes faithful from
  correctly rounded and is the whole reason MPFR is in the strategy.

The README "How ferrodec is developed" disclosure is unaffected and is not
edited; its named failure mode (rounding errors on boundary cases the test
suite did not cover) remains accurate and is, in fact, exactly what this
engagement narrows.

### Honest correctness level and residual frontier

What this engagement supports, stated so it is not overclaimed and so the
limit is recorded for the future maintainer:

- The result is **strongly-corroborated faithful rounding plus a frozen
  worst-case vector set with established correctly-rounded values at the
  specific committed arguments**. At each frozen vector whose Arb enclosure
  is decisive and which MPFR confirms, the correctly-rounded value is
  established and the kernel is checked against it exactly.
- It is **not a coverage proof**. The frozen vectors are decisive at the
  arguments chosen, including a worst-case search, but a finite committed
  corpus does not prove correct rounding over the continuum.
- It is **not proven correct rounding** of the functions. That requires
  proven hardest-case bounds per transcendental per decimal width, which do
  not exist in the literature; deriving them is a research programme
  (ADR-0021, ADR-0024), and a claim the project cannot discharge is worse
  than a weaker claim it can.
- The residual frontier is therefore narrowed, not eliminated: a systematic
  sub-condition-number bias in the shared Extended `exp`/`ln` core, in
  argument decades that lie outside every frozen vector and outside the
  astro-float sound domain, is corroborated against by three structurally
  independent oracles but is not provably excluded. Naming it concretely:
  the exposure is a rounding bias on `exp`/`ln` boundary arguments that the
  committed corpus did not happen to include and that the differential
  sweeps did not happen to land on.

## Consequences

- The correlated-failure surface gains genuinely independent scrutiny,
  prioritised on the primitives with the widest blast radius. Where an Arb
  enclosure is decisive and MPFR agrees, the evidence rises from
  "corroborated faithful" to "established correctly rounded at this
  argument", which is strictly stronger than anything ADR-0021 through
  ADR-0025 provided in the oracle skip regions.
- The strongest artifact, the frozen corpus, costs nothing at consumer or
  CI time: it is committed data checked by a default-on test with no
  external dependency in its path. The C-FFI tax is confined to two
  opt-in, local-only developer features.
- The honest-level statement is deliberately weaker than "correctly
  rounded". This is the anti-overclaiming discipline ADR-0021 and ADR-0024
  established; recording the residual frontier in this ADR is what lets a
  future maintainer extend the corpus toward it rather than rediscover that
  it exists.
- A new licensing surface (LGPL dev tooling) is introduced and is argued
  through here rather than left implicit, so the reasoning is auditable and
  not relitigated: facts emitted by an offline tool are not encumbered, and
  a gated dev-only dependency is the posture already accepted for the
  pure-Rust oracle.
- This ADR carries the durable rationale that the content-dependent docs
  (`fd-au6`, the correlated-failure explanation; `fd-xpb`, the
  testing-surface overview) will cite. Those docs are written after the
  engagement lands, reflecting what shipped, not this hypothesis.

## Addendum (fd-97a, 2026-05-18): directed modes and binary pow/atan2

The frozen corpus shipped `NearestEven`-only and unary-only. Two gaps were
filed and closed. `fd-tgg` first made the decimal rounding step a
lockstep-tested shared unit: the round-half-even keystone moved into
`ferrodec-test-support` `round_dec` and is exercised, against a committed
case table, by both a default-on Rust test and a no-pip Python self-test in
the generator, with the two fd-cb6 near-misses (the all-nines carry exponent
and the `decimal32` input non-representability) pinned as named regression
guards. `fd-97a` then extended the proof tier on that now-trusted
foundation.

The corpus line schema is now uniform and mode-tagged:
`<prec> <mode> <input> [<input2>] <output>`, the loader infers arity from
the file stem. The directed worst case sits at a different place from the
half-ULP tie: a directed decision flips where the enclosure straddles a
representable grid point, not the midpoint, so the directed
Table-Maker's-Dilemma search targets the grid-point boundary
(`directed_margin`), recorded in the provenance as `boundary=` rather than
`margin=`. Directed coverage is bounded to the kernel primitives plus the
widest-blast-radius derived functions (`exp`, `ln`, `sin`, `cos`, `atan`,
`cbrt`, `log10`); the correlated-failure argument says a primitive bias
propagates, so the directed boundary matters most there, and the remaining
unary functions stay `NearestEven` in this tier while keeping their
metamorphic and differential coverage. `pow` and `atan2` enter the proof
tier as binary vectors over `NearestEven` and the four directed modes, found
by a two-dimensional argument search; `pow`'s hard-to-round structure lives
in `y·ln x`, so each is one certified Arb call and the working precision is
raised by both operands' magnitudes. Determinism is preserved by three
independent rng streams seeded from the one `SEED`, so the `NearestEven`
content stays byte-stable under the added token and a regeneration is
byte-identical. MPFR cross-validation applies the directed decimal rounding
on our side, so it does not depend on the binding exposing every MPFR mode,
and the no-double-rounding contract stays with us.

The honest-level statement is unchanged and was written to survive this:
strongly corroborated faithful rounding plus a frozen worst-case set whose
correctly-rounded values are established at the committed arguments, now
across the directed and binary surface as well. It remains not a coverage
proof and not proven correct rounding of the functions. The empirical
result is recorded in the test output and commits, not frozen into this
prose (state versus durable capture): every committed vector across all
formats, modes, and the binary surface was computed exactly correctly
rounded by the faithful kernel, and MPFR independently reproduced the whole
extended corpus with zero disagreements, so the
Arb-decisive-and-MPFR-agrees accept rule holds for the directed and binary
vectors as it did for the original NearestEven set.

## Related

- Plan: `plans/2026-05-17-independent-transcendental-correctness.md`
- Tracking: bead `fd-cb6` (epic); `fd-syf`, `fd-clf`, `fd-x3u`, `fd-i4e`,
  `fd-12v` (phases). Blocks the content-dependent docs `fd-au6`, `fd-xpb`.
- Other ADRs: builds on ADR-0021 (faithful contract, exact oracle),
  ADR-0024 (shared Extended kernel), and ADR-0025 (metamorphic backstop,
  tautology pruning). Supersedes none. The rug/MPFR rejection scoped to the
  arithmetic differential (ADR-0025, `feedback_oracle_choice`) is narrowed,
  not overturned: it stands for arithmetic and does not extend to
  transcendentals.
