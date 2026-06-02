//! String conversion: parsing a General Decimal Arithmetic numeric string
//! and formatting in to-scientific notation. Gated behind the `fmt` feature.

mod display;
mod parse;

pub use parse::ParseDecimalError;
