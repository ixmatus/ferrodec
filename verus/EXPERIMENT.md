# Verus pilot — aborted

This file records a 2026-05-06 experiment that introduced
[Verus](https://verus-lang.github.io/verus/) (a Rust-native
SMT-backed verifier) to ferrodec as a sibling proof crate, then
removed it after concluding the result couldn't fold into the main
crate without copy-pasted function bodies — explicitly the thing the
pilot was meant to avoid.

The notes here exist so future maintainers can resume from where this
one stopped, instead of rediscovering the same dead ends.

## What the pilot proved (when it ran)

A `verus/` sibling crate, `verus_builtin` plus `vstd::prelude::*`
imports, ran two phases under direct `verus --crate-type=lib` invocation:

- **Phase 1 — `Status` flag disjointness.** Five proofs / 23
  ensures clauses, all verified by Z3 in milliseconds via
  `assert(... by(bit_vector))`. The five `Status::F_*` constants are
  pairwise disjoint, `OK` is zero, every flag is non-zero, and
  `from_bits_truncate`'s mask leaves the result `<= ALL`.
- **Phase 2 — `pow10` correctness.** A recursive mirror
  `pow10_verified(k) -> u128` matches an inductive
  `power_of_ten(k: nat) -> nat` spec for every `k <= 38`. Verified
  with a single `assume(prev <= POW10_37)` on the inductive bound;
  Verus's nat reasoning over recursively-defined powers needed more
  induction machinery than the pilot scope absorbed.

Headline output:

```
verification results:: 26 verified, 0 errors
```

The Phase 1 + Phase 2 source is in git history at the commit before
this file landed; recover with `git log --diff-filter=A --
verus/src/lib.rs`.

## Why it didn't graduate

The plan called for "Phase 6 graduation" — folding the proofs into
the main crate so production code and verified code are the same
code. Two distinct walls blocked that:

### 1. `cargo verus verify` panics on the published `vstd`

The canonical Verus integration path is `cargo verus verify`. It
panics inside `rust_verify/src/erase.rs:405` while compiling
`vstd-0.0.0-2026-04-20-1748` from crates.io against the prebuilt
`verus-arm64-macos` binary at `0.2026.05.03.8b81855`:

```
thread 'rustc' panicked at rust_verify/src/erase.rs:405:80:
called `Option::unwrap()` on a `None` value
error: could not compile `vstd` (lib)
```

`cargo verus focus` (skip dep verification) hits the same panic
because `vstd` still needs to compile through Verus to expose its
metadata.

Reproduction is identical on the canonical hello-world produced by
`cargo verus new --lib`, so the failure isn't ferrodec-specific.
The pilot couldn't progress through the cargo integration on the
prebuilt toolchain.

### 2. Direct `verus` rejects external-crate items without
   `assume_specification`

Side-stepping cargo by invoking `verus --extern ferrodec=...rlib
--crate-type=lib src/lib.rs` and referencing
`ferrodec::verify_internals::F_INVALID` etc. produces:

```
error: ferrodec::verify_internals::F_INVALID is not supported
       (note: you may be able to add a Verus specification to this
        function with `assume_specification`)
```

`assume_specification` lets the proof reason about the external
item but only by attaching an axiom — e.g. `ensures result ==
0b0000_0001` — which is the same information as a copy-pasted
constant, just expressed as a trusted axiom instead of a literal.
That's not graduation; it's the duplication wearing a different hat.

### 3. `u128::pow` opacity (Phase 2 specific)

ferrodec's `pow10` is a one-liner: `10u128.pow(k)`. Verus has no
built-in lemma relating `u128::pow` to a recursive `power_of_ten`
spec, so verifying ferrodec's actual body would require either:

- A manual induction lemma about `u128::pow` written in Verus, or
- Replacing ferrodec's body with an inductive form Verus can chew.

The pilot wrote a *parallel* recursive `pow10_verified` and verified
that, then sync-tested its outputs against ferrodec's. Mathematically
equivalent but not the same code, so the proof doesn't apply to
ferrodec's `pow10`.

## What would unblock a future attempt

- A `vstd` release that doesn't panic under `cargo verus verify` on
  this binary, or a different binary that matches the published
  `vstd`. Then the proofs could live in `src/`-side `verus!{}` blocks
  gated by a Cargo feature, and the same source ferrodec compiles
  with stable cargo would also be what Verus verifies.
- Or a Verus-side lemma library for `u32::pow`/`u128::pow` so the
  Phase 2 proof can apply to ferrodec's actual one-liner instead of
  a parallel recursive body.
- Or an `assume_specification`-only graduation path, with the
  understanding that the trusted axioms ARE specifications and the
  values are still effectively duplicated.

## Toolchain probed

- Verus binary: `0.2026.05.03.8b81855` (prebuilt
  `verus-arm64-macos` from the GitHub releases).
- Rust nightly used by Verus: `1.95.0-aarch64-apple-darwin`.
- `vstd`: `0.0.0-2026-04-20-1748` from crates.io.
- ferrodec's stable toolchain pin: 1.84 (untouched throughout the
  experiment).

## Why the rollback is total

When the user's framing ("don't ship copy-pasted function bodies")
was applied honestly, the pilot couldn't ship even a partial
graduation: the `Status` flag values had to be defined twice, and
`pow10_verified` was a parallel implementation, not the production
body. The sibling-crate sync tests caught divergence at runtime but
didn't make the Verus proofs apply to ferrodec's actual code. So the
proofs are valuable as a marker that "Verus runs on this codebase",
but as a verification claim about ferrodec they require trust the
non-Verus reader can't easily audit.

Better to leave the existing four-stack verification (unit tests,
property tests, Kani, conformance vectors, fuzz) as the honest
surface, and revisit Verus when the tooling supports a real
graduation path.
