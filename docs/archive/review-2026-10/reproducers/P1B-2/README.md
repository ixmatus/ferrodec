# P1B-2 (from C5-1, C5-2): public API panics on finite inputs through the Newton seed

Finding: `Extended::recip` and `Extended::sqrt`
(`ferrodec-transcend/src/extended.rs:778-830`) seed their Newton
iterations by rounding the operand to the destination format
(`to_format::<F>`) and asking the format for its reciprocal or square
root. When the operand's square or reciprocal lies outside the
format's range, the seed is an infinity and `from_format` panics
("from_format requires a finite or zero datum"). Reachable from the
public API with ordinary finite inputs:

| kernel | Decimal128 | Decimal64 | Decimal32 |
|---|---|---|---|
| `asinh(x)`, both signs | `|x| ≥ 1E3073` | `|x| ≥ 1E193` | `|x| ≥ 1E50` |
| `acosh(x)` | `x ≥ 1E3073` | `x ≥ 1E193` | `x ≥ 1E50` |
| `atan2(y, x)` | `1/x` overflows: `x = 1E-6145` and below | `x = 1E-398` | `x = 1E-101` |
| `atan2_pi(y, x)` | not reproduced at the probed points | `atan2_pi(1E-390, 1E-398)` | `atan2_pi(1E-95, 1E-101)` |

Not affected at the same edges (probed): `rsqrt`, `hypot`, `rootn`,
`cbrt`, `atan`, `atan_pi`, `powi`, `pow`, `powr`, `tanh`, `atanh`,
`asin`, `acos`.

Reader C5 found the first two rows with libFuzzer in under fifteen
seconds each, once the orphaned fuzz crates were made to build (C5-3:
no fuzz target has built since 2026-05-09, which is why the panic
freedom contract those targets exist to check went unchecked).

- `reader-C5-repro_panic/`: the reader's reproducer, rerun by the
  coordinator with identical output.
- `panic_probe/`: the coordinator's widened probe over every seeded
  kernel on all three formats; `panic_probe_output.txt` is its
  output (20 of 116 calls panic). Build with `CARGO_TARGET_DIR`
  outside the repository; the crate declares its own `[workspace]`.

Closure (recorded, not applied): seed from a rescaled in-range
operand (`Extended::recip` and `sqrt` can halve the exponent or
split it into an even part before `to_format`), or have the kernels
dispatch the out of range ratios first (`asinh` and `acosh` to
`ln(2|x|)` once `x²` would overflow; `atan2` to `±π/2` once `|y/x|`
would overflow). Tests: one regression row per format per kernel at
the threshold and at the format maximum, in all five modes, plus a
fuzz lane that actually builds.
