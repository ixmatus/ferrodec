# Registry entry schema (normative)

Every file in this directory except `SCHEMA.md` and `INDEX.md` is a registry
entry: one external source, or one internal registry document, per file. The
frontmatter below is the normative key set; `references_integrity.rs` in
`ferrodec-test-support` enforces it on every default test run (ADR-0052).

The frontmatter is a deliberately constrained subset of YAML: `---` fences,
scalar `key: value` lines, and one level of `- item` lists. Nothing fancier
parses, by design.

## Fields

```yaml
---
slug: cowlishaw-dectest    # must equal the filename stem
category: conformance      # one of: spec | conformance | oracle | algorithm |
                           #   registry | glossary | verification | history | failure
citation: "Author(s). Title. Venue or publisher, year, edition or revision."
canonical: "https://..."   # canonical URL, or a document number for paywalled
                           #   standards ("IEEE Std 754-2019")
doi: "10.1145/..."         # or "none"
archived: "https://web.archive.org/web/<timestamp>/..."  # or "none (reason)"
archive-date: "2026-06-11" # date the archive capture was made, or "n/a"
retrieved: "2026-05-03"    # date the source was last fetched and read, or "n/a"
sha256: "b70a224c..."      # for fetched binaries (zip, pdf), else "n/a"
license: "verbatim or tightly summarized license terms"
vendor-status: "pointer-only"
                           # one of: vendored-at-path <path> | pointer-only |
                           #   legally-cannot | paper-copy-owned | n/a
rot-risk: academic-personal
                           # one of: died-once | single-maintainer | community-run |
                           #   academic-personal | stable-publisher | ephemeral | n/a
provenance: primary        # primary | secondary
consumers:                 # workspace-relative paths that cite or depend on it;
  - tests/conformance.rs   #   clause numbers belong in the body, not here
verification:              # tests or vector sets derived from this source
  - tests/vendored_integrity.rs
notes: "Why this source; alternatives considered and why not."
---
```

The body is short prose: what the source is, what it grounds, and anything a
future maintainer needs that the fields cannot carry (clause numbers, edition
quirks, the story behind the rot class). Entries are self contained because
downstream synthesis projects copy them rather than link them.

## Per category requirements

External source categories (`spec`, `conformance`, `oracle`, `algorithm`,
`history`) require every field. `sha256` may be `n/a` when nothing binary was
fetched; `doi` may be `none`; `archived` may be `none (reason)` only with a
stated reason (for example a paywalled standard with no archivable free copy).

Internal categories (`registry`, `glossary`, `verification`, `failure`)
describe documents this repository owns. They set `canonical`, `doi`,
`archived`, `archive-date`, `retrieved`, and `sha256` to `n/a`; `license` to
`repo (MIT OR Apache-2.0)`; `rot-risk` and `vendor-status` to `n/a`; and
`provenance` to `primary`. `consumers` and `verification` stay required: the
linkage is the point of the entry.

`conformance` entries must carry a `## Coverage gaps` body section naming what
the vector set does not exercise. The gaps feed the README disclosure's named
failure mode and must never contradict it.

## INDEX.md

One line per entry, format enforced by the guard:

```
- [<slug>](<slug>.md) — <category> — <one line title>
```

INDEX.md never carries content; the entry is the single home for every fact.

## vendor/

`vendor/<slug>/` holds a local copy of a source only when the entry's
`license` field quotes terms that clearly permit redistribution, and the
entry's `vendor-status` is `vendored-at-path docs/references/vendor/<slug>/`.
Every vendored directory carries a `SHA256SUMS` manifest covering every file,
verified by the guard. An empty `vendor/` is the license gate holding, not a
gap (ADR-0052): as of the initial sweep, nothing met the bar.

## Ritual

When a slice cites or relies on a new external source: save the URL to the
Wayback Machine, then add the entry with the archived URL, in the same slice.
When a shipped bug is fixed: write its `failure-<slug>.md` post mortem at fix
time, in the fixing slice. The registry accretes; it is never batch rebuilt.
