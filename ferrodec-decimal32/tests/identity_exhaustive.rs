//! ADR-0034 exhaustive identity-residue sweep for Decimal32.
//!
//! Closes the small exhaustive gap left by the Kani harnesses. The
//! arithmetic and comparison Kani proofs use bounded operand shims
//! (`*_special_only_for_kani`, ADR-0015 / ADR-0016) so CBMC need not
//! reason about the alignment, rounding, and string paths. That leaves
//! a residue of identities that are *not* proven for every input and
//! are otherwise only sampled (by proptest and the fuzz targets). This
//! sweep walks the entire 2^32 Decimal32 encoding space and asserts
//! that residue directly. Where Kani already discharges an identity
//! totally (the canonical-predicate projection and its idempotence, the
//! BID/DPD encode and decode round trips, the same-cohort total order
//! antisymmetry, the special-case-dispatch arithmetic identities), it
//! is deliberately *not* re-checked here: a symbolic proof holds for
//! every input, so an exhaustive concrete loop would be strictly weaker
//! redundancy.
//!
//! Four residue identities, per ADR-0034:
//!
//! 1. Total order reflexivity: `total_cmp(x, x) == Equal` for *every*
//!    bit pattern (every cohort, every NaN payload, canonical or not).
//!    Kani's total-order domain is same-cohort same-sign finite-finite;
//!    this covers the whole encoding space.
//! 2. String round trip: for every canonical finite `x`,
//!    `parse_str(x.to_string())` recovers `x` bit for bit. This is the
//!    Display-then-parse path, distinct from the bit-level encode/decode
//!    round trip Kani proves; today it is only sampled, by the
//!    Decimal128 `parse` fuzz target.
//! 3. Successor inverse: `next_up(next_down(x))` and
//!    `next_down(next_up(x))` recover the value of `x` (away from the
//!    finite extremes, where one direction saturates to an infinity).
//! 4. Additive zero identity: `x + 0` preserves the value of `x`,
//!    exercising the general add path for every canonical finite `x`
//!    (the GDA preferred-exponent cohort behaviour itself is covered
//!    exhaustively by the decTest conformance vectors; this sweep is the
//!    value-preservation witness across the whole input set).
//!
//! This is an on-demand sweep, not a CI gate: 2^32 patterns with a
//! string round trip on the canonical-finite subset run in minutes in
//! release, far too slow for a debug `cargo test`. It is `#[ignore]`d so
//! it still compiles (and is clippy-checked) in CI without running. Run
//! it explicitly in release:
//!
//!     cargo test -p ferrodec-decimal32 --features fmt \
//!         --test identity_exhaustive --release -- --ignored --nocapture

#![cfg(feature = "fmt")]

use core::cmp::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as Memord};
use std::sync::Mutex;

use ferrodec_decimal32::{Decimal32, RoundingMode};

const NE: RoundingMode = RoundingMode::NearestEven;

/// Check the residue identities for one encoding. Returns `Err` with a
/// diagnostic on the first violation; the canonical-finite identities
/// are skipped for non-canonical or non-finite encodings (only the
/// total-order reflexivity check applies to every pattern).
fn check(bits: u32) -> Result<(), String> {
    let x = Decimal32::from_bits(bits);

    // (1) Total order reflexivity — every bit pattern.
    if x.total_cmp(x) != Ordering::Equal {
        return Err(format!("total_cmp(x, x) != Equal for bits={bits:#010x}"));
    }

    if !(x.is_canonical() && x.is_finite()) {
        return Ok(());
    }

    // (2) String round trip — bit-exact for canonical finite (GDA toSci
    // preserves the cohort, parse_str recovers it).
    let s = x.to_string();
    let y = Decimal32::parse_str(&s, NE)
        .map_err(|e| format!("parse_str({s:?}) failed: {e:?} (bits={bits:#010x})"))?
        .0;
    if y.to_bits() != x.to_bits() {
        return Err(format!(
            "round trip: bits={bits:#010x} -> {s:?} -> {:#010x}",
            y.to_bits()
        ));
    }

    // (4) Additive zero identity — value preserved through the general
    // add path.
    let sum = x.add(Decimal32::ZERO, NE).0;
    if sum.partial_cmp(x).0 != Some(Ordering::Equal) {
        return Err(format!(
            "x + 0 not value-equal to x for bits={bits:#010x} ({s:?})"
        ));
    }

    // (3) Successor inverse — value recovered, guarding the extremes
    // where one direction saturates to an infinity.
    let dn = x.next_down().0;
    if dn.is_finite() && dn.next_up().0.partial_cmp(x).0 != Some(Ordering::Equal) {
        return Err(format!(
            "next_up(next_down(x)) lost the value for bits={bits:#010x} ({s:?})"
        ));
    }
    let up = x.next_up().0;
    if up.is_finite() && up.next_down().0.partial_cmp(x).0 != Some(Ordering::Equal) {
        return Err(format!(
            "next_down(next_up(x)) lost the value for bits={bits:#010x} ({s:?})"
        ));
    }

    Ok(())
}

#[test]
#[ignore = "exhaustive 2^32 sweep; run in release: \
            cargo test -p ferrodec-decimal32 --features fmt \
            --test identity_exhaustive --release -- --ignored --nocapture"]
fn identity_residue_exhaustive() {
    const TOTAL: u64 = 1u64 << 32;

    let threads = match std::thread::available_parallelism() {
        Ok(n) => n.get(),
        Err(_) => 4,
    };
    let chunk = TOTAL / threads as u64 + 1;

    let failed = AtomicBool::new(false);
    let first_err: Mutex<Option<String>> = Mutex::new(None);
    let canonical_finite = AtomicU64::new(0);

    std::thread::scope(|scope| {
        for t in 0..threads {
            let lo = t as u64 * chunk;
            let hi = ((t as u64 + 1) * chunk).min(TOTAL);
            let failed = &failed;
            let first_err = &first_err;
            let canonical_finite = &canonical_finite;
            scope.spawn(move || {
                let mut local_cf: u64 = 0;
                for b in lo..hi {
                    // Cheap relaxed poll so a violation in any thread
                    // stops the whole sweep promptly.
                    if b.trailing_zeros() >= 20 && failed.load(Memord::Relaxed) {
                        break;
                    }
                    let x = Decimal32::from_bits(b as u32);
                    if x.is_canonical() && x.is_finite() {
                        local_cf += 1;
                    }
                    if let Err(msg) = check(b as u32) {
                        let mut slot = first_err.lock().unwrap();
                        if slot.is_none() {
                            *slot = Some(msg);
                        }
                        failed.store(true, Memord::Relaxed);
                        break;
                    }
                }
                canonical_finite.fetch_add(local_cf, Memord::Relaxed);
            });
        }
    });

    if let Some(msg) = first_err.into_inner().unwrap() {
        panic!("ADR-0034 identity residue violation: {msg}");
    }

    eprintln!(
        "ADR-0034 identity residue exhaustive sweep (Decimal32): all \
         2^32 = {TOTAL} encodings pass total_cmp reflexivity; the \
         {} canonical finite values additionally pass the string round \
         trip, the x+0 value identity, and the next_up/next_down \
         successor inverse. Zero violations.",
        canonical_finite.load(Memord::Relaxed)
    );
}
