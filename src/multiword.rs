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

// `U512` was only used by the Payne-Hanek argument reduction, whose
// kernel moved into `ferrodec-transcend` (P0a.2 c7). It now imports
// `U512` straight from `ferrodec-multiword`, so the core no longer
// needs to re-export it.
