# Reference registry index

One line per entry; the entry file is the single home for every fact
(schema and ritual: [SCHEMA.md](SCHEMA.md), mechanism: ADR-0052).

- [cowlishaw-dectest](cowlishaw-dectest.md) — conformance — General Decimal Arithmetic Testcases suite 2.62, vendored with hash pins, with the coverage-gap statement
- [cowlishaw-gda-arith](cowlishaw-gda-arith.md) — spec — General Decimal Arithmetic Specification 1.70, the GDA semantic authority
- [decnumber](decnumber.md) — oracle — decNumber 3.68, the GDA reference implementation consulted as a behavioral model
- [ieee-754-2008](ieee-754-2008.md) — spec — the revision that brought decimal and the BID/DPD encodings into IEEE 754 (lineage)
- [ieee-754-2019](ieee-754-2019.md) — spec — IEEE Std 754-2019, the storage and operation authority for the fixed formats
- [arb-flint](arb-flint.md) — oracle — Arb certified ball enclosures (FLINT 3), the proof-tier corpus generator
- [astro-float](astro-float.md) — oracle — pure Rust faithful oracle for the transcendental property suites
- [mpdecimal](mpdecimal.md) — oracle — libmpdec via CPython decimal, the differential oracle
- [mpfr](mpfr.md) — oracle — MPFR via rug behind mpfr-gate, the independent corpus re-derivation
- [mpmath](mpmath.md) — oracle — adaptive precision breadth oracle and anchor band corpus generator
- [num-bigint](num-bigint.md) — oracle — exact integer oracle for the closed arithmetic operations
