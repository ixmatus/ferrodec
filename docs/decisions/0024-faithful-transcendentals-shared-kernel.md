# ADR-0024: Faithful sibling transcendentals on one shared Extended kernel

- **Status**: accepted
- **Date**: 2026-05-17

## Context

`ferrodec` (Decimal128) computes every §9.2 transcendental in its own
50-digit `Extended` pipeline, faithfully rounded and Kani-bounded. The
sibling crates `ferrodec-decimal64` and `ferrodec-decimal32` shipped a
deliberately narrow v1.0: every transcendental converted to `f64`,
called `libm`, and converted back. That detour caps achievable
precision at roughly 10^-15 relative (one digit below decimal64's 16,
far above decimal32's 7 only by luck of the wider binary mantissa),
saturates exponent ranges far short of each format's true domain
(`exp` saturated near x = 709 instead of decimal64's true overflow at
x ≈ 886), and routes argument reduction through a binary type whose
mantissa cannot even represent the decimal argument exactly. Half
implementations consume the canonical name; leaving the siblings on
`f64` while the parent is faithful is exactly the asymmetry the
frugality principle rejects.

The fd-r0l train removes the detour. The question this ADR settles is
*how*: what correctness contract the siblings promise, where the code
lives, how the seam is shaped, and how the result is verified without
regressing the formally-verified parent.

## Decision

**Faithful rounding (≤ 1 ULP), not correctly rounded.** IEEE 754-2019
§9.2 *recommends* but does not *require* correctly rounded results for
the recommended (transcendental) operations. The family declares the
weaker faithful contract (ADR-0021): the returned value is one of the
two representable values adjacent to the true result. Two stronger
directions were considered and rejected, recorded here so they are not
relitigated:

- **Ziv-style correctly rounded with a retry loop.** Rejected: Ziv's
  technique terminates in practice but has no proven worst-case bound
  for decimal; the loop bound becomes a runtime parameter, not a
  discharged invariant. A correctness property that cannot be proven
  total conflicts with the verification-first posture and would ship a
  silent unbounded loop into a no_std embedded target.
- **True provably correct rounding.** Rejected: correct decimal
  rounding requires solving the Table Maker's Dilemma, which needs
  proven hardest-case bounds for each transcendental at decimal64 and
  decimal128 widths. No such bounds exist in the literature, and
  deriving them is a research programme, not an engineering slice. A
  claim the project cannot discharge is worse than a weaker claim it
  can.

**One shared kernel crate, not per-sibling code.** The proven
Decimal128 `Extended` kernel was lifted verbatim into a new
`ferrodec-transcend` crate, generic over a `DecimalFormat` seam, and
the parent re-instantiated on it. Two alternatives were rejected:
keeping the kernel in `ferrodec` and having the siblings depend on the
parent (rejected: a sibling must not pull the entire Decimal128 crate,
and it inverts the dependency arrow), and duplicating a narrowed kernel
into each sibling (rejected: three copies of a Kani-proven numeric
core is the maintenance tax the frugality principle exists to refuse,
and the proofs would not transfer). `ferrodec-multiword` holds the
wide-integer primitives as its own foundation crate so the frozen
proven core depends on a stable base, not on `ferrodec-transcend`.

**The seam is three generic boundaries plus format-provided domain
limits.** The kernel touches the concrete format only at
`to_extended_parts` (decode in), `round_and_pack_finite` (round out,
forwarding to the format's own verified rounder), and the
`recip_seed` / `sqrt_seed` Newton seeds. The `exp` overflow and
underflow thresholds are format-provided
(`DecimalFormat::exp_overflow_limit` / `exp_underflow_limit`), derived
per format from its `E_MAX` / `E_MIN` / `PRECISION` and documented at
each impl (decimal64 887/918, decimal32 224/235, Decimal128
14150/14221). Everything else the kernel needs is loop-free and
exposed directly.

**Oracle architecture (Design A): astro-float confined to
`ferrodec-test-support`.** The faithful-rounding oracle builders
(`transcend_oracle::oracle::{exp,ln,exp2,log2,log10,cbrt,sin,cos,tan,
asin,acos,atan,atan2,sinh,cosh,tanh,asinh,acosh,atanh,pow}`) compute
the exact value with astro-float at 256-bit precision once, in the
shared test-support crate, which also re-exports `BigFloat` / `Consts`.
Every sibling feeds its own exact `{:e}` string to those builders and
brackets at its own ULP. The earlier "widen decimal32 through
decimal64" tier proved unnecessary: under Design A decimal32 is
structurally a direct-tier consumer that simply never names
`astro_float`, so `ferrodec-decimal32`'s manifest stays
astro-float-free (the oracle compiles only transitively inside the
dev-dependency). The faithful bracket is asserted exactly, never as a
± ULP envelope (ADR-0021).

**Behaviour-neutral parent migration, enforced by exact-match
regression.** The Decimal128 instantiation of the shared kernel is
byte-identical to the pre-extraction kernel. This is guaranteed by the
parent's unchanged property and conformance suites, not by a file
freeze: across the entire train (P0a through P5) the diff of the
repo-root `src/` and every `verify/` directory is empty.

## Consequences

- Every transcendental on every member of the family
  (`exp ln exp2 log2 log10 cbrt sin cos tan asin acos atan atan2 sinh
  cosh tanh asinh acosh atanh pow`) is now faithfully rounded across
  the format's true domain, proven by per-function 5-mode property
  suites. The capability asymmetry against the parent is closed.
- `libm` is no longer a dependency of either sibling; the `f64_bridge`
  shim is deleted. The published dependency graph gains the
  `ferrodec-transcend` and `ferrodec-multiword` workspace crates
  (pulled by the transcendental features). The siblings remain no_std
  and astro-float-free.
- The faithful, not correctly rounded, contract is a deliberate
  public limitation. Callers that need the exact best-rounding of a
  transcendental do not get it here; the doc comments and READMEs say
  so plainly. The specific failure mode this contract is exposed to is
  a result that is one ULP off the correctly rounded value on a
  boundary input the property suites did not draw.
- The R1 spike (fd-57z) confirmed the Payne-Hanek argument reduction
  needed no generalisation: it is already generic over `F::BIAS`, and
  the 2/π table plus the wide-integer product machinery are sized for
  the widest format, so the narrower siblings are a strict subset with
  more precision headroom (≈ 27 surviving digits for decimal64,
  ≈ 36 for decimal32, against the 9 the parent relies on).
- Verification cost moved into the test harness, not the shipped
  artifact: the oracle, its 256-bit precision, and the directed-mode
  side logic live once. Two discovered test-construction defects
  (fd-dfs: exp/ln-family sweeps generated out-of-domain inputs whose
  result overflows; fd-3cd: the moderate sincos sweep outran the
  fixed-256-bit oracle's sound magnitude range) were fixed by
  out-of-domain skips that match the existing `coef == 0` idiom and do
  not weaken the bracket. The lesson: a fixed-precision oracle has a
  sound-magnitude domain, and a faithful sweep must stay inside it or
  scale the oracle (the `property_sincos_large` pattern). Neither was a
  kernel or seam defect.

## Related

- Plan: `~/.claude/plans/resume-the-fd-r0l-sequential-seal.md`
- Other ADRs: builds on ADR-0021 (the exact faithful-rounding oracle
  contract) and ADR-0016 (Kani special-only shims; every sibling's
  `verify/*.rs` is 0-diff across the train, so CBMC never enters the
  Extended kernel). Supersedes nothing.
- Beads: `fd-r0l` (parent), `fd-8za` (P2), `fd-57z` (R1 spike),
  `fd-2rq` (P3), `fd-28k` (P4), `fd-qlh` (P5), `fd-dfs` / `fd-3cd`
  (discovered test-construction defects, both closed in-train).
- Commits: P0a `82a7fe1`; P0a.2 `d9106b0`..`756d336`; P0b `4ae8d67`;
  P1 `5adfa2d` / `9efecb3`; P2 `dc40f8d` `d790fd6` `b9b7071`
  `9998e45`; P3 `dc30b59` `1555528` `b30ae5d` `46758b4`; P4 `337213c`
  `3b3357f` `c5c708a` `224eca9`; P5 `3d6534a` `a65ed6e` `7bba16b`
  `3ffe137` `2de3f81`; fd-3cd `4e3c484`.
