//! Kani verification harnesses.
//!
//! Compiled only under `cfg(kani)`. To run:
//!
//! ```sh
//! cargo kani --enable-stable
//! ```
//!
//! Each harness function is annotated with `#[kani::proof]` and uses
//! `kani::any()` to introduce symbolic inputs, with `kani::assume()` to
//! constrain to non-pathological domains where useful. We aim for the full
//! proof set to terminate within tens of minutes on a developer laptop.

mod addsub;
mod canonical;
mod classify;
mod cmp;
mod decimal;
mod div;
mod encode;
mod mul;
mod quantum;
mod rem;
mod sqrt;
