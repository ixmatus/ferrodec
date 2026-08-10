# ADR-0061: the pi scaled family (sinPi through atan2Pi): exact reduction, anchor dispositions, tier, naming, and gate

- **Status**: accepted (D4 design pass, fd-4zo.26; Parnell approved
  the brief's recommendations in full, 2026-08-09)
- **Date**: 2026-08-09

## Context

Track D's last surface group is the IEEE 754-2019 §9.2 pi scaled
family: `sinPi`, `cosPi`, `tanPi`, `asinPi`, `acosPi`, `atanPi`,
`atan2Pi`. The charter ordered it last because it was the largest
unknown; the design pass brief
(`plans/2026-08-09-d4-pi-scaled-design-brief.md`) closed the unknowns
and this ADR records the rulings. Two structural facts drive
everything: argument reduction is exact decimal arithmetic (`x mod 2`
on the operand's own digits; no pi constant, no Payne and Hanek, no
truncation item — the S1 thin spot has no analog here), and the
decimal formats cannot represent the abscissas `k ± 1/6` and
`k ± 1/3`, which collapses the textbook special value tables to a
shape cleaner than any shipped group.

## Decision

### Exact and tie classification (spec obligations, proofs at the classifier sites)

Per the brief's §2 table: `sinPi` and `cosPi` classify exactly at
integers and half integers only; `tanPi` additionally at quarter
integers (`±1`) and half integers (`±∞` with `DIV_BY_ZERO`, parity
table re-extracted from §9.2.1); the inverse four classify on their
finite tables (`asinPi(±1/2) = ±1/6` and `acosPi(±1/2) ∈ {1/3, 2/3}`
are the non-terminating rational class: representable inputs, never
exact outputs, the `rsqrt` `1/q` argument verbatim; `atanPi(±1) =
±1/4` and `atan2Pi`'s axis and diagonal rows are exact). **The
family has no nearest mode ties**: the forward family's rational
values are `{0, ±1/2, ±1}` by Niven and each is a grid point; the
inverse family's rational values are the finite tables. The no-ties
proof is a spec obligation written before any kernel; if it fails,
ties get the D3 classifier treatment and only scope moves.

### Anchor dispositions (the CLOSED list)

The brief's §3 table is blessed as closed: ADR-0051 residual
channels at `cosPi`'s `±1` (quadratic hug, side theorem `|cos| < 1`
off the exact set), `tanPi`'s `±1` near quarter integers (linear
hug, side from `δ`'s sign and parity), `atanPi` and `atan2Pi`'s
`±1/2` at large arguments, `acosPi`'s `1/2` at tiny arguments; plain
ladder for `sinPi` and `asinPi` everywhere (slopes `π` and `1/π`
give no grid anchor — the `log2p1` precedent) and for `tanPi`'s pole
neighborhoods. A hug family discovered outside this list after the
specs land is a spec defect, not a surprise: the exact reduction
pins every neighborhood to an exact fractional part `δ`, which is
what makes the enumeration finite and checkable.

`tanPi` carries a deliberate absence: **no overflow gate**.
Representing `n + 1/2 + δ` forces `δ ≥ 10^(adj − P + 1)`, so the
pole magnitude caps near `10^P` (`~3.2×10^33` at Decimal128),
decades inside every format ceiling. The cap is proven at the spec
and stated at the kernel, so the absence reads as designed.

### Tier (the negative result first)

`sin(πp/q)` is algebraic of degree growing with `φ(2q)`: at format
denominators the degree is astronomically past any fixed width
comparison, so **the ADR-0060 adjudicator route is closed for this
family** (the powr wall's shape), and the S5 spike does not gain it.
The claim is Tier 1 by construction plus the Tier 2 model — with no
reduction caveat, budgets that lose trig's dominant item entirely
(sketch: rung 1 near `10^4` units against trig's `1.5×10^14`;
escalation `~10^−12` per call against trig's 3%), and the classified
and anchored sets unconditional. If the no-ties proof closes,
`ladder_audit` is vacuous for the family by construction.

### Naming and gate

Public names `sin_pi`, `cos_pi`, `tan_pi`, `asin_pi`, `acos_pi`,
`atan_pi`, `atan2_pi` (C23's `sinpi` family snake cased exactly as
std snake cased `expm1`), each with `doc(alias)` for both the IEEE
spelling (`sinPi`) and the C23 spelling (`sinpi`). The family ships
under its own **`trig-pi`** feature which does NOT imply `trig`: it
needs none of the reduction machinery, and the embedded revolutions
based user gets correctly rounded `sin_pi` without Payne and Hanek
tables; the thumbv6m size delta is measured and stated at landing.

### Instruments

- MPFR gate: native `*_pi` arms — checked fact, not a fork: the tree
  vendors MPFR 4.2.2 (`gmp-mpfr-sys 1.7.1`) and `rug 1.30` exposes
  all seven with explicit rounding variants.
- Arb corpus: `python-flint` exposes `sin_pi`/`cos_pi` natively
  (probed); the generator gains the seven functions via `--funcs`
  with names appended after every legacy name, its exact output
  filter learns the tables above, and the planted families are the
  anchor neighborhoods at exact `δ` decades plus `asinPi(±1/2)`.
- §9.2.1 special tables: re-extracted at spec time from the free
  proxies (the C23 draft's `sinpi` family Annex F rows and the MPFR
  4.2 documentation, both of which track 754-2019), cross checked
  against each other and mathematical necessity; the standard stays
  pointer only per the registry posture, and any proxy disagreement
  is surfaced to Parnell rather than resolved silently.

### Process (one deviation, named)

Supervisor owns the ADR, specs, budgets, side theorems, `exact.rs`
classifiers, feature plumbing, corpus, gates, and docs; kernels and
their test files are delegated. **Deviation from the
one-agent-per-operation pattern**: D4's kernels live in two files
(`sincospi.rs` for the forward trio, which shares one exact
reduction core and the identity `cosPi(x) = sinPi(x + 1/2)` exactly;
`inverse_trig_pi.rs` for the inverse four), so the delegation is two
agents by FILE, not seven by operation. The D3 splice lesson is the
reason: per operation agents on shared files recreate exactly the
integration failure the deterministic rebuild discipline exists to
contain, and the validated pattern's per operation shape assumed
per operation files. Worktrees cut from the D4 branch AFTER the
shared seams land (budgets, classifiers, gates), which also
retires the D3 baseline hash failure mode by construction.

## Consequences

- The three most plausible ways this is wrong, inverted up front.
  First, a missed anchor family (the D3 review's repeated finding):
  countered by the closed list argument (exact `δ` enumeration), the
  planted anchor neighborhood corpus, and the no-ties obligation —
  a missed hug surfaces as a loud pin or audit failure, not
  silence. Second, §9.2.1 proxy drift (C23 or MPFR docs diverging
  from the standard's actual rows): countered by dual proxy cross
  check plus mathematical necessity, with disagreement escalated to
  Parnell, who holds the standard. Third, the no-ties proof failing
  at spec time on some inverse family rational: the fallback is the
  D3 tie classifier treatment; scope moves, correctness does not.
- The family becomes the surface's best behaved group: exact
  reduction, tight budgets, no ties, a standalone gate. That is
  also the marketing truth for users: `sin_pi(0.5) == 1` exactly,
  at speed, on an M0+.
- The adjudicator non-extension is recorded here so it is not
  relitigated: no minimal polynomial route exists at format
  denominators.

## Landing addendum (2026-08-10)

The family landed as designed; this section records where the
implementation taught the design something, so the errata are not
relitigated from the spec text.

- **cosPi anchor gate erratum.** Spec A gated the 1-anchor arm at
  `adj(δ) ≤ −⌈(P + 3)/2⌉`. The spec-A agent's margin table showed
  that at P = 7 the P + 3 gate admits `(πδ)²/2` up to
  `4.93·10^−8` against the boundary `5·10^−8`: 1.3 % of headroom,
  failing the ×10 discipline. The shipped gate is
  `⌈(P + 4)/2⌉`, whose worst case clears the boundary ×10 at every
  format. The derivation stands at the arm site in `sincospi.rs`.
- **tanPi ±1 anchor arm unreachable.** The reduced offset `δ` from
  an exact reduction of a P-digit input carries the quantum floor
  `|δ| ≥ 10^−P` near every odd quarter integer that is not itself
  representable territory (the classifier owns representable quarter
  integers). The spec's gate `adj(δ) ≤ −(P + 4)` therefore never
  fires. The arm shipped anyway with the proof at the site: its
  premise is checked, not assumed, so a future widening of the
  reduction cannot silently reopen the neighborhood. The same
  quantum floor is the honest reason `sinPi` has no anchor arm near
  integers (the spec's slope argument only covers zero).
- **atan2Pi hug side.** Spec B's two hug families are correct, but
  the tiny-ratio family's anchor sign follows the ordinate while its
  selection follows the abscissa (`x < 0` selects the ±1 anchor; the
  delivered sign is `sign(y)`); the four-case table at the site in
  `inverse_trig_pi.rs` replaces the spec's two-line sketch.
- **Wrapper paths.** The sibling wrappers live at
  `ferrodec-decimal{64,32}/src/ops/trig_pi.rs`: the sibling crates
  keep their transcendental wrappers under `ops/`, not `math/`. The
  spec's `math/` paths were the parent crate's convention applied by
  analogy.
- **fd-z26 (pre-existing, fixed in this landing).** Sibling
  `--features trig` builds alone did not compile: the internal
  `transcend_impl` module was gated on `exp-log`. Found by the
  spec-B agent at its baseline; the gate now covers every
  transcendental family feature. The narrow-feature blind spot (no
  CI lane builds each feature alone) is the residue, tracked on the
  bead.
- **Size (thumbv6m, release rlib delta).** Featureless base
  584,178 B; `trig-pi` alone 721,142 B (+137 KB, the shared
  extended-precision engine dominating); `trig` alone 720,742 B;
  `trig` + `trig-pi` 783,910 B, so the family's marginal cost on a
  build that already carries a transcendental cluster is +62 KB.
- **Verification at landing.** 3,494 new Arb-certified corpus rows
  across the seven operations (cap-hits 0), replayed byte-exact at
  all three formats with exact per-bucket pins; MPFR 4.2.2
  cross-validation through the native `sinu`/`asinu` family arms,
  zero disagreements; the full workspace matrix green including the
  `force_escalate` and `ladder_audit` lanes. The no-ties obligation
  and the closed anchor list held: no pin or audit failure surfaced
  a missed family.

## Related

- The design pass brief:
  `plans/2026-08-09-d4-pi-scaled-design-brief.md` (facts, sketches,
  and geometry; this ADR records the rulings).
- ADR-0051 (residual channels), ADR-0059 (ladder and tiers),
  ADR-0060 (the adjudicator whose route is closed here), ADR-0005
  (rounding scope), the D3 process record in
  `plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md`.
- Registry: `niven-irrational-numbers`, `arb-flint`,
  `ieee-754-2019`; C23 draft and MPFR 4.2 manual entries to be added
  at spec time with archives.
- Bead: fd-4zo.26.
