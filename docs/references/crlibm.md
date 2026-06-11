---
slug: crlibm
category: algorithm
citation: "Daramy-Loirat, C., Defour, D., de Dinechin, F., Gallet, M., Gast, N., Lauter, C., Muller, J.-M. CRlibm: a library of correctly rounded elementary functions in double-precision. ENS Lyon / LIP, 2006."
canonical: "https://github.com/taschini/crlibm"
doi: none
archived: "https://web.archive.org/web/20260309194505/https://github.com/taschini/crlibm"
archive-date: "2026-03-09"
retrieved: "2026-06-11"
sha256: n/a
license: "LGPL (the library); the documentation is distributed with it. Pointer and archive of the surviving mirror."
vendor-status: pointer-only
rot-risk: died-once
provenance: secondary
consumers:
  - docs/decisions/0032-correctly-rounded-transcendentals.md
verification:
  - tests/transcend_vectors.rs
notes: "Rejected alternative, recorded against relitigation: CRlibm proved correctly rounded double precision libm feasible via per-function pre-computed hard case tables, but the per-function table and code size cost is wrong for an embedded decimal target, and the technique does not transfer to three decimal formats without redoing the worst case campaigns anyway (ADR-0032). Rot class died-once is literal: the original gforge.inria.fr home is gone; the GitHub mirror archived here is the surviving copy. Its successor project CORE-MATH (core-math.gitlabpages.inria.fr) continues the program for binary formats."
---

# CRlibm (rejected for the fixed formats; host died once)

CRlibm is both prior art and a rot lesson. As prior art, it is the
table-driven road to correct rounding ADR-0032 declined on code size
grounds. As a rot lesson, it is the registry's only died-once entry:
the INRIA gforge that hosted it was decommissioned, and the project
survives through mirrors. That history is why the registry archives
personal and institutional project pages at citation time instead of
trusting them to persist.
