//! Content-hash integrity of the vendored Decimal128 `dq*.decTest` vectors
//! (ADR-0042). Every `tests/vectors/*.decTest` must match its pinned SHA-256 in
//! `tests/vectors/SHA256SUMS`, and no unpinned file may appear. This guards the
//! committed bytes against silent drift, the companion to the ADR-0010 per-file
//! pass-count pins that guard behavior. See `tests/vectors/README.md` for the
//! upstream archive provenance.

#[test]
fn vendored_dectest_integrity() {
    ferrodec_test_support::vendored::verify("tests/vectors");
}

/// The transcendental replay fixtures (fd-4zo.8): byte-identical
/// copies of the GDA crate's vendored `exp` / `ln` / `log10` /
/// `power` files, attested by their own manifest (the directory's
/// README carries the provenance chain).
#[test]
fn vendored_gda_transcend_integrity() {
    ferrodec_test_support::vendored::verify("tests/vectors/gda-transcend");
}
