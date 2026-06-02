//! Embedding exact published physical constants at compile time.
//!
//!     cargo run --example physical_constants
//!
//! Every constant below is built during `const` evaluation, so the binary
//! carries the baked decimal value with no runtime parser. A mistyped or
//! inexact literal would be a compile error, not a silent rounding, which
//! is what you want for a value that propagates into a calculator's
//! results.

use ferrodec::{dec, Decimal128, Decimal128Parts};

// The legible spelling: the source reads as the published decimal.
const PLANCK: Decimal128 = Decimal128::from_str_const("6.62607015e-34");
const SPEED_OF_LIGHT: Decimal128 = Decimal128::from_str_const("2.99792458e8");

// The `dec!` macro is the same thing, terser.
const ELEMENTARY_CHARGE: Decimal128 = dec!("1.602176634e-19");
const BOLTZMANN: Decimal128 = dec!("1.380649e-23");
const AVOGADRO: Decimal128 = dec!("6.02214076e23");

// The raw-parts spelling, available even without the `fmt` feature:
// 9.80665 = 980665 * 10^-5.
const STANDARD_GRAVITY: Decimal128 = Decimal128::from_parts(Decimal128Parts {
    negative: false,
    coefficient: 980_665,
    exponent: -5,
})
.unwrap();

fn main() {
    println!("Planck constant     h    = {PLANCK} J s");
    println!("Speed of light      c    = {SPEED_OF_LIGHT} m/s");
    println!("Elementary charge   e    = {ELEMENTARY_CHARGE} C");
    println!("Boltzmann constant  k    = {BOLTZMANN} J/K");
    println!("Avogadro constant   N_A  = {AVOGADRO} 1/mol");
    println!("Standard gravity    g_0  = {STANDARD_GRAVITY} m/s^2");

    // Each value is exact and quantum preserving: it decodes back to the
    // very parts it was built from, with no rounding in between.
    assert_eq!(PLANCK.decode().unwrap().coefficient, 662_607_015);
    assert_eq!(STANDARD_GRAVITY.decode().unwrap().exponent, -5);
}
