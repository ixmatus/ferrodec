# W4-D4 (reader D4): the reference registry, licensing and provenance

Scripts behind the entry audit: `rev.py` (reverse linkage: files that
cite a registry source against the entry's consumer list) and `cons.py`
(forward consumer paths resolved), plus the reader's working lists.
The audit table, the entry drafts in the accretion schema and the
README disclosure diff proposal are in the report itself
(`readers/D4.md`). The coordinator confirmed by command: `cargo package
--list` shows no LICENSE file for ferrodec-ieee, -multiword, -transcend
and -decimal64 (D4-6), the root package carries the docs/archive tree,
the §7.4 CLAMPED attribution occurs on 64 lines in 46 files outside
docs/archive (D4-1), and the duplicate entry pairs share one canonical
source (D4-3).
