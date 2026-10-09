#![no_std]
#![no_main]
use core::panic::PanicInfo;
use ferrodec::Decimal128;
use serde::ser::{Impossible, Serialize, Serializer};
#[panic_handler]
fn panic(_: &PanicInfo) -> ! { loop {} }
#[derive(Debug)]
struct E;
impl core::fmt::Display for E { fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str("e") } }
impl serde::ser::StdError for E {}
impl serde::ser::Error for E { fn custom<T: core::fmt::Display>(_: T) -> Self { E } }
struct S;
macro_rules! sc { ($($m:ident: $t:ty),*) => { $(fn $m(self, _v: $t) -> Result<u32, E> { Ok(0) })* } }
impl Serializer for S {
    type Ok = u32; type Error = E;
    type SerializeSeq = Impossible<u32, E>; type SerializeTuple = Impossible<u32, E>;
    type SerializeTupleStruct = Impossible<u32, E>; type SerializeTupleVariant = Impossible<u32, E>;
    type SerializeMap = Impossible<u32, E>; type SerializeStruct = Impossible<u32, E>;
    type SerializeStructVariant = Impossible<u32, E>;
    sc!(serialize_bool: bool, serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64,
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64, serialize_f32: f32,
        serialize_f64: f64, serialize_char: char, serialize_bytes: &[u8]);
    fn serialize_str(self, v: &str) -> Result<u32, E> { Ok(v.len() as u32) }
    fn collect_str<T: ?Sized + core::fmt::Display>(self, _: &T) -> Result<u32, E> { Ok(0) }
    fn serialize_none(self) -> Result<u32, E> { Ok(0) }
    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<u32, E> { Ok(0) }
    fn serialize_unit(self) -> Result<u32, E> { Ok(0) }
    fn serialize_unit_struct(self, _: &'static str) -> Result<u32, E> { Ok(0) }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<u32, E> { Ok(0) }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, _: &T) -> Result<u32, E> { Ok(0) }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<u32, E> { Ok(0) }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, E> { Err(E) }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, E> { Err(E) }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, E> { Err(E) }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, E> { Err(E) }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, E> { Err(E) }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, E> { Err(E) }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, E> { Err(E) }
}
#[no_mangle]
pub extern "C" fn _start() -> u32 {
    let d = core::hint::black_box(Decimal128::ONE);
    d.serialize(S).unwrap_or(7)
}
