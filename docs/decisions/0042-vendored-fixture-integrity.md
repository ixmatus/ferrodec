# ADR-0042: Hash-pinned vendored-fixture integrity

- **Status**: accepted
- **Date**: 2026-06-03

## Context

The decTest suites are vendored from Mike Cowlishaw's upstream archive
(`dectest.zip`) and committed unmodified. Their provenance was recorded in prose
in each directory's README (source URL, version, license, an "unmodified"
attestation), and two of the four directories (`ferrodec-decimal32`,
`ferrodec-decimal64`) also recorded the upstream archive SHA-256 and a retrieved
date. But nothing was enforced by the build:

- The recorded archive SHA-256 was a refresh-time hand-verify note, never
  checked by `cargo test`.
- Two of the four directories (the root Decimal128 `dq*` vectors and the
  `ferrodec-decimal` general vectors) lacked even that note: no hash, no
  retrieved date.
- The ADR-0010 per-file pass-count pins guard *behavior* (a fixture change that
  moves a result fails the build), but a behavior-neutral byte drift, or a file
  swapped without updating provenance, went uncaught.

So the guarantee was "a human verified the archive once at vendoring time," not
"the committed bytes are still the ones we vetted."

## Decision

Pin the vendored fixtures by content hash and enforce it in the default test
run, and bring every directory's README up to the full provenance standard the
two stronger ones already set.

**Per-directory manifest.** Each vectors directory carries a committed
`SHA256SUMS` in the standard `shasum -a 256` format (`<hex>  <name>`), pinning
the SHA-256 of every `*.decTest` file it vendors. Regenerable with
`cd <dir> && shasum -a 256 *.decTest > SHA256SUMS`.

**Default-on enforcement.** `ferrodec_test_support::vendored::verify(dir)`
re-hashes the committed `*.decTest` files and asserts set-equality with the
manifest (every pinned file present, no unpinned file on disk) and a per-file
hash match. Each crate runs it from a default-on `tests/vendored_integrity.rs`,
so a silent byte drift, or a newly vendored file that was never attested, fails
the build. This is the content-hash companion to the ADR-0010 pass-count pins:
those guard behavior, this guards the bytes.

**README provenance.** Every vectors README records the upstream archive
provenance: source URL, archive size, archive SHA-256, retrieved date, suite
version, license, and the "unmodified" attestation, and points at the
`SHA256SUMS` manifest as the test-enforced per-file pin. The two lagging READMEs
are brought up to this standard; all four share the same archive (suite 2.62,
SHA-256 `b70a224cd52e82b7a8150aedac5efa2d0cb3941696fd829bdbe674f9f65c3926`,
791733 bytes), differing only in which files were extracted and when.

**Dependency.** The check uses `sha2` (RustCrypto), added to
`ferrodec-test-support`, which is `publish = false` and consumed only as a
dev-dependency, so `sha2` never reaches a shipped or embedded artifact.
`ferrodec-decimal` takes a dev-dependency on `ferrodec-test-support` for this
helper only; its decTest *runner* stays bespoke (ADR-0039), so the
minimal-published-graph intent there is unaffected.

## Consequences

The chain is now closed end to end: the committed bytes are test-enforced
against a per-file manifest, and the README attests they were extracted from a
named upstream archive with a recorded SHA-256, retrieved on a recorded date.
Two complementary guards stand over the fixtures: content-hash (provenance and
byte integrity) and pass-count (behavioral regression).

The model is non-adversarial: it detects accidental drift (an editor reflowing
line endings, a botched re-vendor, an unattested new file), not a coordinated
malicious edit of both the fixture and its manifest, which is out of scope for
any in-repo pin. The refresh ritual is: re-fetch the archive, verify its SHA-256
against the README, extract, regenerate `SHA256SUMS`, and update the retrieved
date.

The self-computed Arb/MPFR frozen transcendental corpus (`tests/vectors/transcend/`,
`.txt` plus `.prov`) is deliberately out of scope: it is not vendored from a
third party, so an upstream archive hash does not apply. Its provenance is the
per-value `.prov` companion that records how each value was certified (ADR-0026).
A content-hash guard over the generated corpus, to catch accidental corruption,
is a possible future extension.

## Related

- ADR-0010 (record-then-pin per-file pass counts, the behavioral companion).
- ADR-0026 (the frozen corpus and its `.prov` provenance model).
- ADR-0039 (the `ferrodec-decimal` decTest runner whose vectors this pins).
- Issues: `fd-vendored-integrity`.
