> **Archived 2026-05-22.**
>
> This report describes the ferrodec project state through commit
> `b3a8901` (pre Phase B, pre ADR-0029 2.0 release). The version
> table (`ferrodec` 1.17.0, `ferrodec-decimal64` 1.7.0,
> `ferrodec-decimal32` 1.7.0), the ADR count ("thirty"), and the
> §9.2 transcendental contract ("faithful to within one ULP, not
> correctly rounded") are all overtaken by subsequent work; the
> live equivalents are in the parent
> [`README.md`](../../README.md), the three crates' `CHANGELOG.md`,
> the [`docs/decisions/README.md`](../decisions/README.md) ADR
> index, and ADR-0032 (correctly rounded §9.2 transcendentals).
> The Appendix evidence ledger remains useful as session id
> provenance for the development arc through mid May 2026.
>
> Retained as a snapshot of the project's mid May 2026 narrative.

---

# ferrodec: capabilities, development arc, and verification surface

This report describes what ferrodec is, how it was built across a sequence of
Parnell and Claude working sessions, and the verification surface that carries
its correctness claims. It is a project narrative and a verification map, not a
design decision; the decisions themselves live in the Architecture Decision
Records under `docs/decisions/`. State that moves (queues, counts, the ready
list) is deliberately not frozen into this prose; where a number would drift
the text points at the live source instead.

The development model is the one stated in the README disclosure and is the
premise of everything below. Parnell sets the what and the why and signs every
merge into `main`; Claude drafts the how. **Parnell does not review the
generated code line by line.** This report is, in effect, the argument for why
that model produces a defensible decimal floating point library: the IEEE
754-2019 specification is the arbiter, the ADR is the durable substitute for an
absent reviewer, and an exact correctly rounded verification stack is what
actually catches the boundary defects a human reading would not.

## 1. Executive summary

ferrodec is a pure Rust, `no_std`, `forbid(unsafe)` implementation of the IEEE
754-2019 decimal floating point formats: `Decimal128`, `Decimal64`, and
`Decimal32`. It targets durability on hardware that does not yet exist (an
STM32U class embedded scientific calculator is the lead consumer) and it plants
a flag for a canonical Rust implementation in a domain that has a load bearing
external standard and no satisfactory pure Rust incumbent. Correctness is
placed in the type system and in a layered verification stack (decTest
conformance with a per bucket floor, an exact correctly rounded oracle, Kani
proofs, property and fuzz testing, metamorphic identities) rather than in line
by line human audit. As of this writing `main` is at `b3a8901`, the workspace
is seven crates, and there are thirty ADRs.

## 2. What ferrodec is, and its capabilities

### Formats and workspace topology

| Crate | Role | Version |
|-------|------|---------|
| `ferrodec` (root) | `Decimal128`: 34 digit precision, exponent `10^-6143` to `10^+6144` | 1.17.0 |
| `ferrodec-decimal64` | `Decimal64`: 16 digit precision, `10^-383` to `10^+384` | 1.7.0 |
| `ferrodec-decimal32` | `Decimal32`: 7 digit precision, `10^-101` to `10^+96` | 1.7.0 |
| `ferrodec-ieee` | Shared `Status`, `RoundingMode`, IEEE class metadata | 0.1.4 |
| `ferrodec-transcend` | Shared 50 digit `Extended` transcendental kernel | 0.1.0 |
| `ferrodec-multiword` | Fixed width `U256`/`U384`/`U512` intermediates | 0.1.0 |
| `ferrodec-test-support` | decTest harness and oracle scaffolding (`publish = false`) | 0.1.0 |

The workspace forbids `unsafe` at the lint level, pins MSRV at Rust 1.84, and
is dual licensed MIT or Apache 2.0. Production code is `no_std` and takes no
`alloc` dependency; the oracle and conformance machinery is dev only and never
ships in a built artifact.

### Operation surface

Every arithmetic operation returns `(value, Status)` and takes an explicit
`RoundingMode` (the five IEEE directions), a design fixed early in ADR-0002 and
ADR-0003 (per operation status word, method only API, `core::ops` opt in).

- Arithmetic: `add`, `sub`, `mul`, `div`, `fma`, `sqrt`, `rem` (IEEE
  nearest even) with `rem_trunc` for the C99 and decTest truncating variant.
- Classification and ordering: the §5.4.2 and §5.7.2 predicates,
  `canonicalize`, the §5.10 total order (`total_cmp`,
  `compare_total_magnitude`), numeric `partial_cmp`.
- Quantum operations (§5.3): `quantize`, `same_quantum`, `scaleb`, `logb`,
  `next_up`, `next_down`, `radix`.
- Round to integral (§5.3): `floor`, `ceil`, `trunc`, `round`,
  `round_ties_even`, `round_to_integral`, and the INEXACT raising
  `round_to_integral_exact`.
- §9.2 transcendentals, faithfully rounded to within one ULP at the stated
  precision: `exp`, `exp2`, `ln`, `log2`, `log10`, `pow`, `cbrt`, the
  trigonometric and inverse trigonometric family with Payne Hanek argument
  reduction over the full exponent range, and the hyperbolic and inverse
  hyperbolic family.
- §9.6 magnitude operations: `min`, `max`, `min_magnitude`, `max_magnitude`
  (ADR-0028).

Storage is Binary Integer Decimal by default (ADR-0001); Densely Packed
Decimal interchange sits behind the `dpd` feature (ADR-0009). Transcendentals,
formatting, binary float bridges, `serde`, `num-traits`, and the `core::ops`
overloads are all behind feature flags so the embedded consumer pays only for
what it uses. The siblings share the same feature names and the same
`ferrodec-transcend` kernel, so a function is faithful at `Decimal128` parity
across all three formats.

## 3. The verification and testing surface

ferrodec treats verification as entropy reduction and orders it from most to
least durable: types, proofs, property tests, example tests, documentation. The
surface is layered so that a defect that slips one layer is caught by the next.

- **decTest conformance with a per bucket floor.** The IBM General Decimal
  Arithmetic corpus is vendored under `tests/vectors/` and routed through all
  three formats. The current standing is 8622 passing and 0 failing; the
  remaining skips are the non IEEE rounding directives ferrodec will not
  implement (`half_down`, `05up`, ADR-0005) plus DPD extension residue. The
  guard is exact match per bucket, not a global floor (ADR-0010, ADR-0013):
  a regression that trades a gain in one file for a loss in another is caught
  because each file's expected count is pinned. `KNOWN_ISSUES.md` is the live
  source for the per bucket table.
- **An exact correctly rounded oracle.** Where a tolerance envelope once
  accepted any result within one ULP, ADR-0021 replaced it with an oracle that
  forms the exact mathematical result and asserts the implementation is bit for
  bit correctly rounded, cohort and IEEE status included. The standing
  `property_fma_oracle` sweep is the live instance of this contract and is the
  guard that caught the most recent defect.
- **Independent oracle tiers (ADR-0026).** The transcendental corpus is
  cross checked against pure Rust `astro-float` (the `no_std` aligned choice over
  an MPFR FFI dependency), an Arb generated frozen vector corpus with `.prov`
  ball enclosures for the Table Maker's Dilemma worst cases, an optional MPFR
  gate, and an opt in mpmath differential harness. No tier trusts another.
- **Kani proofs with shim routing (ADR-0015, ADR-0016).** Bounded model
  checking under `cfg(kani)` proves NaN propagation, special value sign rules,
  encode and decode round trips, the canonical predicate equivalence and
  idempotence, total order antisymmetry, and the dispatch invariants.
  Harnesses call bounded special only shims so CBMC stays tractable; they never
  call production operations directly.
- **Property, regression, fuzz, metamorphic.** proptest sweeps per operation
  feed the oracle; named regression files pin every discovered defect shape bit
  for bit and status for status; six libFuzzer targets assert panic freedom and
  algebraic identities; metamorphic identities (ADR-0025) use condition number
  derived bounds and were audited to remove tautologies that would have
  cancelled shared kernel error. Cross precision oracles widen `Decimal32` into
  `Decimal64` and `Decimal64` into `Decimal128`.
- **What was tried and rejected, recorded.** The Verus pilot is archived in
  `verus/EXPERIMENT.md` with its graduation walls named (external crate
  opacity without an `assume_specification` axiom that is only copy paste as
  trust), so the dead end is not rediscovered (ADR-0004).

`docs/testing.md` carries the conceptual map and, in keeping with the
disclosure standard, names the residual frontier rather than hiding it.

## 4. The development arc (Parnell and Claude)

ferrodec was built across the recorded working sessions between 2026-05-02 and
2026-05-19. The arc is legible in the transcripts.

**Foundations (2026-05-02 to 2026-05-04).** The first session locked the major
decisions (BID-128, method only API returning `(Status, value)`, faithful
rounding with `astro-float` as the oracle) and wrote Kani harnesses in Phase 1.
By the end of the foundational work fifty Kani harnesses proved in about
twenty seconds and a 6609 case decTest runner was wired in as, in the session's
own words, real CI value. Verification was not retrofitted; it was present from
the first phase.

**The six agent correctness review (2026-05-09).** Six independent review
agents were run and synthesized into one prioritized verdict. It found real
HIGH severity defects, for example `pow(-1, ±Inf)` panicking where IEEE
754-2019 §9.2.1 mandates the result is 1. Each fix landed as its own commit
with a regression pin, including one that pinned the absence of double rounding
by comparing against the direct parse path. ADR-0010 captured the testing
strategy that came out of this review.

**Workspace extraction and the correctness trains (2026-05-11 to 2026-05-16).**
The siblings, `ferrodec-ieee`, and `ferrodec-multiword` were extracted into a
workspace (ADR-0011, ADR-0012, ADR-0013). The `Decimal64` and `Decimal32` H
tier correctness trains closed (ADR-0018, ADR-0019). The exact oracle
superseded the ULP envelope (ADR-0021), which reframed every later correctness
question as bit for bit rather than within tolerance.

**The FMA defect family (2026-05-16 to 2026-05-19).** A single defect family
recurred across the format family and is the clearest illustration of why the
verification stack earns its keep. `fd-7nf` was a static alignment window
defect in the `Decimal128` FMA kernel; `fd-9fi` was its sibling analogue
remediation (ADR-0022); `fd-42l` was a shared `round_and_pack_finite`
subnormal double rounding defect proven, by the exact oracle, to affect
multiply and divide as well as FMA; `fd-dc6` (2026-05-18) was the sibling
subnormal single rounding port that the standing oracle sweep caught after
ADR-0022 had ported only part of the parent fix family (ADR-0030). None of
these were found by reading the code. Each was found by a standing oracle or
conformance guard, reproduced before being fixed, and pinned.

**Faithful transcendentals and the 2.0 plan (2026-05-17 to 2026-05-19).** The
sibling transcendentals were unified onto one shared `Extended` kernel
(ADR-0024), three independent oracle tiers were established (ADR-0026), the
§9.6 magnitude operations completed the IEEE mandatory surface (ADR-0028), and
the breaking changes were consolidated into a single 2.0 plan (ADR-0029) so a
declined direction is relitigated against one document rather than from zero.

**The working disciplines that emerged.** The collaboration produced a set of
durable disciplines, each from a concrete failure. Signed merges are a YubiKey
coordination point: the agent stops at the boundary, surfaces the exact command
and state, and waits, because the gpg-agent that signs the merge is also the
SSH agent that pushes, and a touch misfire aborts the merge silently. Task
state lives in beads, not in prose. Cross session memory is a file per fact
with an index. The promotion ritual closes the loop: a resolved design item
becomes an ADR or a design document edit, and the tracker only ever holds
state. The informed consent OSS disclosure (originated 2026-05-11) is itself an
artifact of this arc; Parnell framed the model as a tech lead who owns the what
and the why while the agent owns the how, and insisted the load bearing
sentence be the explicit statement that he does not review the code line by
line, because without it the rest reads as overclaim.

## 5. Why IEEE fidelity, ADR discipline, and verification rigor are load bearing

These three are not process decoration. Under a no line by line review posture
they are the mechanism by which the work is trustworthy at all.

### The IEEE 754-2019 specification is the arbiter

The spec, and the decTest corpus that operationalizes it, settles design
questions that intuition gets wrong. The `pow(±1, ±∞) = 1` rule, the §6.3
preferred quantum, the §7.4 Clamped condition, and the §7.5 tininess and
underflow rules each decided behavior that would otherwise have been guessed.
The sharpest instance is the distinction between an IEEE mandatory operation
and a General Decimal Arithmetic extension: when the operation choice for the
§9.6 work was left open, the recommendation was driven explicitly by ferrodec's
central claim of full IEEE 754-2019, because `minimumMagnitude` and
`maximumMagnitude` are mandatory and load bearing for the flag the project
plants while `reduce`, `rotate`, and the logical operations are decNumber
extensions and are not. The single rounding contract of `fma` is a spec
property, and the entire `fd-42l` and `fd-dc6` line of work exists because
double rounding violates it on inputs no human reviewer would have enumerated.

### The ADR is the durable substitute for an absent reviewer

Because there is no line by line human review, the reasoning a reviewer would
have demanded has to be written down where a future stranger can reconstruct
it. ADRs are treated as the deliverable, not the commit message: a slice closes
by producing or amending an ADR, the index is watched for drift, and a
superseding ADR keeps the old one and links forward. ADRs are also explicitly
not laws. Early in the arc Parnell pushed back that ADR-0001 should not be
treated as law from god and that a strong prior may be relitigated when there
is a legitimate reason. That is the discipline working as intended: decisions
are durable and revisable, and the revision is itself recorded.

### Verification rigor is what actually catches the defects

Every material defect in this project was found by a standing automated guard,
not by inspection, and the team's instinct each time was to reproduce before
fixing and to distrust convenient signals. A delegated agent's proposed
`status.inexact() || dropped_sticky` patch for the subnormal bug was rejected
as a band aid because double rounding is not fixed by merging a sticky bit into
the flag; the correct fix was a single rounding directly to the subnormal
quantum, derived
from the spec contract. When a green test run contained the substring
`panicked` inside the passing test name `is_clamped_not_panicked`, that was
caught as a false positive rather than trusted, the same agent claim
verification reflex that recurred and held in the most recent session. When the
oracle itself proved unsound past a magnitude domain, the response was to bound
the sweep, never to widen the envelope. A neutral performance measurement
reverts; the ADR is the deliverable. The pure Rust `astro-float` oracle was
chosen over an MPFR FFI binding because the embedded ethos and clean CI outrank
the last percentage of battle testing.

## 6. Residual frontier and honest limits

This report describes a process and is not a warranty; the LICENSE governs use,
and issues are triaged as time allows with no SLA. The process has a named
exposure, stated here rather than hidden. The transcendental functions share
one `Extended` kernel, so an error in a primitive is correlated across the
derived functions and across the three formats; the independent oracle tiers
exist precisely because a shared kernel can be wrong in a way that a self
consistent test would not reveal. The kernel is faithful to within one ULP, not
correctly rounded, so the frozen vector tests gate faithfulness, not exactness,
for transcendentals. For numerical code the specific failure mode is a rounding
error on a boundary case the sweep did not generate; the `fd-42l` to `fd-dc6`
line is the existence proof that such cases are real, and the standing oracle
sweeps are the standing defense. ferrodec is a personal project with Parnell as
lead consumer, not a funded library with a maintenance team behind it.

## Appendix: evidence ledger

Each item cites the session by short id and date. Quotations are tight
paraphrases of the transcript unless in quotes.

1. **ADRs are revisable, not law.** Session `2b93b662`, 2026-05-08: Parnell,
   "ADR-0001 shouldn't be treated as a law from god. We should reason about our
   decisions and we can relitigate it if there's a legitimate need."
2. **The six agent review found spec violations.** Session `e11b8393`,
   2026-05-09: synthesized verdict; `pow(-1, ±Inf)` panicked where IEEE
   754-2019 §9.2.1 mandates 1; the parser scaled integer literals beyond 76
   digits to the wrong exponent. Five HIGH fixes, one commit each, regression
   pinned.
3. **No double rounding pinned by construction.** Session `e11b8393`,
   2026-05-09: a regression test pins the absence of double rounding by
   comparing to the direct f32 parse path.
4. **Verification present from Phase 1.** Session `e456f238`, 2026-05-02 to
   2026-05-04: `astro-float` chosen as the oracle on day one; fifty Kani
   harnesses proving in about twenty seconds; a 6609 case decTest runner wired
   as real CI value.
5. **A real arithmetic bug found by the `astro-float` oracle.** Session
   `b0226103`, 2026-05-07: the addsub path dropped the smaller operand once the
   exponent delta exceeded the alignment limit; found by the oracle, not by
   reading.
6. **ULP envelope distrusted, then superseded.** Session `b0226103`,
   2026-05-06: agent claims about a transcendental ULP envelope were sanity
   checked because they conflicted with recall; ADR-0021 later replaced the
   envelope with the exact oracle.
7. **Double rounding diagnosed, band aid rejected.** Session `7bbf13fc`,
   2026-05-17: "double-rounding isn't generally fixed by OR-ing sticky"; the
   correct fix is a single rounding directly to the subnormal quantum;
   deterministic reproducer written first.
8. **The defect family is shared.** Session `7bbf13fc`, 2026-05-17:
   `property_div` failed with the same `fd-42l` family as multiply, confirming
   a shared `round_and_pack_finite` subnormal double rounding defect; "I must
   not cross the signed-merge boundary on this premise."
9. **A green run distrusted.** Session `7bbf13fc`, 2026-05-17: the substring
   `panicked` matched inside the passing test name `is_clamped_not_panicked`
   and was caught as a false positive rather than trusted.
10. **fd-7nf and fd-42l are distinct.** Session `879a5355`, 2026-05-17: a trace
    of `round_and_pack_finite` separated the FMA kernel alignment family from
    the subnormal double round, so the right fix was applied to the right
    defect.
11. **IEEE mandatory versus GDA extension as the selection lens.** Session
    `1bb1b380`, 2026-05-19: `minimumMagnitude` and `maximumMagnitude` are §9.6
    mandatory and load bearing for the full IEEE 754-2019 claim; `reduce`,
    `rotate`, the logical operations are decNumber extensions and out of scope.
12. **The disclosure's load bearing sentence.** Session `e0b7cb31`,
    2026-05-11: the model is a tech lead who owns the what and the why while the
    agent owns the how; the explicit statement that Parnell does not review the
    code line by line is the load bearing piece, "without it the rest reads as
    overclaim."
13. **The Verus dead end, recorded.** Session `b0226103`, 2026-05-07: Verus
    rejects external crate consts without an `assume_specification` axiom that
    is only copy paste as trust; archived so it is not rediscovered (ADR-0004).
