//! Content-hash integrity of the *generated* corpora (fd-aqs.10): the
//! Arb/FLINT frozen transcendental vectors (`tests/vectors/transcend/`
//! plus its `anchor_bands/` and `exhaustive/` subdirectories) and the
//! rounding-kernel cases (`tests/vectors/round_half_even/`). Unlike the
//! vendored decTest fixtures (`vendored_integrity.rs`, ADR-0042), these
//! are produced by committed generators rather than an upstream
//! archive; the byte-drift guard is identical, so a silent regeneration
//! or an unattested new corpus file fails the build. This is the
//! byte-level companion to the per-(func,mode) bucket pins in
//! `ferrodec_test_support::frozen` (which guard the loader's
//! interpretation). Each directory carries its own `SHA256SUMS`
//! (subdirectories are not descended); regenerate one with
//! `(cd <dir> && shasum -a 256 *.txt > SHA256SUMS)`.

#[test]
fn generated_corpus_integrity() {
    for dir in [
        "tests/vectors/transcend",
        "tests/vectors/transcend/anchor_bands",
        "tests/vectors/transcend/exhaustive",
        "tests/vectors/transcend/external",
        "tests/vectors/transcend/planted",
        "tests/vectors/round_half_even",
    ] {
        ferrodec_test_support::vendored::verify_txt(dir);
    }
}
