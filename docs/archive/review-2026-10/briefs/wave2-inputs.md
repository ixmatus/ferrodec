# Wave 2 inputs (every A reader reads this after common.md)

Wave 1 closed at `765ac1e`. What it established, and what it hands you:

- **Trust tables.** `readers/C1.md` (transcendental test surface) and
  `readers/C2.md` (oracle, conformance, Kani) end with instrument trust
  tables. Cite them when you answer "why the stack missed it".
- **The faithful oracle is one sided (C1-1).** `assert_faithful` and
  its family accept any result below the true value, because
  astro-float 0.3.6's `abs_cmp` compares with sign. Do not cite a
  passing `property_*` transcendental suite as evidence of anything on
  the low side.
- **The exact integer oracle is validated (C2).** 309,000 cases against
  libmpdec at every mode and format, zero unexplained mismatches. It
  models no CLAMPED and no out of format operands.
- **Two P0 classes already found.** (1) `Extended::recip` and `sqrt`
  seed through `to_format`; when `x²` or `1/x` leaves the format's
  range the seed is an infinity and `from_format` panics: `asinh`,
  `acosh`, `atan2`, the siblings' `atan2_pi` (`reproducers/P1B-2`).
  (2) `Decimal128::fma` delivers `10^34 − 10` for `10^34 − 1` under the
  directed modes on an exact power of ten product with a far
  opposite sign addend (`reproducers/P1B-3`). Do not re-report these;
  do look for their siblings in your scope.
- **The coordinator's derivations** (`phase1b-derivations.md`): the
  Newton table (Decimal32 reaches 25 digits at rung 1 where the model
  assumes 50), the adjudicator read (no defect; `Equal` panic is a
  design question), the four budget verifications, and P1B-1: the
  catalog's "at most 33 leading zeros" cancellation premise is false
  (true Decimal128 worst case 37.88 digits at
  `9308532438209917461067659354862169E4639`; `reproducers/P1B-1`).
- **Contradiction list** (`wave1-verification.md`, the section before
  the per reader ones): documents that still state the refuted
  ADR-0032 envelope, the Tier 2 figure's units (per domain versus per
  call, D3-13), the `tan ±∞` doc, the sibling wrapper docs saying
  "faithful".
- **Recurring defect classes to carry into every file you read:**
  special case dispatch order (an early return on a quiet NaN before
  the signaling check: C2-2, C2-3); binade drops at powers of ten
  (C2-1); seeds and intermediates that round through the destination
  format (P1B-2, fd-k9t); tests that compare a copy of a constant with
  itself (C1-12) or never call the classifier they name (C1-13).
