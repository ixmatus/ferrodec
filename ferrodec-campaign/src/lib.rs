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
pub mod prng;
pub mod sample;
pub mod sweep;

use ferrodec_multiword::U256;

/// Decimal string of a [`U256`] (test and report rendering).
#[must_use]
pub fn u256_to_decimal(mut v: U256) -> String {
    if v.is_zero() {
        return "0".into();
    }
    let mut digits = Vec::new();
    while !v.is_zero() {
        let (q, r) = v.div_rem10();
        digits.push(char::from(b'0' + u8::try_from(r).unwrap()));
        v = q;
    }
    digits.iter().rev().collect()
}

/// Approximate `f64` of a [`U256`] (histogram rendering only; every
/// contract comparison stays exact integer).
#[must_use]
pub fn u256_to_f64(mut v: U256) -> f64 {
    let mut digits = Vec::new();
    while !v.is_zero() {
        let (q, r) = v.div_rem10();
        digits.push(r);
        v = q;
    }
    digits
        .iter()
        .rev()
        .fold(0f64, |acc, &d| acc * 10.0 + f64::from(d))
}

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
