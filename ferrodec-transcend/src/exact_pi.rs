//! Input-side exact classification for the pi-scaled family
//! (IEEE 754-2019 §9.2 `sinPi`, `cosPi`, `tanPi`, `asinPi`, `acosPi`,
//! `atanPi`, `atan2Pi`; ADR-0061, fd-4zo.26).
//!
//! A format value is `p/10^k` in lowest terms with a power-of-ten
//! denominator. Niven's theorem (registry:
//! `niven-irrational-numbers`) inventories the rational values of the
//! forward family completely — `sin(πr)` is rational only in
//! `{0, ±1/2, ±1}` — and the decimal representability of the special
//! abscissas does the rest: `±1/2` needs `r = k ± 1/6` (sin) or
//! `k ± 1/3` (cos), which no decimal format value is. The exact sets
//! therefore collapse to residue tests on the operand's own digits:
//! integers and half integers for `sinPi`/`cosPi`, plus quarter
//! integers (values `±1`) and half-integer poles for `tanPi`. The
//! inverse family's exact sets are the finite tables the same theorem
//! gives in reverse, with the non-terminating outputs (`asinPi(±1/2)
//! = ±1/6`, `acosPi(±1/2) ∈ {1/3, 2/3}`) proven never-exact by the
//! `1/q` argument `exact::rsqrt_exact_input` established: a lowest
//! terms denominator with a prime other than 2 and 5 terminates
//! nowhere.
//!
//! ## The no-ties theorem (ADR-0061's spec obligation, proven here)
//!
//! A nearest-mode tie is a value exactly on a `PRECISION + 1`-digit
//! midpoint: rational, terminating, and NOT representable at
//! `PRECISION` digits. For the forward family the rational values
//! are `{0, ±1/2, ±1}` (Niven), each representable at every format
//! precision (`5·10^−1` needs one digit): grid points, never
//! midpoints. For the inverse family the rational values are the
//! finite tables below; the terminating members (`0`, `±1/4`,
//! `±1/2`, `±3/4`, `1`) are all representable, and the
//! non-terminating members cannot be midpoints because midpoints
//! terminate. Therefore NO operation in this family has a
//! nearest-mode tie at any format, `ladder_audit` is vacuous for the
//! family by construction, and every classifier below delivers exact
//! values only (`Status::OK`, §7.5).
//!
//! ## Sign conventions (transcribed, not derived)
//!
//! The zero-sign and pole-sign rules follow IEEE 754-2019 §9.2.1 as
//! transcribed from the two proxies ADR-0061 names (the MPFR 4.2.2
//! manual and source, which state they follow IEEE 754 sinPi/cosPi/
//! tanPi, and the C23 Annex F rows): `sinPi(n) = ±0` with the SIGN
//! OF THE OPERAND (the standard chooses odd-function consistency
//! over the one-sided limit); `cosPi(n + 1/2) = +0` always (even
//! function); `tanPi(n)` is zero with sign `(−1)^n · sign(x)`, and
//! `tanPi(n + 1/2)` is `+∞` for even `n`, `−∞` for odd `n`, with
//! `DIV_BY_ZERO` (the mod-8 table in `mpfr-4.2.2/src/tanu.c`,
//! extracted per the ADR).
//!
//! Every `None` below is provably neither exact nor a tie (the
//! no-ties theorem plus Niven), so the kernels' unconditional
//! `INEXACT` past these classifiers is correct in every mode.

use crate::format::DecimalFormat;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::U256;

/// The operand's pi-family residue class, decided exactly from the
/// stored digits: which multiple of `1/4` the value is, if any.
///
/// `Integer { odd }` and `HalfInteger { odd }` carry the parity of
/// the integer part `n` (for `n + 1/2`, the parity of `n`);
/// `QuarterInteger { .. }` carries the mod-4 position of `4x` among
/// `{1, 3, 5, 7} mod 8` folded to what the `tanPi` table needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PiResidue {
    /// `x` is an integer; `odd` is `n mod 2` of its magnitude.
    Integer { odd: bool },
    /// `x = n + 1/2` exactly; `odd` is `n mod 2` for the magnitude
    /// spelling `|x| = m + 1/2` (so the caller reflects for sign).
    HalfInteger { odd: bool },
    /// `x = n + 1/4` or `n + 3/4` exactly; `three_quarters` says
    /// which, again for the magnitude spelling.
    QuarterInteger { odd: bool, three_quarters: bool },
    /// None of the above: by Niven the forward family's value at `x`
    /// is irrational.
    Generic,
}

/// Classify `|x| = coef · 10^exp` (nonzero, finite) into its residue
/// class by exact integer arithmetic on the digits.
///
/// The tests are exact and total: `x` is an integer iff its stripped
/// exponent is nonnegative; `2x` (`4x`) is an integer iff the doubled
/// (quadrupled) coefficient strips to a nonnegative exponent. The
/// coefficient arithmetic stays in `U256` (`4 · 10^34 < 10^36`,
/// decades inside the envelope). Parity of an integer `m · 10^e`
/// (stripped, `e ≥ 0`): even for `e ≥ 1`, else `m`'s own low bit.
pub(crate) fn pi_residue(coef: U256, exp: i32) -> PiResidue {
    debug_assert!(!coef.is_zero(), "zero is dispatched by the kernels");
    if let Some(odd) = integer_parity(coef, exp) {
        return PiResidue::Integer { odd };
    }
    // 2x: is x a half integer? The integer part's parity is the
    // parity of (2x − 1)/2 … computed directly: 2|x| = m odd integer
    // means |x| = (m − 1)/2 + 1/2, and (m − 1)/2 is odd iff
    // m ≡ 3 (mod 4).
    let twice = coef.add(coef);
    if let Some(odd2) = integer_parity(twice, exp) {
        debug_assert!(odd2, "2x even would have made x an integer");
        let m_mod4 = low_mod4(twice, exp);
        return PiResidue::HalfInteger { odd: m_mod4 == 3 };
    }
    // 4x: is x a quarter integer? 4|x| = q odd integer; |x| =
    // (q − 1)/4 + 1/4 when q ≡ 1 (mod 4) (a `+1/4` point) and
    // (q − 3)/4 + 3/4 when q ≡ 3 (mod 4). The integer part's parity
    // is ((q − 1)/4) mod 2 resp. ((q − 3)/4) mod 2, i.e. bits of
    // q mod 16; `tanPi`'s table only needs `q mod 8` (the mod-8
    // classes 1/3/5/7 of `tanu.c`), which `low_mod8` supplies.
    let quad = twice.add(twice);
    if let Some(odd4) = integer_parity(quad, exp) {
        debug_assert!(odd4, "4x even would have made x a half integer");
        let q_mod8 = low_mod8(quad, exp);
        debug_assert!(q_mod8 % 2 == 1);
        return PiResidue::QuarterInteger {
            odd: q_mod8 == 5 || q_mod8 == 7,
            three_quarters: q_mod8 == 3 || q_mod8 == 7,
        };
    }
    PiResidue::Generic
}

/// `Some(parity)` when `coef · 10^exp` is an integer (parity of its
/// magnitude), `None` otherwise. Stripping first makes the exponent
/// test decisive.
fn integer_parity(coef: U256, exp: i32) -> Option<bool> {
    let (c, e) = crate::exact::strip_trailing_zeros(coef, exp);
    if e < 0 {
        return None;
    }
    if e >= 1 {
        return Some(false); // a multiple of ten is even
    }
    Some(c.lo & 1 == 1)
}

/// The integer `coef · 10^exp` reduced mod 4 (caller guarantees it
/// IS an integer). `10^e mod 4 = 0` for `e ≥ 2`, `2` for `e = 1`.
fn low_mod4(coef: U256, exp: i32) -> u8 {
    let (c, e) = crate::exact::strip_trailing_zeros(coef, exp);
    debug_assert!(e >= 0);
    let c_mod = (c.lo % 4) as u8;
    match e {
        0 => c_mod,
        1 => (c_mod * 2) % 4,
        _ => 0,
    }
}

/// The integer `coef · 10^exp` reduced mod 8 (caller guarantees it
/// IS an integer). `10^e mod 8`: `1, 2, 4, 0` for `e = 0, 1, 2, ≥3`.
fn low_mod8(coef: U256, exp: i32) -> u8 {
    let (c, e) = crate::exact::strip_trailing_zeros(coef, exp);
    debug_assert!(e >= 0);
    let c_mod = (c.lo % 8) as u8;
    match e {
        0 => c_mod,
        1 => (c_mod * 2) % 8,
        2 => (c_mod * 4) % 8,
        _ => 0,
    }
}

/// The delivered shape of a pi-family exact case, decoupled from the
/// format so the decision logic tests without a rounder (the
/// `rsqrt_exact_parts` pattern): the wrappers below translate into
/// format constants and one exact `round_and_pack_finite` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PiExact {
    /// A signed zero.
    Zero { neg: bool },
    /// A representable nonzero value `coef · 10^exp`, delivered
    /// exactly (`OK`, §7.5).
    Zvalue { coef: u128, exp: i32, neg: bool },
    /// A `tanPi` pole: signed infinity with `DIV_BY_ZERO`.
    Pole { neg: bool },
}

/// `sinPi`'s exact decision on the decoded magnitude parts plus the
/// operand's sign: integers give `±0` with the SIGN OF THE OPERAND
/// (the §9.2.1 odd-function rule, not the one-sided limit); half
/// integers give `(−1)^n` for `|x| = n + 1/2`, reflected through the
/// odd function. `None` is irrational by Niven, and the kernel's
/// `INEXACT` past it is correct in every mode.
pub(crate) fn sinpi_exact(coef: U256, exp: i32, sign: bool) -> Option<PiExact> {
    match pi_residue(coef, exp) {
        PiResidue::Integer { .. } => Some(PiExact::Zero { neg: sign }),
        PiResidue::HalfInteger { odd } => Some(PiExact::Zvalue {
            coef: 1,
            exp: 0,
            neg: odd != sign,
        }),
        _ => None,
    }
}

/// `cosPi`'s exact decision: integers give `(−1)^n` (sign
/// independent, even function); half integers give `+0` ALWAYS (the
/// §9.2.1 rule that keeps the function even).
pub(crate) fn cospi_exact(coef: U256, exp: i32) -> Option<PiExact> {
    match pi_residue(coef, exp) {
        PiResidue::Integer { odd } => Some(PiExact::Zvalue {
            coef: 1,
            exp: 0,
            neg: odd,
        }),
        PiResidue::HalfInteger { .. } => Some(PiExact::Zero { neg: false }),
        _ => None,
    }
}

/// `tanPi`'s exact decision, the `tanu.c` mod-8 table transcribed
/// (ADR-0061): integers give zero with sign `(−1)^n · sign(x)`;
/// half integers `|x| = n + 1/2` give `+∞` for even `n`, `−∞` for
/// odd `n`, odd-reflected for a negative operand, with
/// `DIV_BY_ZERO`; quarter integers give `+1` at `n + 1/4` and `−1`
/// at `n + 3/4` (period 1), odd-reflected.
pub(crate) fn tanpi_exact(coef: U256, exp: i32, sign: bool) -> Option<PiExact> {
    match pi_residue(coef, exp) {
        PiResidue::Integer { odd } => Some(PiExact::Zero { neg: odd != sign }),
        PiResidue::HalfInteger { odd } => Some(PiExact::Pole { neg: odd != sign }),
        PiResidue::QuarterInteger { three_quarters, .. } => Some(PiExact::Zvalue {
            coef: 1,
            exp: 0,
            neg: three_quarters != sign,
        }),
        PiResidue::Generic => None,
    }
}

/// `asinPi`'s exact decision beyond the kernel's `±0` row: the
/// finite table `{±1 → ±1/2}`. `asinPi(±1/2) = ±1/6` is rational
/// but non-terminating (3 divides the lowest-terms denominator, the
/// `rsqrt` `1/q` argument), so it is neither exact nor a tie and
/// stays `None`; every other rational output is excluded by Niven
/// reversed. Domain rows (`|x| > 1` → NaN) are the kernel's.
pub(crate) fn asinpi_exact(coef: U256, exp: i32, sign: bool) -> Option<PiExact> {
    if is_one(coef, exp) {
        return Some(PiExact::Zvalue {
            coef: 5,
            exp: -1,
            neg: sign,
        });
    }
    None
}

/// `acosPi`'s exact decision beyond the kernel's `±0 → 1/2` row:
/// `{+1 → +0, −1 → 1}`. `acosPi(±1/2) ∈ {1/3, 2/3}` is
/// non-terminating: `None`, kernel `INEXACT` correct.
pub(crate) fn acospi_exact(coef: U256, exp: i32, sign: bool) -> Option<PiExact> {
    if is_one(coef, exp) {
        return Some(if sign {
            PiExact::Zvalue {
                coef: 1,
                exp: 0,
                neg: false,
            }
        } else {
            PiExact::Zero { neg: false }
        });
    }
    None
}

/// `atanPi`'s exact decision beyond the kernel's `±0` row:
/// `{±1 → ±1/4}` — the family the decimal formats keep where `1/6`
/// and `1/3` denied the arcsine and arccosine.
pub(crate) fn atanpi_exact(coef: U256, exp: i32, sign: bool) -> Option<PiExact> {
    if is_one(coef, exp) {
        return Some(PiExact::Zvalue {
            coef: 25,
            exp: -2,
            neg: sign,
        });
    }
    None
}

/// `atan2Pi`'s exact decision beyond the kernel's §9.2.1 axis rows:
/// the diagonals. For finite nonzero operands with `|y| = |x|`
/// (compared on stripped parts, cohort insensitive), the value is
/// exactly `±1/4` (`x > 0`) or `±3/4` (`x < 0`), signed by `y`; by
/// Niven on the tangent no other finite nonzero ratio yields a
/// rational result, so this one comparison completes the
/// classification.
pub(crate) fn atan2pi_exact(
    cy: U256,
    ey: i32,
    sy: bool,
    cx: U256,
    ex: i32,
    sx: bool,
) -> Option<PiExact> {
    let (cys, eys) = crate::exact::strip_trailing_zeros(cy, ey);
    let (cxs, exs) = crate::exact::strip_trailing_zeros(cx, ex);
    if cys.lo != cxs.lo || cys.hi != cxs.hi || eys != exs {
        return None;
    }
    Some(PiExact::Zvalue {
        coef: if sx { 75 } else { 25 },
        exp: -2,
        neg: sy,
    })
}

/// `|x| = 1` on stripped parts.
fn is_one(coef: U256, exp: i32) -> bool {
    let (c, e) = crate::exact::strip_trailing_zeros(coef, exp);
    c.lo == 1 && c.hi == 0 && e == 0
}

/// Translate a [`PiExact`] into a format delivery: zeros and
/// infinities through the format constants, values through one exact
/// `round_and_pack_finite` call (preferred quantum 0, this family's
/// §9.2.2 row; `OK` per §7.5, `DIV_BY_ZERO` on the poles).
pub(crate) fn deliver_pi_exact<F: DecimalFormat>(e: PiExact, rm: RoundingMode) -> (F, Status) {
    match e {
        PiExact::Zero { neg } => (if neg { F::NEG_ZERO } else { F::ZERO }, Status::OK),
        PiExact::Pole { neg } => (
            if neg { F::NEG_INFINITY } else { F::INFINITY },
            Status::DIV_BY_ZERO,
        ),
        PiExact::Zvalue { coef, exp, neg } => {
            let (r, st) =
                F::round_and_pack_finite(U256::from_u128(coef), exp, 0, neg, false, rm, Status::OK);
            debug_assert!(st == Status::OK, "pi-family exact values pack cleanly");
            (r, st)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(n: u128) -> U256 {
        U256::from_u128(n)
    }

    #[test]
    fn residue_classes_from_exact_digits() {
        use PiResidue::*;
        let r = |c: u128, e: i32| pi_residue(u(c), e);
        assert_eq!(r(3, 0), Integer { odd: true });
        assert_eq!(r(40, -1), Integer { odd: false }); // 4.0
        assert_eq!(r(1, 2), Integer { odd: false }); // 100
        assert_eq!(r(5, -1), HalfInteger { odd: false }); // 0.5
        assert_eq!(r(15, -1), HalfInteger { odd: true }); // 1.5
        assert_eq!(r(25, -1), HalfInteger { odd: false }); // 2.5
        assert_eq!(r(35, -1), HalfInteger { odd: true }); // 3.5
        assert_eq!(
            r(25, -2), // 0.25
            QuarterInteger {
                odd: false,
                three_quarters: false
            }
        );
        assert_eq!(
            r(75, -2), // 0.75
            QuarterInteger {
                odd: false,
                three_quarters: true
            }
        );
        assert_eq!(
            r(125, -2), // 1.25: 4x = 5, mod8 = 5
            QuarterInteger {
                odd: true,
                three_quarters: false
            }
        );
        assert_eq!(
            r(175, -2), // 1.75: 4x = 7, mod8 = 7
            QuarterInteger {
                odd: true,
                three_quarters: true
            }
        );
        assert_eq!(r(1234567, -6), Generic);
        assert_eq!(r(3, -1), Generic); // 0.3
                                       // Huge integers: quantum >= 1 forces the class (and evenness).
        assert_eq!(
            r(9_999_999_999_999_999_999_999_999_999_999_999, 10),
            Integer { odd: false }
        );
    }

    #[test]
    fn sinpi_decision_table() {
        use PiExact::*;
        // Integers: zero with the operand's sign (the §9.2.1 odd rule).
        assert_eq!(sinpi_exact(u(3), 0, false), Some(Zero { neg: false }));
        assert_eq!(sinpi_exact(u(3), 0, true), Some(Zero { neg: true }));
        // Half integers: (−1)^n, odd-reflected.
        assert_eq!(
            sinpi_exact(u(5), -1, false), // sinPi(0.5) = 1
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        assert_eq!(
            sinpi_exact(u(15), -1, false), // sinPi(1.5) = −1
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: true
            })
        );
        assert_eq!(
            sinpi_exact(u(15), -1, true), // sinPi(−1.5) = 1
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        assert_eq!(sinpi_exact(u(1234567), -7, false), None);
    }

    #[test]
    fn cospi_decision_table() {
        use PiExact::*;
        assert_eq!(
            cospi_exact(u(2), 0), // cosPi(2) = 1
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        assert_eq!(
            cospi_exact(u(3), 0), // cosPi(±3) = −1
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: true
            })
        );
        // Half integers: +0 always (even function).
        assert_eq!(cospi_exact(u(35), -1), Some(Zero { neg: false }));
        assert_eq!(cospi_exact(u(7), -1), None); // 0.7
    }

    #[test]
    fn tanpi_decision_table() {
        use PiExact::*;
        // Integers: zero, sign (−1)^n · sign(x).
        assert_eq!(tanpi_exact(u(2), 0, false), Some(Zero { neg: false }));
        assert_eq!(tanpi_exact(u(3), 0, false), Some(Zero { neg: true }));
        assert_eq!(tanpi_exact(u(3), 0, true), Some(Zero { neg: false }));
        // Half-integer poles: parity plus odd reflection, DIV_BY_ZERO
        // at the delivery.
        assert_eq!(tanpi_exact(u(5), -1, false), Some(Pole { neg: false })); // 0.5 → +∞
        assert_eq!(tanpi_exact(u(15), -1, false), Some(Pole { neg: true })); // 1.5 → −∞
        assert_eq!(tanpi_exact(u(5), -1, true), Some(Pole { neg: true })); // −0.5 → −∞
        assert_eq!(tanpi_exact(u(25), -1, false), Some(Pole { neg: false })); // 2.5 → +∞
                                                                              // Quarter integers: ±1 by the three-quarters flag, period 1,
                                                                              // odd-reflected.
        assert_eq!(
            tanpi_exact(u(25), -2, false),
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        assert_eq!(
            tanpi_exact(u(75), -2, false),
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: true
            })
        );
        assert_eq!(
            tanpi_exact(u(125), -2, false),
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        assert_eq!(
            tanpi_exact(u(25), -2, true),
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: true
            })
        );
        assert_eq!(tanpi_exact(u(3), -1, false), None); // 0.3
    }

    #[test]
    fn inverse_decision_tables() {
        use PiExact::*;
        // asinPi(±1) = ±1/2.
        assert_eq!(
            asinpi_exact(u(1), 0, false),
            Some(Zvalue {
                coef: 5,
                exp: -1,
                neg: false
            })
        );
        assert_eq!(
            asinpi_exact(u(10), -1, true), // cohort 1.0
            Some(Zvalue {
                coef: 5,
                exp: -1,
                neg: true
            })
        );
        // asinPi(1/2) = 1/6: rational, non-terminating, declined.
        assert_eq!(asinpi_exact(u(5), -1, false), None);
        // acosPi(+1) = +0; acosPi(−1) = 1.
        assert_eq!(acospi_exact(u(1), 0, false), Some(Zero { neg: false }));
        assert_eq!(
            acospi_exact(u(1), 0, true),
            Some(Zvalue {
                coef: 1,
                exp: 0,
                neg: false
            })
        );
        // atanPi(±1) = ±1/4.
        assert_eq!(
            atanpi_exact(u(1), 0, true),
            Some(Zvalue {
                coef: 25,
                exp: -2,
                neg: true
            })
        );
        assert_eq!(atanpi_exact(u(2), 0, false), None);
        // atan2Pi diagonals, cohort-insensitively.
        assert_eq!(
            atan2pi_exact(u(3), 0, false, u(30), -1, false),
            Some(Zvalue {
                coef: 25,
                exp: -2,
                neg: false
            })
        );
        assert_eq!(
            atan2pi_exact(u(3), 0, true, u(3), 0, true),
            Some(Zvalue {
                coef: 75,
                exp: -2,
                neg: true
            })
        );
        assert_eq!(atan2pi_exact(u(3), 0, false, u(2), 0, false), None);
    }
}
