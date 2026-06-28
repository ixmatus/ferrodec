# Work ledger: ferrodec-decimal 2.0.0 metrology bridge

Temporary capture of work done on branch `fd-decimal-1.1.0-metrology-bridge`,
because the beads database lives on the Studio machine and is not present here.
**On return to the Studio: translate each item below into the close-out actions,
then delete this file** (its durable content is already promoted to ADR-0055).

Plan: `~/.claude/plans/async-swinging-nygaard.md` (approved, but as an additive
1.1; see the pivot note).
ADR: `docs/decisions/0055-decimal-ordering-and-float-string-ergonomics.md`
(supersedes ADR-0045).
Version: `ferrodec-decimal` 1.0.1 → **2.0.0** (breaking; see item 1).

## The pivot (file a bead, link discovered-from the Ord item)

The plan assumed `Ord` was a clean additive `1.1`. During implementation the
compiler showed it is **not additive**: `Ord`'s provided `max(self, other)` /
`min(self, other)` shadow the frozen GDA inherent `max(&self, other, ctx)` /
`min(...)` at every value receiver (method resolution picks `Ord::max` at the
by-value step), breaking all `d.max(other, ctx)` calls. So adding `Ord` is a
breaking change. User chose (AskUserQuestion) to take the major bump and rename
the GDA ops to `maxnum` / `minnum` (= IEEE 754-2019 `maximumNumber` /
`minimumNumber`). Hence 2.0 + supersede ADR-0045.

## Items completed (one per bead)

1. **`Ord` / `PartialOrd` for `Decimal`** — `src/compare.rs`. Hand-written, both
   delegating to private `total_cmp` (IEEE totalOrder), gateless, not derived,
   lawful vs structural `Eq`. Unit tests: `ord_agrees_with_compare_total`,
   `ord_is_lawful_against_eq`, `ord_sorts_and_keys_a_btree`,
   `ord_totally_orders_nans_without_panic`.

2. **Rename GDA `max`/`min` → `maxnum`/`minnum`** (the breaking change) —
   `src/compare.rs` (defs + docs), call sites in `tests/conformance.rs`,
   `tests/differential.rs`, and the compare.rs unit tests. decTest string keys
   `"max"`/`"min"` unchanged (spec op names). `max_magnitude`/`min_magnitude`
   untouched (no Ord collision).

3. **`FromStr` for `Decimal`** — `src/convert/parse.rs`, delegates to
   `parse_str`. Tests: `from_str_matches_parse_str_and_preserves_cohort`,
   `from_str_propagates_the_same_error`.

4. **`Decimal::to_f64(&self, RoundingMode) -> (f64, Status)`** — new
   `src/to_float.rs` under `binary-float`, mirrors `Decimal128::to_f64`
   (pass-through specials, exact-string single round-to-nearest-even,
   rm informs only over/underflow edges). 8 in-module unit tests + property test
   `tests/property_to_f64.rs` (f64/f32 bit-exact round-trip).

5. **Feature graph + re-export** — `binary-float` now pulls `fmt` (Display needed
   for `to_f64`); `RoundingMode` re-export widened to
   `any(feature="interop", feature="binary-float")`; `mod to_float` in `lib.rs`.

6. **Docs + version** — ADR-0055 (supersedes ADR-0045; ADR-0045 marked
   superseded); `Cargo.toml` 1.0.1 → 2.0.0; `lib.rs` module docstring rewritten
   for the 2.0 surface; `README.md` design-principle #4 rewritten (no-ordering →
   totalOrder) + snake-case note records the `maxnum`/`minnum` exception. No new
   `docs/references/` entry (no new external source).

## Relay note for metrology (not a code change here)

metrology's mental "DecBig" is ferrodec's `Decimal` (not `ferrodec_multiword::DecBig`,
which is the unsigned base-10⁹ coefficient bignum). `const_value_big(key)` should
return `Decimal`. metrology depends on `ferrodec-decimal` **2.0** with
`["interop","binary-float"]`. No sibling workspace crate version-depends on
ferrodec-decimal, so the major bump breaks nothing inside the workspace.

## Verification — ALL GREEN (toolchain: stable 1.96.0 via nix rustup)

- [x] `cargo test -p ferrodec-decimal` — pass
- [x] `cargo test -p ferrodec-decimal --features interop,binary-float` — 134 lib + conformance + property pass
- [x] `cargo test -p ferrodec-decimal --all-features` — pass (incl. differential)
- [x] `cargo fmt --all -- --check` — clean (after applying fmt)
- [x] `cargo clippy -p ferrodec-decimal --all-features -- -D warnings` — clean
- [x] `cargo build -p ferrodec --target thumbv6m-none-eabi --no-default-features --features fmt` — embedded floor unaffected

## Not yet done

- Commit (unsigned, on branch) — pending.
- Signed merge to main — **YubiKey boundary, prompt Parnell first; do not run.**
