---
slug: failure-comparator-masking
category: failure
citation: "ferrodec failure museum: the conformance comparator accepted any NaN and masked CLAMPED, hiding a whole drift family behind a green suite (fd-92w.1, fixed in the ADR-0035 train)."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - docs/decisions/0035-decimal128-parity-train.md
verification:
  - tests/conformance.rs
notes: "Closed; the fd-92w parity train hardened the comparator first and then fixed everything the hardening exposed, which is the correct order and the reason this entry exists."
---

# Failure: the conformance comparator was the weakest oracle

**What shipped.** The decTest comparator treated every NaN as equal
to every NaN (no sign check, no payload check) and the status mask
omitted CLAMPED. decTest pins NaN sign and payload literals
precisely because implementations drift there; the comparator never
looked, so `sub` flipping NaN signs, position-first instead of
sNaN-priority payload propagation, and dropped CLAMPED flags all
rode under a 0-fail badge across two formats.

**Why every guard missed it.** The comparator IS the guard; nothing
audits the auditor. The pass-count pins (ADR-0010) pin what the
comparator reports, so a comparator that cannot see a difference
pins the blindness in place. The independent oracles compare values,
not NaN payload conventions, so the drift family lived exactly in
the comparator's blind spot.

**The fix.** fd-92w.1 hardened the comparator first (NaN sign,
payload, CLAMPED in the mask), then the newly red cases were fixed
across the family (the ADR-0035 parity train), in that order, so
every later fix was made against an oracle that could see it.

**The lesson.** Strengthen the oracle before fixing what it reports;
a green suite is a statement about the comparator, not the code. The
2026-06-09 review institutionalized the question (every finding must
say why the existing stack missed it), and this registry's guard
exists under the same rule: the checker gets checked.
