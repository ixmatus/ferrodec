# P1B-1: the Payne-Hanek cancellation depth premise

Finding: `ferrodec-transcend/src/argred.rs` (`make_residual` doc) and
the `SIN`, `COS`, `TAN` budget itemizations in `ladder.rs` assume at
most 33 leading zeros in the reduced fraction `y = x·2/π − round(x·2/π)`
for any representable input, hence 43 surviving digits at rung 1
(76 − 33) and 110 at rung 2 (143 − 33). The premise is stated, not
derived. A continued fraction search over every Decimal128 exponent
shows the true worst case is 37.88 digits, reached at
`x = 9308532438209917461067659354862169E4639`, and every one of the
6,146 exponents has a worst input deeper than 33 digits.

Environment: `/usr/bin/python3` 3.9.6 with mpmath 1.4.1 (user site:
run without `-I`); the stable Rust toolchain for the scratch crate.

- `cancellation_depth.py`: the search (about six seconds). For each
  format and each exponent `q`, `min_c ||c·θ_q||` over `c < 10^P` is
  the last continued fraction convergent denominator below `10^P`
  (best approximation theorem); `θ_q` is read off the decimal
  expansion of `2/π` at 6,400 digits.
- `scratch-crate/`: calls the public `argred::reduce::<Decimal128>` on
  the worst input and prints the rung 1 residual, the delivered `sin`
  and `cos` in all five modes, and nine `bigconst` generators at five
  depths. Build with `CARGO_TARGET_DIR` outside the repository.
- `scratch-crate-output.txt`: the captured residual and deliveries.

Measured: rung 1 residual relative error `−5.7e-38` (5.7e12 rung 1
units; the catalog's model predicts ≈ 6.3e-38 from the 38 digit `π/2`
plus ≈ 7.6e-39 from the window). The delivered `sin` and `cos` are
correctly rounded in every mode on this input (checked against
mpmath at 6,400 digits), so no wrong value is witnessed; the finding
is the unsound stated premise and the rung 2 budget it mis-sizes.
