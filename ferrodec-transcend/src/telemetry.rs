//! Escalation-depth telemetry (fd-4zo.21, ADR-0059 S3): process-wide
//! counters for the ladder's escalation events, so tests can pin
//! EXACT rung-2 entry counts over the planted and sampled corpora —
//! the drift tripwire in both directions (a budget loosening stops a
//! planted row from escalating; a tightening raises the sampled
//! corpus counts; either moves an exact pin).
//!
//! ## Test-only, off by default
//!
//! The `telemetry` feature exists for the pinned-count tests and for
//! ad hoc escalation-rate measurement; it is not a production
//! observability surface and no shipped configuration enables it.
//! The counters are `core::sync::atomic::AtomicU64` with `Relaxed`
//! ordering (counts, not synchronization), which keeps the module
//! `no_std`. Atomic read-modify-write at 64 bits does not exist on
//! every supported target (`thumbv6m` has no atomic RMW at any width;
//! `thumbv8m.main` stops at 32), so on targets without it the feature
//! still compiles but the counters are no-ops and every reader returns
//! zero. The reason: a test-only instrument must never be what makes a
//! `--all-features` cross build fail to link, and no pinned-count test
//! runs on those targets (the pins run on the host, where the real
//! counters exist). A future target with 32-bit RMW that wants live
//! counts needs a third arm on each item below; none is planned.
//!
//! ## Counting discipline
//!
//! The counters are process-wide, so exact pins require SERIAL
//! replay: one `#[test]` per binary drives the corpus loop and calls
//! [`reset`] between files. Two tests reading the same counter
//! concurrently blend their counts; the pinned tests are structured
//! as a single test function for exactly that reason.
//!
//! Under `--cfg force_escalate` (and the other test-lane cfgs) the
//! rung-1 predicate is bypassed, so the natural-escalation counters
//! read zero there by design: the lanes answer a different question
//! (byte identity of the rungs), and the pinned counts run in the
//! default lane only.

#[cfg(target_has_atomic = "64")]
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_has_atomic = "64")]
static RUNG2_ENTRIES: AtomicU64 = AtomicU64::new(0);
#[cfg(target_has_atomic = "64")]
static RUNG3_ENTRIES: AtomicU64 = AtomicU64::new(0);
#[cfg(target_has_atomic = "64")]
static ADJUDICATIONS: AtomicU64 = AtomicU64::new(0);

/// Rung 1's boundary predicate reported "cannot decide": the caller
/// re-runs the kernel at rung 2. One increment per guarded delivery
/// that naturally escalates (the `force_escalate` bypass does not
/// count). A no-op on targets without 64-bit atomic RMW (module docs).
#[inline]
#[cfg_attr(
    not(any(feature = "exp-log", feature = "trig", feature = "trig-pi")),
    allow(dead_code)
)]
pub(crate) fn count_rung2_entry() {
    #[cfg(target_has_atomic = "64")]
    RUNG2_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// Rung 2's predicate reported "cannot decide" in an
/// `unbounded-ladder` build: the caller enters the dynamic Ziv rung.
#[inline]
#[cfg_attr(
    not(all(
        feature = "unbounded-ladder",
        any(feature = "exp-log", feature = "trig", feature = "trig-pi")
    )),
    allow(dead_code)
)]
pub(crate) fn count_rung3_entry() {
    #[cfg(target_has_atomic = "64")]
    RUNG3_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// The ADR-0060 exact integer adjudicator decided a rung-2 boundary
/// ambiguity (the operands were inside the adjudicable range).
#[inline]
#[cfg_attr(not(feature = "exp-log"), allow(dead_code))]
pub(crate) fn count_adjudication() {
    #[cfg(target_has_atomic = "64")]
    ADJUDICATIONS.fetch_add(1, Ordering::Relaxed);
}

/// Total natural rung-2 entries since the last [`reset`]. Always zero
/// on targets without 64-bit atomic RMW (module docs).
#[must_use]
pub fn rung2_entries() -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        RUNG2_ENTRIES.load(Ordering::Relaxed)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        0
    }
}

/// Total rung-3 (dynamic rung) entries since the last [`reset`].
/// Always zero without the `unbounded-ladder` feature, and on targets
/// without 64-bit atomic RMW (module docs).
#[must_use]
pub fn rung3_entries() -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        RUNG3_ENTRIES.load(Ordering::Relaxed)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        0
    }
}

/// Total adjudicator decisions since the last [`reset`]. Always zero
/// on targets without 64-bit atomic RMW (module docs).
#[must_use]
pub fn adjudications() -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        ADJUDICATIONS.load(Ordering::Relaxed)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        0
    }
}

/// Zero every counter. Call between corpus files when authoring or
/// checking exact per-file pins. A no-op on targets without 64-bit
/// atomic RMW (module docs).
pub fn reset() {
    #[cfg(target_has_atomic = "64")]
    {
        RUNG2_ENTRIES.store(0, Ordering::Relaxed);
        RUNG3_ENTRIES.store(0, Ordering::Relaxed);
        ADJUDICATIONS.store(0, Ordering::Relaxed);
    }
}
