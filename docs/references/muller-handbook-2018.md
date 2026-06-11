---
slug: muller-handbook-2018
category: algorithm
citation: "Muller, J.-M., Brunie, N., de Dinechin, F., Jeannerod, C.-P., Joldes, M., Lefèvre, V., Melquiond, G., Revol, N., Torres, S. Handbook of Floating-Point Arithmetic, 2nd edition. Birkhäuser, 2018."
canonical: "https://doi.org/10.1007/978-3-319-76526-6"
doi: "10.1007/978-3-319-76526-6"
archived: "none (paywalled Springer book; no archivable free copy)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "Copyright Springer/Birkhäuser. No paper copy owned; pointer only."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: secondary
consumers:
  - README.md
  - docs/decisions/0032-correctly-rounded-transcendentals.md
  - docs/decisions/0033-worst-case-margin-completeness.md
  - docs/decisions/0034-empirical-coverage-extension.md
  - docs/decisions/0050-anchor-band-reformulations.md
verification:
  - tests/transcend_vectors.rs
notes: "The proof backbone under ADR-0032: the wider fixed working precision technique delivers correct rounding when the working margin exceeds the worst case hardness, and this book carries the proofs that the technique delivers what it claims. Chapter 11's elementary function reformulations near fixed points ground the ADR-0050 anchor band forms. The README reading list names it as the recommended companion to the standard."
---

# Handbook of Floating-Point Arithmetic (Muller et al., 2nd ed.)

The Handbook is the secondary literature the correctly rounded
contract leans on where the paywalled standard cannot be quoted: the
fixed precision envelope argument (ADR-0032's mechanism), the
hardness-of-rounding framing that connects the Arb worst case search
to a proof, and the near-anchor reformulation patterns ADR-0050
applied. Pointer only: no paper copy is owned and the publisher
copy is paywalled, but Springer book DOIs are about as rot-resistant
as pointers get.
