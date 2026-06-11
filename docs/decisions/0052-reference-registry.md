# ADR-0052: Reference registry under docs/references/ with a default-on schema guard

- **Status**: accepted
- **Date**: 2026-06-11

## Context

ferrodec implements two external standards and verifies itself against a stack
of third party oracles, vector suites, papers, and books. The citations for all
of that are real but scattered: license and provenance blocks live in four
`tests/vectors/README.md` files, paper citations live in code comments and
ADRs, oracle roles live in `docs/testing.md`, and the reasons a source was
chosen over its alternatives live mostly in ADR prose. Nothing collects them,
nothing records which external URLs are load bearing, and nothing notices when
one of those URLs dies. Two of the most load bearing hosts are personal sites
(speleotrove.com for the General Decimal Arithmetic specification and test
vectors, bytereef.org for mpdecimal); the web forgets such sites on its own
schedule.

The project CLAUDE.md already states the accretion convention (one registry
entry per external source, updated in the same slice that cites it). This ADR
records the concrete mechanism, the initial mining sweep, and the guard that
keeps the registry from rotting into prose.

A concrete instance of the problem surfaced during the initial sweep: the
Payne and Hanek citation in `ferrodec-transcend/src/argred.rs` carried a DOI
that no longer resolves (`10.1145/29380.29384`); the correct DOI is
`10.1145/1057600.1057602` (Crossref, verified 2026-06-11). A registry with a
recorded canonical identifier per source is the durable home for that kind of
fact; a code comment is not.

## Decision

**One markdown file per source under `docs/references/`.** Each entry carries
YAML style frontmatter with a fixed key set: `slug` (equal to the filename
stem), `category`, `citation`, `canonical` (URL or document number), `doi`,
`archived` (Wayback URL saved at citation time, or `none (reason)`),
`archive-date`, `retrieved`, `sha256` (binaries only), `license`,
`vendor-status`, `rot-risk`, `provenance`, `consumers` (repo paths),
`verification` (tests or vectors derived from the source), and `notes` (why
this source, alternatives considered). The body is short prose. `SCHEMA.md` in
the same directory is the normative field reference; `INDEX.md` carries one
line per entry and never carries content.

**Nine categories, eight from the program taxonomy.** `spec`, `conformance`,
`oracle`, `algorithm`, and `history` entries describe external sources and
require the full field set. `registry`, `glossary`, `verification`, and
`failure` entries are internal documents that reuse the same frontmatter with
external fields relaxed to `n/a`; they keep `consumers` and `verification`
because the linkage is the point. Conformance entries additionally carry a
mandatory `## Coverage gaps` body section, because the gaps feed the README
disclosure's named failure mode ("boundary cases the decTest suite did not
cover").

**License gated vendoring.** Every load bearing URL is saved to the Wayback
Machine at citation time and the archived URL is recorded. A local copy is
vendored under `docs/references/vendor/<slug>/` only when the source's own
license text, read and quoted in the entry, clearly permits redistribution.
The initial sweep vendored nothing: the speleotrove specification pages carry
IBM copyright reproduced with Cowlishaw's permission (not ours to extend), the
Brent and Zimmermann free PDF is restricted to non commercial copying, and the
paywalled IEEE standards are pointer only by rule. An empty `vendor/` is the
gate working, not a gap. The decTest fixtures themselves remain vendored at
`tests/vectors/` under the longstanding ADR-0042 regime; the registry entry
points there rather than duplicating the bytes.

**Default-on guard.** A new `references` module in `ferrodec-test-support`
(the ADR-0042 home) parses every entry with a small hand rolled parser and a
default-on `tests/references_integrity.rs` asserts: required keys present per
category, enumerated fields valid, `slug` equals the filename stem, `INDEX.md`
and the entry set agree in both directions, every `consumers` and
`verification` path exists in the workspace, any `vendor/` subdirectory
verifies against its `SHA256SUMS` and belongs to an entry, and the
generator pinned registry documents byte match blocks rendered from the
`ferrodec-ieee` enums (an exhaustive match with no wildcard arm makes a new
enum variant a compile error before it is a stale document). The frontmatter
is deliberately a flat constrained subset of YAML (scalar values, one list
level); the parser enforces the subset, so no YAML dependency is taken.
`serde_yaml` was considered and rejected: it is archived upstream, and the
eager dependency posture argues against buying a full YAML parser for a
format this small.

**No network in CI.** The guard reads committed files only. URL liveness is a
manual concern; the `archived` field is the hedge that makes liveness loss
recoverable.

## Consequences

Every external claim the crate family stands on now has one durable, machine
checked home: where it came from, under what license, where the archived copy
lives, and which code and tests consume it. A future maintainer (or a
downstream synthesis project, which copies these entries rather than linking
them) can reconstruct the provenance without reading fifty ADRs.

The guard adds a maintenance tax by design: renaming a cited file breaks the
`consumers` check until the entry is updated, exactly as a dead link should.
The registry records facts as of their retrieval dates; it does not promise
the live web still matches, only that the archived copies exist.

The accretion ritual (CLAUDE.md) keeps the registry current: any future slice
that cites a new source adds its entry in the same slice. Failure museum
entries are written at fix time. The initial sweep is the bootstrap, not the
steady state.

## Related

- ADR-0042 (hash pinned vendored fixtures; the guard pattern this extends).
- ADR-0010 (per file pass count pins; the behavioral companion).
- ADR-0026 / ADR-0032 / ADR-0050 (the oracle and corpus decisions whose
  sources the registry now catalogs).
- ADR-0005 (the half_down / 05up rejection recorded in the rounding mode
  registry document).
- Issues: `fd-oxj` and children.
