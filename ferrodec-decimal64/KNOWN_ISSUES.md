# ferrodec-decimal64 known issues

Slice D of the 1.15 cycle surfaced three H class correctness bugs
through the conformance dispatcher (see ADR-0017 in the workspace
root). The 1.4.0 decimal64 correctness train closed them; ADR-0018
records that train and supersedes ADR-0017. The three entries below
are kept for the audit trail with their closing commits. Two
documented limitations remain and are described after them.

## Closed (1.4.0): finite operand addition magnitude loss

* **Status**: closed in 1.4.0. Fixed by `778650a` (H1).
* **Reproducer**: `ddAdd.decTest:358` (case `ddadd360`).
* **Was**: `Decimal64::add(...).0` returned `0E+50` (bits
  `0x2300000000000000`) where the spec expects `1.0000E+5`, the
  value 100000 (bits `0x223C000000002710`). When exactly one operand
  was zero and the exponent gap exceeded the working window, the
  alignment funnel collapsed both magnitudes to zero and returned
  `0E+exp_hi` instead of the non zero operand.
* **Fix**: when exactly one operand is zero the result is the other
  operand requantised to the §6.3 preferred quantum
  `min(exp_a, exp_b)`. Same bug class as the `Decimal128`
  long mantissa magnitude loss closed in 1.13.x.

## Closed (1.4.0): rounding direction at the precision boundary

* **Status**: closed in 1.4.0. Fixed by `efde899` (H2), with the
  residual boundary family `fd-d47` closed by `502e8a5` (add) and
  `d819e83` (fma).
* **Reproducer**: `ddAdd.decTest:802..821` (`ddadd71100..71119`),
  the negated mirror near `71200..71219`, one `ddMultiply` case,
  and 20 `ddFMA` mirrors; the `fd-d47` residual added the
  `ddadd64xx` / `ddadd713xx` and `ddfma364xx` families.
* **Was**: on an effective subtraction the truncated lower operand's
  residue subtracts from the result magnitude, but the funnel's
  sticky convention read that residue as additive, so a value at the
  16 digit precision boundary rounded the wrong way (`99.999...9`
  became `100.0...0`, and the sign mirror produced `-100.0...0`
  where `-99.999...9` was expected). The `fd-d47` residual had a
  distinct root cause: the alignment used a static 22 digit window
  that truncated the lower operand prematurely when the dominant
  coefficient was small (`1E16` is stored as `coef 1, exp 16`).
* **Fix**: borrow one ULP correctly on effective subtraction (H2),
  then replace the static window with a dynamic per side shift bound
  keyed on the actual digit count (the `rem.rs` H5 approach), so the
  subtraction stays exact whenever it fits in `u128` and the
  boundary tie is decided on the true residue. The full `dd*`
  conformance corpus now runs with zero failures.

## Closed (1.4.0): pack_finite biased_exp precondition panic in FMA

* **Status**: closed in 1.4.0. Fixed by `5b7fcab` (H3).
* **Reproducer**: `ddFMA.decTest:281` (case `ddfma2504`), operands
  `fma 0E-260 1000E-260 0E+384`, spec answer `0E-398` with the IEEE
  754-2019 §7.4 informational `Clamped` condition raised.
* **Was**: the FMA zero product path summed the product operand
  exponents `(-260) + (-260) = -520`, took the minimum with the
  addend exponent `+384`, and fed the resulting out of range biased
  exponent into `pack_finite`. Debug builds panicked at the
  `debug_assert!(biased_exp <= BIASED_EXP_MAX)`; release builds
  compiled it out and packed garbage bits.
* **Fix**: typed `BiasedExp` and `Coefficient` newtypes in `bid.rs`
  whose constructors prove the range, so the invariant is enforced
  by the type system rather than a debug assertion. The §6.3 plus
  §7.4 clamp now raises `Status::CLAMPED`.

## Documented limitation: GDA ideal-exponent Clamped on pre-normalised operands

* **Status**: documented limitation, not a value error. Recorded at
  H9 of the decimal64 correctness train; deferred beyond 1.4.0.
* **Symptom**: a few decTest cases whose operands carry an exponent
  outside the format's quantum range (e.g. `dddiv497`
  `0E+380 / 1000E-13 -> 0E+369 Clamped`, `ddrem422`
  `1E+384 % 1E+383 -> 0E+369 Clamped`, `ddrem424`) mark the IEEE
  754-2019 §7.4 `Clamped` condition, but `ferrodec-decimal64`
  returns the bit exact correct value with `Clamped` not raised.
* **Mechanism**: `Decimal64::parse_str` normalises an out of range
  operand quantum into the cohort at parse time (`1E+384` is stored
  as coefficient `10^15`, exponent `+369`). The downstream operation
  then sees in range operand exponents and performs no
  representational clamp, so it raises no flag. GDA's reference
  computes `Clamped` from an extended precision ideal exponent that
  predates this normalisation. Replicating that bookkeeping is a
  large change with no value impact.
* **Why deferred**: the conformance harness classifies `Clamped` as
  informational (`status_conformance_eq` masks it, mirroring
  `decode_conditions`), so per file pass counts are unaffected and
  the returned values are exact. The §7.4 flag is raised at genuine
  in operation clamp sites (the `round.rs` §6.3 coefficient pad and
  zero exponent clamp, and `div.rs`'s finite or zero over Infinity
  Etiny path); regression tests pin those in
  `tests/regression_h9_clamped.rs`.
* **Fix landing**: a follow up may thread an ideal exponent
  accumulator through the arithmetic paths if a downstream consumer
  needs the informational flag on these cases.

## Documented limitation: conformance dispatch is arithmetic plus tosci / apply

* **Status**: by design. The 1.4.0 train wired the arithmetic
  surface; the remaining `dd*` operations are out of scope for this
  release.
* **State after 1.4.0**: the dispatcher runs `tosci` / `apply`,
  `add`, `subtract`, `multiply`, `divide`, and `fma` with exact per
  file pass counts guarded per ADR-0010 (`ddAdd` 973, `ddSubtract`
  514, `ddMultiply` 444, `ddDivide` 702, `ddFMA` 1318, `ddBase`
  708; the full corpus runs with zero failures). The remaining
  `dd*.decTest` files (comparison, the quantum family, the bitwise
  and copy family, DPD interchange) still route to `Outcome::Skip`.
  The underlying methods exist and are exercised by unit and
  property tests; the gap is dispatcher coverage, not implementation
  coverage.
* **Closing this gap**: enable each remaining op's dispatch arm in a
  follow up slice as its conformance is verified, on the same exact
  match per file cadence.
