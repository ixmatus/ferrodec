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
//! `no_std` but does require a target with 64-bit atomic
//! read-modify-write; `thumbv6m` (no atomic RMW at any width) cannot
//! enable the feature, and the size-delta CI lane does not.
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

use core::sync::atomic::{AtomicU64, Ordering};

static RUNG2_ENTRIES: AtomicU64 = AtomicU64::new(0);
static RUNG3_ENTRIES: AtomicU64 = AtomicU64::new(0);
static ADJUDICATIONS: AtomicU64 = AtomicU64::new(0);

/// Rung 1's boundary predicate reported "cannot decide": the caller
/// re-runs the kernel at rung 2. One increment per guarded delivery
/// that naturally escalates (the `force_escalate` bypass does not
/// count).
#[inline]
pub(crate) fn count_rung2_entry() {
    RUNG2_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// Rung 2's predicate reported "cannot decide" in an
/// `unbounded-ladder` build: the caller enters the dynamic Ziv rung.
#[inline]
#[cfg_attr(not(feature = "unbounded-ladder"), allow(dead_code))]
pub(crate) fn count_rung3_entry() {
    RUNG3_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

/// The ADR-0060 exact integer adjudicator decided a rung-2 boundary
/// ambiguity (the operands were inside the adjudicable range).
#[inline]
#[cfg_attr(not(feature = "exp-log"), allow(dead_code))]
pub(crate) fn count_adjudication() {
    ADJUDICATIONS.fetch_add(1, Ordering::Relaxed);
}

/// Total natural rung-2 entries since the last [`reset`].
#[must_use]
pub fn rung2_entries() -> u64 {
    RUNG2_ENTRIES.load(Ordering::Relaxed)
}

/// Total rung-3 (dynamic rung) entries since the last [`reset`].
/// Always zero without the `unbounded-ladder` feature.
#[must_use]
pub fn rung3_entries() -> u64 {
    RUNG3_ENTRIES.load(Ordering::Relaxed)
}

/// Total adjudicator decisions since the last [`reset`].
#[must_use]
pub fn adjudications() -> u64 {
    ADJUDICATIONS.load(Ordering::Relaxed)
}

/// Zero every counter. Call between corpus files when authoring or
/// checking exact per-file pins.
pub fn reset() {
    RUNG2_ENTRIES.store(0, Ordering::Relaxed);
    RUNG3_ENTRIES.store(0, Ordering::Relaxed);
    ADJUDICATIONS.store(0, Ordering::Relaxed);
}
