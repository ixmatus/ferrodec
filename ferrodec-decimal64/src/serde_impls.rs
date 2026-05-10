//! Serde [`Serialize`] / [`Deserialize`] for [`Decimal64`] (gated on
//! the `serde` feature).
//!
//! Default serialization round-trips through the canonical decimal
//! string. For binary formats where the string round-trip is
//! wasteful, callers can opt into [`serde_bid`] via
//! `#[serde(with = "ferrodec_decimal64::serde_bid")]`.
//!
//! The `serde` feature pulls in `fmt` because the default path needs
//! [`Display`](core::fmt::Display), and even the BID-bits path uses
//! the same fmt-gated parser as a fallback for human-readable text
//! deserializers.

extern crate alloc;

use alloc::string::ToString;
use core::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

use crate::decimal::Decimal64;
use crate::status::RoundingMode;

impl Serialize for Decimal64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Decimal64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;
        impl Visitor<'_> for StringVisitor {
            type Value = Decimal64;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal literal string parseable as Decimal64")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Decimal64::parse_str(v, RoundingMode::NearestEven)
                    .map(|(d, _)| d)
                    .map_err(E::custom)
            }
        }
        deserializer.deserialize_str(StringVisitor)
    }
}

/// Serialize / deserialize a [`Decimal64`] as the raw 64-bit BID
/// pattern. Compact in binary formats; the human-readable path falls
/// back to the default decimal-string deserializer if the input is a
/// string rather than a 64-bit integer.
///
/// Use via `#[serde(with = "ferrodec_decimal64::serde_bid")]` on a
/// field.
pub mod serde_bid {
    use super::Decimal64;
    use serde::de::{self, Deserializer};
    use serde::ser::Serializer;

    /// Serialize as a `u64` BID bit pattern.
    ///
    /// # Errors
    ///
    /// Returns the underlying serializer's error (for example, if the
    /// format does not support `u64`).
    pub fn serialize<S: Serializer>(d: &Decimal64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(d.to_bits())
    }

    /// Deserialize from a `u64` BID bit pattern, falling back to the
    /// canonical decimal-string parser for human-readable formats.
    ///
    /// # Errors
    ///
    /// Returns the underlying deserializer's error if the input is
    /// neither a `u64` (or smaller integer) nor a decimal-literal
    /// string.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal64, D::Error> {
        struct BitsVisitor;
        impl de::Visitor<'_> for BitsVisitor {
            type Value = Decimal64;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a 64-bit BID pattern (as u64) or a decimal-literal string")
            }
            fn visit_u32<E: de::Error>(self, v: u32) -> Result<Self::Value, E> {
                Ok(Decimal64::from_bits(u64::from(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Decimal64::from_bits(v))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Decimal64::parse_str(v, crate::status::RoundingMode::NearestEven)
                    .map(|(d, _)| d)
                    .map_err(E::custom)
            }
        }
        deserializer.deserialize_any(BitsVisitor)
    }
}
