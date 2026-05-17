//! Internal re-export of the [`ferrodec-multiword`] foundation crate.
//!
//! The fixed-width 256/384/512-bit integer primitives were extracted
//! into their own crate so the transcendental kernel
//! (`ferrodec-transcend`) and the sibling decimal crates can share
//! them with the core arithmetic here. This shim re-exports them under
//! the original `crate::multiword::…` paths so the verified arithmetic
//! kernels (`ops/{addsub,mul,div,fma,rem,sqrt,quantum,round}`,
//! `convert/{int,parse}`) are untouched by the move — the extraction
//! is behaviour-neutral by construction and proven so by the full
//! Kani / property / conformance suite.
//!
//! [`ferrodec-multiword`]: ferrodec_multiword

pub(crate) use ferrodec_multiword::{u256, U256, U384};

// `U512` is only used by the Payne-Hanek argument reduction in
// `src/math/argred.rs`, which lives under the `trig` feature; keep the
// re-export gated identically so non-`trig` builds see the same
// surface as before the extraction.
#[cfg(feature = "trig")]
pub(crate) use ferrodec_multiword::U512;
