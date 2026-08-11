//! Shared correctly rounded Extended precision transcendental kernel for
//! the ferrodec decimal family.
//!
//! The exponential / logarithmic / trigonometric / hyperbolic / power
//! kernels evaluate on an escalation ladder (ADR-0059, superseding
//! ADR-0032's fixed 50 digit posture for `Decimal64`/`Decimal128`;
//! ADR-0033 proved `Decimal32` exhaustively): a 50 digit `Extended`
//! rung whose delivery is guarded by a per function error budget
//! against every rounding boundary, a 110 digit `Extended2` rung the
//! guard escalates to, and, behind the `unbounded-ladder` feature, a
//! dynamic precision rung that doubles until the boundary is decided.
//! Exact results and nearest mode ties are classified from the inputs
//! before any kernel runs, and asymptotic grid huggers are decided by
//! side theorems (ADR-0051), so the ladder only ever rounds values a
//! finite bracket can decide.
//!
//! The claim is three tiered (ADR-0059 §The claim ladder):
//!
//! * **Tier 0, unconditional**: every result lies within the top
//!   rung's quantified error bracket of the true value — strictly
//!   stronger than a faithful (≤ 1 ULP) contract.
//! * **Tier 1, by construction**: correctly rounded, conditional on
//!   two auditable premises — the per function budgets are sound
//!   (itemized in `ladder.rs` rustdoc, padded tenfold, audited
//!   empirically over the historically falsifying bands) and the
//!   input side exact/tie classification is complete (per function
//!   number theoretic arguments, cited in each kernel's rustdoc).
//! * **Tier 2, model**: for default (two rung) builds the expected
//!   residual exception rate under the equidistribution model is
//!   ~10^-36 per call. Builds with `unbounded-ladder` have **no
//!   exception set**: a near boundary escalation widens until the
//!   rounding is decided instead of delivering from a fixed top rung.
//!
//! The standing empirical witnesses: the corpus tests
//! (`tests/transcend_vectors.rs` and the sibling mirrors), the 1 819
//! row S1 misround witness corpus replayed as a pinned gate, the
//! force escalate and force rung 3 byte identity differentials, and
//! MPFR cross confirmation behind the optional `mpfr-gate` feature
//! (ADR-0026).
//!
//! The kernel is generic over a [`DecimalFormat`] seam so all three
//! siblings ([`ferrodec`] at Decimal128, `ferrodec-decimal64`,
//! `ferrodec-decimal32`) share one verified implementation rather than
//! a per precision copy. The seam has exactly three generic boundaries:
//! decoding a datum into the kernel ([`DecimalFormat::to_extended_parts`]),
//! rounding the kernel result back out
//! ([`DecimalFormat::round_and_pack_finite`]), and the Newton seed for
//! reciprocal / square root ([`DecimalFormat::recip_seed`] /
//! [`DecimalFormat::sqrt_seed`]). Everything between those boundaries is
//! precision agnostic fixed width integer math.
//!
//! `no_std`, alloc-free by default (the only unconditional `alloc` use
//! is in test modules); the `unbounded-ladder` rung is the crate's one
//! allocating path and stays behind its opt-in feature, so default and
//! Cortex-M0+ builds are unchanged by its existence.
//!
//! [`ferrodec`]: https://crates.io/crates/ferrodec
//! [`tests/transcend_vectors.rs`]: https://github.com/ixmatus/ferrodec/tree/main/tests/transcend_vectors.rs

#![no_std]

// Tests read committed corpora (the S1 witness bands for the ladder
// budget audit) from the workspace tree; production stays no_std.
#[cfg(test)]
extern crate std;

// The unbounded rung allocates (its coefficients are heap-backed);
// nothing else in the crate does, so the dependency is feature-gated
// rather than crate-wide.
#[cfg(feature = "unbounded-ladder")]
extern crate alloc;

pub mod consts;
pub mod extended;
#[cfg(feature = "telemetry")]
pub mod telemetry;
// Rung 2 of the ADR-0059 escalation ladder. Crate-internal until M8
// wires the ladder; the kernels reach it only through the `ExtNum`
// seam.
mod extended2;
// The unbounded rung of the same ladder (M8b): the working type whose
// precision is chosen at run time, behind the `unbounded-ladder`
// feature because it is the crate's only allocating code. The Ziv
// driver that walks it lands in M8b step 5.
#[cfg(feature = "unbounded-ladder")]
mod extended_dyn;
mod format;
mod ladder;
#[cfg(test)]
mod mock_format;

// Exact-result detection for cbrt / rootn / pow (§7.5 INEXACT
// contract). Gated on `exp-log` (cbrt, rootn); `pow` implies
// `exp-log`, so every consumer sees it.
#[cfg(any(feature = "exp-log", feature = "trig-pi"))]
mod exact;

// The ADR-0060 exact integer adjudicator: the rung 2 ambiguous-path
// side decision for the algebraic §9.2 group. Same gate as `exact`,
// whose classifiers are its completeness premise.
#[cfg(feature = "exp-log")]
mod adjudicate;

// The pi-scaled family's exact classification (ADR-0061): the
// residue classes and finite tables, Niven-complete, no ties.
#[cfg(feature = "trig-pi")]
mod exact_pi;

#[cfg(feature = "trig")]
pub mod argred;
#[cfg(feature = "exp-log")]
pub mod cbrt;
// `compound(x, n) = (1 + x)^n` (ADR-0059 Track D D3). Gated on
// `exp-log` rather than `pow`: the kernel is `exp ∘ logp1`, so it needs
// the same modules `exp2` and the `logp1` family already pull in, and
// none of `pow`'s Newton-seeded surface.
#[cfg(feature = "exp-log")]
pub mod compound;
#[cfg(feature = "exp-log")]
pub mod exp;
#[cfg(feature = "hyperbolic")]
pub mod hyperbolic;
// The algebraic §9.2 group's first member (ADR-0060 Track D D3). It
// needs no series machinery, only `sqrt` and the exact classifier, and
// it rides `exp-log` for the same reason `cbrt` does: that is where
// this crate's input-side classification module lives.
#[cfg(feature = "exp-log")]
pub mod hypot;
// Two features reach this module. `trig` compiles the public `atan` /
// `asin` / `acos` / `atan2` kernels; `trig-pi` compiles only their
// extended-precision cores, which `inverse_trig_pi` scales by `1/π`
// (ADR-0061) so the pi-scaled family inherits one implementation of
// the series rather than carrying a second.
#[cfg(any(feature = "trig", feature = "trig-pi"))]
pub mod inverse_trig;
// The pi-scaled inverse family `asinPi` … `atan2Pi` (ADR-0061 Track D
// D4): `inverse_trig`'s cores scaled by `1/π`, under their own §9.2.1
// tables, exact classifiers, and ADR-0051 anchor arms.
#[cfg(feature = "trig-pi")]
pub mod inverse_trig_pi;
#[cfg(feature = "exp-log")]
pub mod ln;
#[cfg(feature = "pow")]
pub mod pow;
#[cfg(feature = "exp-log")]
pub mod rootn;
#[cfg(feature = "exp-log")]
pub mod rsqrt;
// IEEE 754-2019 §9.2 `powr`: `pow`'s rule-8 pipeline under the §9.2.1
// `powr` special-value table (ADR-0059 Track D D3). Shares the `pow`
// feature: the two operations differ only before the first
// approximation runs.
#[cfg(feature = "pow")]
pub mod powr;
#[cfg(feature = "trig")]
pub mod sincos;
// The IEEE 754-2019 §9.2 forward pi-scaled trio (ADR-0061 Track D D4).
// Standalone under `trig-pi`: the exact decimal reduction needs none of
// `trig`'s Payne-Hanek machinery.
#[cfg(feature = "trig-pi")]
pub mod sincospi;

pub use format::DecimalFormat;
