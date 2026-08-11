# S5: transcendence-measure spike memo (fd-4zo.27)

> **NOT SPECIALIST VERIFIED.** This memo was drafted by a
> non-specialist from recalled knowledge of the number-theory
> literature, timeboxed as a feasibility spike (ADR-0059 Track C).
> Every theorem statement, constant, and citation below must be
> verified against the published papers by someone qualified before
> any shipped tier language, ADR, or KNOWN_ISSUES posture changes.
> The one live-verified fact is marked; everything else is recalled.
> No shipped claim changes on the strength of this memo.

## Question

ADR-0060 bought unconditional correct rounding for the algebraic
group by Liouville-type floors: an explicit lower bound on how close
a non-classified true value can sit to a rounding boundary converts
"terminates by Ziv iteration, depth unknown" into "decided within N
digits, unconditionally". The spike asks whether published effective
transcendence measures buy the same shape for any of the
transcendental families, in the three directions the lane plan
names.

## 1. Huge-argument trig via the irrationality measure of pi

**Result: positive in shape, pending verification.** This is the one
direction that plausibly yields a usable explicit cap.

The fact (live-verified 2026-08-10 against the arXiv abstract, the
only non-recalled citation here): Zeilberger and Zudilin prove the
irrationality measure of pi is at most **7.103205334137**, improving
Salikhov's 7.606308. An irrationality measure mu means: for every
eps > 0 there are C, q0 such that |pi - p/q| > C * q^-(mu+eps) for
all rationals with q >= q0. Both proofs are constructive (explicit
integral constructions), so C and q0 are believed extractable; that
extraction is the specialist step.

The derivation for the integer-argument case (x = N, a canonical
Decimal128 value with nonnegative exponent, N <= 10^6145): the
reduced argument against the nearest multiple of pi satisfies

    |N - k*pi| = k * |pi - N/k| > k^(1 - mu) ~ (N/pi)^(-6.1033)

so |sin(N)| >~ 10^(-6.11 * 6145) ~ 10^(-37,600) at the format's
extreme, and proportionally shallower at smaller adjusted exponents:

    cap(adj) <~ (mu - 1) * (adj + 34) + O(1)   decimal digits.

For the S1 thin-spot decades (adjusted exponents in the hundreds to
low thousands, where the falsification campaign found the sampled
evidence thin) the cap lands near 10^3 to 2*10^4 digits, matching
the lane plan's "finite cap near 10^4 digits" expectation. The
half-integer multiples (tan poles, cos zeros) reduce to the same
measure: mu(pi/2) = mu(pi) because scaling by a rational preserves
the measure (elementary; state and reprove at verification). The
negative-exponent case inserts the power-of-ten denominator D into
the quotient and worsens the cap only through q_eff = D*k, keeping
the same formula with adj + 34 as the driver.

**Scope boundary, stated hard:** mu(pi) bounds only the approach to
the ZERO and POLE anchors, i.e. how completely a huge argument can
cancel against a multiple of pi/2. That is exactly the Ziv
termination question for the reduction (and the S1 thin spot's
geometry), but it says NOTHING about how close sin(N) can sit to a
generic rounding boundary y* != 0, +-1: that is algebraic
independence territory (section 3) and stays on the statistical
model. The buyable upgrade is therefore: the unbounded rung's trig
termination gets an explicit worst-case depth, and the reduction's
worst-case cancellation gets an unconditional floor. It is NOT a
correct-rounding tier upgrade for the trig family.

**Verification path:** extract (C, q0) from Zeilberger-Zudilin (or
Salikhov, whose weaker 7.6063 still gives a cap of the same shape);
confirm the measure statement covers all q >= q0 rather than
infinitely many q; re-derive the two display formulas above;
price a consult only if the extraction stalls.

## 2. Matveev linear forms in logarithms (exp, ln, pow, atan2)

**Result: theoretical closure only; practically useless constants.**

The boundary-distance question for the exponential family reduces to
non-homogeneous linear forms in one logarithm: for decimal rationals
x and y*, |e^x - y*| ~ y* * |x - ln y*|, and Baker-Matveev theory
lower-bounds |x - ln y*| effectively. pow reduces to two logarithms
(|y ln x - ln z|), and atan2 reduces to logarithms of Gaussian
rationals over Q(i) (arctan t = Im ln(1 + it); degree doubles the
constants; the reduction is standard but must be re-derived
carefully at verification, per the lane plan's warning).

The shape of Matveev's bound (recalled, constants NOT trustworthy
here): |Lambda| > exp(-C(n, d) * prod A_i * log B) with C(2, 1) of
order 10^10 to 10^11, A_i bounding the logarithmic heights of the
algebraic inputs (for Decimal128 operands the heights reach
ln(10^6178) ~ 1.4 * 10^4), and log B ~ 10^2 for the coefficient
sizes. The exponent lands at order 10^16 or beyond: a floor near
10^(-10^16), i.e. a Ziv termination cap of tens of quadrillions of
digits. Order-of-magnitude slop in the recalled constants cannot
rescue this; the conclusion is robust.

What it still buys: the unbounded rung's termination on exp, ln,
pow, and atan2 becomes a THEOREM (with an absurd but finite bound)
instead of a conjecture-shaped confidence. That is worth one
sentence in the S6 posture text and nothing else. No budget, no
tier, no cap worth engineering against.

## 3. Shidlovskii E-function measures

**Result: negative, as the lane plan predicted.** The values of
E-functions (exp, sin, cos at algebraic points) fall under
Shidlovskii's quantitative algebraic-independence theory, which is
where the generic-boundary question from section 1 would have to be
answered. The published general theorems are effective in the
technical sense but their constants are not explicit: the
literature's own phrase is "effective but not explicit", and no
amount of careful reading by a non-specialist turns an unspecified
constant into a budget. Writing this down as a checked dead end is
the deliverable: the generic-boundary TMD depth for the
transcendental families keeps the ADR-0059 statistical model as its
only quantitative account, and S2's deep-margin campaign remains
the right instrument on that frontier.

## Consequences

- Nothing shipped changes now.
- After specialist verification of section 1, one ADR could record
  the explicit trig reduction floor and the unbounded rung's
  explicit termination cap for trig; S6's KNOWN_ISSUES posture can
  then cite it. powr's negative result (ADR-0060) is unaffected:
  its obstruction is the unbounded algebraic degree of its
  boundary set, not a missing measure.
- Sections 2 and 3 close their directions: one sentence of
  termination-theorem comfort, one recorded dead end.
- Registry entries accompany this memo (salikhov-2008,
  zeilberger-zudilin-2020, matveev-2000, shidlovskii-e-functions),
  each marked with its provenance class; only the arXiv abstract
  was live-verified during the spike.

## Related

ADR-0059 (the lane and its statistical model), ADR-0060 (the
Liouville precedent this spike tried to generalize), the S1
campaign record (the thin spot), fd-4zo.27 (bead).
