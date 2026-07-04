//! Serde [`Serialize`] / [`Deserialize`] for [`Decimal128`].
//!
//! Default serialization round-trips through the canonical decimal
//! string ([`Display`](core::fmt::Display) / [`parse_str`]). That keeps
//! values human-readable in JSON / TOML / YAML and survives every
//! format. For binary formats where the string round-trip is wasteful,
//! callers can opt into [`serde_bid`] via `#[serde(with = "...")]`:
//!
//! ```ignore
//! use serde::{Serialize, Deserialize};
//! use ferrodec::Decimal128;
//!
//! #[derive(Serialize, Deserialize)]
//! struct Row {
//!     // String serialization (default).
//!     price: Decimal128,
//!     // Raw 128-bit BID pattern (compact for bincode / MessagePack).
//!     #[serde(with = "ferrodec::serde_bid")]
//!     id_amount: Decimal128,
//! }
//! ```
//!
//! Both forms require `feature = "fmt"` because the default path needs
//! `Display`, and even the BID-bits path uses the same fmt-gated
//! parser as a fallback for human-readable text deserializers.
//!
//! [`parse_str`]: crate::Decimal128::parse_str

extern crate alloc;

use alloc::string::ToString;
use core::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

use crate::decimal::Decimal128;
use crate::status::RoundingMode;

impl Serialize for Decimal128 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The canonical Display representation is what humans (and most
        // formats' string types) want. Allocates a small String per
        // call; if that's a hot path the `serde_bid` module is the
        // 16-byte alternative.
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Decimal128 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;
        impl Visitor<'_> for StringVisitor {
            type Value = Decimal128;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal literal string parseable as Decimal128")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Decimal128::parse_str(v, RoundingMode::NearestEven)
                    .map(|(d, _)| d)
                    .map_err(|e| E::custom(e))
            }
        }
        deserializer.deserialize_str(StringVisitor)
    }
}

/// Serialize / deserialize a [`Decimal128`] as the raw 128-bit BID
/// pattern. Compact in binary formats; the human-readable path falls
/// back to the default decimal-string deserializer if the input is a
/// string rather than a 128-bit integer.
///
/// Use via `#[serde(with = "ferrodec::serde_bid")]` on a field.
pub mod serde_bid {
    use super::Decimal128;
    use serde::de::{self, Deserializer};
    use serde::ser::Serializer;

    pub fn serialize<S: Serializer>(d: &Decimal128, serializer: S) -> Result<S::Ok, S::Error> {
        // u128 is supported by serde >= 1.0.110 across most data
        // formats. JSON serializes as a JSON number when the format
        // accepts arbitrary precision integers; otherwise it errors,
        // which is the correct behaviour for "use the bits" mode.
        serializer.serialize_u128(d.to_bits())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal128, D::Error> {
        struct BitsVisitor;
        impl de::Visitor<'_> for BitsVisitor {
            type Value = Decimal128;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a 128-bit BID pattern (as u128) or a decimal-literal string")
            }
            fn visit_u128<E: de::Error>(self, v: u128) -> Result<Self::Value, E> {
                Ok(Decimal128::from_bits(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Decimal128::from_bits(u128::from(v)))
            }
            fn visit_u32<E: de::Error>(self, v: u32) -> Result<Self::Value, E> {
                Ok(Decimal128::from_bits(u128::from(v)))
            }
            fn visit_u16<E: de::Error>(self, v: u16) -> Result<Self::Value, E> {
                Ok(Decimal128::from_bits(u128::from(v)))
            }
            fn visit_u8<E: de::Error>(self, v: u8) -> Result<Self::Value, E> {
                Ok(Decimal128::from_bits(u128::from(v)))
            }
            fn visit_i128<E: de::Error>(self, v: i128) -> Result<Self::Value, E> {
                #[allow(clippy::cast_sign_loss)]
                Ok(Decimal128::from_bits(v as u128))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                #[allow(clippy::cast_sign_loss)]
                Ok(Decimal128::from_bits(v as u128))
            }
            fn visit_i32<E: de::Error>(self, v: i32) -> Result<Self::Value, E> {
                #[allow(clippy::cast_sign_loss)]
                Ok(Decimal128::from_bits(u128::from(v as u32)))
            }
            fn visit_i16<E: de::Error>(self, v: i16) -> Result<Self::Value, E> {
                #[allow(clippy::cast_sign_loss)]
                Ok(Decimal128::from_bits(u128::from(v as u16)))
            }
            fn visit_i8<E: de::Error>(self, v: i8) -> Result<Self::Value, E> {
                #[allow(clippy::cast_sign_loss)]
                Ok(Decimal128::from_bits(u128::from(v as u8)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                // String fallback for human-readable formats.
                Decimal128::parse_str(v, crate::status::RoundingMode::NearestEven)
                    .map(|(d, _)| d)
                    .map_err(|e| E::custom(e))
            }
        }
        // Per-format deserialization (fd-aqs.13). `deserialize_any` asks
        // the format to self-describe, which the non-self-describing
        // formats this module targets (bincode, MessagePack) reject at
        // runtime — the review's finding. Branch on `is_human_readable`:
        // a binary format reads the `u128` that `serialize_u128` wrote,
        // while a human-readable format keeps `deserialize_any` so the
        // decimal-string fallback (`visit_str`) still accepts a
        // hand-written JSON / YAML string.
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(BitsVisitor)
        } else {
            deserializer.deserialize_u128(BitsVisitor)
        }
    }
}
