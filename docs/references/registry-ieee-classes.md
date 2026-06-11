---
slug: registry-ieee-classes
category: registry
citation: "ferrodec registry: the ten IEEE 754-2019 §5.7.2 classes, generated from ferrodec_ieee::IeeeClass."
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
  - ferrodec-ieee/src/classify.rs
verification:
  - ferrodec-test-support/tests/references_integrity.rs
notes: "The block between the GENERATED markers is rendered by the pin test from IeeeClass; the exhaustive match makes an eleventh class a compile error before it can be a stale document. Edit the block only by pasting the test's expected output."
---

# Registry: IEEE 754-2019 §5.7.2 classes

Every decimal datum occupies exactly one of the ten standard classes;
each format exposes `ieee_class` returning this enum, and the class
operation is quiet even on signaling NaN (per the standard).

<!-- BEGIN GENERATED: ieee-classes -->
- `SignalingNaN`: signaling NaN; most operations consume it and raise INVALID.
- `QuietNaN`: quiet NaN; propagates without INVALID.
- `NegativeInfinity`: negative infinity.
- `NegativeNormal`: negative finite at or above the minimum normal magnitude.
- `NegativeSubnormal`: negative finite strictly between zero and the minimum normal magnitude.
- `NegativeZero`: negative zero.
- `PositiveZero`: positive zero.
- `PositiveSubnormal`: positive finite strictly between zero and the minimum normal magnitude.
- `PositiveNormal`: positive finite at or above the minimum normal magnitude.
- `PositiveInfinity`: positive infinity.
<!-- END GENERATED: ieee-classes -->

NaN classes carry no sign by IEEE convention (the sign bit is
observable through `is_sign_negative`, but does not split the NaN
classes). The coarser five way `core::num::FpCategory` view exists as
`classify` on each format, and the decode level `IeeeDecodedClass`
(sign, biased exponent, coefficient or payload) is a separate,
internal shape: that one is what the kernels consume, this one is the
standard's observation surface.
