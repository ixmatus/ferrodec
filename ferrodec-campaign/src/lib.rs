//! Offline verification campaign driver for the correctly rounded
//! decimal128 lane (ADR-0059).
//!
//! The S1 falsification probe and the S2 deep margin campaigns run the
//! production `ferrodec-transcend` kernel and measure how close each 50
//! digit `Extended` intermediate lands to a format rounding boundary.
//! Only near boundary survivors go to the offline Arb certifier
//! (`tools/campaign_certify.py`), which is what makes depth affordable:
//! the kernel is its own cheap filter.
//!
//! This crate is workspace internal (`publish = false`) and host side
//! (`std`). It never ships, and it deliberately makes zero changes to
//! production code: `Extended`'s fields and the `*_extended` kernel
//! entry points are already public.
//!
//! The margin definitions are pinned to the corpus generator's
//! (`tools/gen_transcend_vectors.py::{tie_margin, directed_margin}`)
//! by the committed lockstep table
//! `tests/vectors/margin_lockstep.txt`; see [`margin`].

pub mod margin;

use ferrodec_multiword::U256;

/// Parse a non negative decimal integer string (at most 76 digits)
/// into a [`U256`]. Returns `None` on an empty string, a non digit
/// character, or a value that would not fit.
#[must_use]
pub fn u256_from_decimal(s: &str) -> Option<U256> {
    if s.is_empty() || s.len() > 76 {
        return None;
    }
    let mut acc = U256::ZERO;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        acc = acc.mul10().add(U256::from_u128(u128::from(b - b'0')));
    }
    Some(acc)
}
