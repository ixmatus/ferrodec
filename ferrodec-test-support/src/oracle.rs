//! Exact correctly-rounded oracle for the decimal arithmetic operations.
//!
//! IEEE 754-2019 §4.3 requires every arithmetic operation to deliver the
//! representable value *nearest* the infinitely-precise mathematical
//! result, broken per the active rounding-direction attribute. That is a
//! statement with **zero tolerance**: the result either *is* the
//! correctly-rounded value or the implementation is wrong.
//!
//! This module computes that value exactly. For `add`, `subtract`,
//! `multiply`, and `fusedMultiplyAdd` the infinitely-precise result is a
//! finite decimal (the operands are integers scaled by powers of ten, so
//! the exact sum / product / fused result is too), and the oracle forms
//! it exactly with arbitrary-precision integers. For `divide` and
//! `squareRoot` the exact result is generally non-terminating, but the
//! correctly-rounded value is still exactly determined: the oracle
//! expands the quotient / root to `precision + 2` significant digits
//! with an exact integer remainder, and the remainder being non-zero is
//! the exact sticky bit. Every rounding decision is therefore made from
//! exact integer comparisons, never a floating tolerance.
//!
//! The oracle is independent of the code under test: the rounding
//! decision ([`round_up_decision`]) is transcribed directly from IEEE
//! 754-2019 §4.3.3, and the cohort / preferred-exponent selection is
//! derived from the General Decimal Arithmetic "ideal exponent" rules,
//! not read out of `ferrodec`'s kernel. It produces a decTest-style
//! string (the convention the conformance vectors already use) plus the
//! five mandatory IEEE 754 status flags; the per-crate test re-parses
//! that string through its own `parse_str` and compares `to_bits()` and
//! status, exactly as [`crate::conformance`] compares decTest expected
//! values. Cohort equality is the strongest possible faithfulness
//! statement: it pins the §6.3 preferred exponent, which a ULP envelope
//! cannot observe at all.
//!
//! See ADR-0021 for why this supersedes the prior `within_ulps`
//! tolerance envelope.

use core::cmp::Ordering;

use ferrodec_ieee::{RoundingMode, Status};
use num_bigint::BigUint;

// ---------------------------------------------------------------------------
// Format parameters

/// Static parameters of one decimal interchange format.
///
/// `precision` is the significand digit count; `emax` / `emin` are the
/// maximum / minimum *adjusted* exponents (IEEE 754-2019 §3.5). The
/// quantum (encoding) exponent of a representable value lies in
/// `[qmin, qmax]` where `qmin = emin - (precision - 1)` and
/// `qmax = emax - (precision - 1)`.
#[derive(Clone, Copy, Debug)]
pub struct Format {
    /// Significand digit count (34 / 16 / 7).
    pub precision: u32,
    /// Maximum adjusted exponent (6144 / 384 / 96).
    pub emax: i32,
    /// Minimum adjusted exponent (-6143 / -383 / -95).
    pub emin: i32,
}

impl Format {
    /// IEEE 754-2019 decimal128 parameters.
    pub const DECIMAL128: Self = Self {
        precision: 34,
        emax: 6144,
        emin: -6143,
    };
    /// IEEE 754-2019 decimal64 parameters.
    pub const DECIMAL64: Self = Self {
        precision: 16,
        emax: 384,
        emin: -383,
    };
    /// IEEE 754-2019 decimal32 parameters.
    pub const DECIMAL32: Self = Self {
        precision: 7,
        emax: 96,
        emin: -95,
    };

    /// Smallest representable quantum exponent.
    #[must_use]
    pub const fn qmin(&self) -> i32 {
        self.emin - (self.precision as i32 - 1)
    }

    /// Largest representable quantum exponent.
    #[must_use]
    pub const fn qmax(&self) -> i32 {
        self.emax - (self.precision as i32 - 1)
    }
}

// ---------------------------------------------------------------------------
// Decimal value model

/// A finite decimal value `(-1)^neg · coeff · 10^exp`.
///
/// `coeff` carries no implied normalization: trailing zeros are
/// significant and define the value's cohort (quantum), exactly as in
/// the interchange encoding. A decoded `0` keeps its sign and exponent
/// so signed-zero and zero-quantum behaviour round-trips.
#[derive(Clone, Debug)]
pub struct Dec {
    /// Sign: `true` for negative.
    pub neg: bool,
    /// Significand magnitude (non-negative).
    pub coeff: BigUint,
    /// Quantum exponent.
    pub exp: i32,
}

impl Dec {
    /// Construct from raw parts.
    #[must_use]
    pub fn new(neg: bool, coeff: BigUint, exp: i32) -> Self {
        Self { neg, coeff, exp }
    }

    /// `true` if the magnitude is zero (sign and exponent still
    /// meaningful for cohort / signed-zero purposes).
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.coeff == BigUint::ZERO
    }
}

/// Parse a finite decimal literal (the form `Display` emits for any
/// finite value: optional sign, integer / fractional digits, optional
/// `E±exp`). Returns `None` for the non-finite tokens (`NaN`, `sNaN`,
/// `Infinity`, `Inf`), which the caller handles structurally.
///
/// Parsing is exact and cohort-preserving: `"1.00"` yields
/// `coeff = 100, exp = -2`; `"0.001"` yields `coeff = 1, exp = -3`;
/// `"1.234E-100"` yields `coeff = 1234, exp = -103`.
#[must_use]
pub fn parse_decimal(s: &str) -> Option<Dec> {
    let s = s.trim();
    let (neg, rest) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    // Reject the non-finite spellings (case-insensitive).
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("nan")
        || lower.starts_with("snan")
        || lower.starts_with("inf")
        || lower.is_empty()
    {
        return None;
    }

    let (mantissa, exp_part) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let parsed_exp: i32 = if exp_part.is_empty() {
        0
    } else {
        exp_part.parse().ok()?
    };

    let (int_str, frac_str) = match mantissa.find('.') {
        Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
        None => (mantissa, ""),
    };
    let mut digits = String::new();
    digits.push_str(int_str);
    digits.push_str(frac_str);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let coeff = BigUint::parse_bytes(digits.as_bytes(), 10)?;
    let exp = parsed_exp - i32::try_from(frac_str.len()).ok()?;
    Some(Dec { neg, coeff, exp })
}

/// Decode a canonical BID-128 finite (incl. zero) bit pattern into
/// `(sign, coefficient, quantum exponent)`.
///
/// The exact inverse of the `pack_finite` layout the ferrodec family
/// uses for finite values. This is the cohort-faithful way to read a
/// result back: `Display`/`{:e}` does *not* preserve a zero's quantum
/// (every zero renders as `0e+0`), so comparing a migrated property
/// test's result through a formatted string gives false cohort
/// mismatches on zero (and Etiny-underflow) results. The caller must
/// ensure the value is finite (check `is_finite()`); NaN/Infinity have
/// no coefficient/quantum.
#[must_use]
pub fn decode_decimal128(bits: u128) -> (bool, BigUint, i32) {
    const D128_BIAS: i32 = 6176;
    let sign = (bits >> 127) & 1 == 1;
    let type_bits = (bits >> 122) & 0b1_1111;
    let exp_high2 = (type_bits >> 3) & 0b11;
    let coef_high3 = type_bits & 0b111;
    let ec = (bits >> 110) & 0xFFF;
    let t = bits & ((1u128 << 110) - 1);
    let biased = ((exp_high2 << 12) | ec) as i32;
    let coef = (coef_high3 << 110) | t;
    (sign, BigUint::from(coef), biased - D128_BIAS)
}

/// Decode a canonical BID-64 finite (incl. zero) bit pattern into
/// `(sign, coefficient, quantum exponent)`. The 64-bit analogue of
/// [`decode_decimal128`] — but, unlike decimal128, decimal64 *does*
/// use BID **form B** (the `11`-prefixed encoding) for coefficients
/// `≥ 2^53`: its largest coefficient `10^16 − 1 > 2^53`, whereas
/// decimal128's `10^34 − 1 < 2^113` so form B is unreachable there.
/// This inverts both forms of `ferrodec_decimal64`'s `pack_finite`:
/// 50-bit trailing field, 8-bit exponent continuation, bias 398,
/// form-B base `2^53`. The caller must ensure the value is finite.
#[must_use]
pub fn decode_decimal64(bits: u64) -> (bool, BigUint, i32) {
    const BIAS: i32 = 398;
    const T_BITS: u32 = 50;
    const EC_BITS: u32 = 8;
    let sign = (bits >> 63) & 1 == 1;
    let type_bits = (bits >> 58) & 0b1_1111;
    let ec = (bits >> 50) & ((1u64 << EC_BITS) - 1);
    let t = bits & ((1u64 << T_BITS) - 1);
    let (exp_high2, coef) = if type_bits >> 3 == 0b11 {
        // Form B: type bits = `11 ee d`, coefficient = 2^53 + d·2^50 + t.
        let d = type_bits & 0b1;
        ((type_bits >> 1) & 0b11, (1u64 << 53) | (d << T_BITS) | t)
    } else {
        // Form A: type bits = `ee ccc`, coefficient = ccc·2^50 + t.
        ((type_bits >> 3) & 0b11, (type_bits & 0b111) << T_BITS | t)
    };
    let biased = ((exp_high2 << EC_BITS) | ec) as i32;
    (sign, BigUint::from(coef), biased - BIAS)
}

/// Decode a canonical BID-32 finite (incl. zero) bit pattern into
/// `(sign, coefficient, quantum exponent)`. As with decimal64,
/// decimal32 uses BID form B for coefficients `≥ 2^23`
/// (`10^7 − 1 > 2^23`). Inverts both forms of
/// `ferrodec_decimal32`'s `pack_finite`: 20-bit trailing field, 6-bit
/// exponent continuation, bias 101, form-B base `2^23`. The caller
/// must ensure the value is finite.
#[must_use]
pub fn decode_decimal32(bits: u32) -> (bool, BigUint, i32) {
    const BIAS: i32 = 101;
    const T_BITS: u32 = 20;
    const EC_BITS: u32 = 6;
    let sign = (bits >> 31) & 1 == 1;
    let type_bits = (bits >> 26) & 0b1_1111;
    let ec = (bits >> 20) & ((1u32 << EC_BITS) - 1);
    let t = bits & ((1u32 << T_BITS) - 1);
    let (exp_high2, coef) = if type_bits >> 3 == 0b11 {
        let d = type_bits & 0b1;
        ((type_bits >> 1) & 0b11, (1u32 << 23) | (d << T_BITS) | t)
    } else {
        ((type_bits >> 3) & 0b11, (type_bits & 0b111) << T_BITS | t)
    };
    let biased = ((exp_high2 << EC_BITS) | ec) as i32;
    (sign, BigUint::from(coef), biased - BIAS)
}

// ---------------------------------------------------------------------------
// Expected result

/// The correctly-rounded result the oracle predicts.
#[derive(Clone, Debug)]
pub enum Expect {
    /// A finite value with `coeff` already `<= precision` digits and
    /// `exp` in `[qmin, qmax]`.
    Finite { neg: bool, coeff: BigUint, exp: i32 },
    /// A signed infinity (overflow under a to-nearest mode, or the
    /// directional mode that rounds away from zero past `MAX`).
    Infinity { neg: bool },
    /// A quiet NaN with `INVALID` raised. The General Decimal
    /// Arithmetic spec defines a few finite-operand cases as undefined
    /// — notably `remainder`/`remainder-near` when the integer
    /// quotient would exceed `precision` digits (`Division_impossible`,
    /// speleotrove.com/decimal/daops.html#refrema). decTest and
    /// `ferrodec` both yield `NaN + Invalid_operation` there, so the
    /// oracle must predict it rather than the bare mathematical
    /// remainder.
    Nan,
}

impl Expect {
    /// A lossless decTest-style string. `<digits>E<exp>` parses through
    /// every `parse_str` to the exact cohort (`parse_str` preserves the
    /// literal's quantum for exactly-representable input), so the caller
    /// can compare `to_bits()` directly. Infinities map to the spelling
    /// every `parse_str` accepts.
    #[must_use]
    pub fn to_decimal_string(&self) -> String {
        match self {
            Self::Finite { neg, coeff, exp } => {
                let sign = if *neg { "-" } else { "" };
                format!("{sign}{coeff}E{exp}")
            }
            Self::Infinity { neg } => {
                if *neg {
                    "-Infinity".to_string()
                } else {
                    "Infinity".to_string()
                }
            }
            Self::Nan => "NaN".to_string(),
        }
    }
}

/// A predicted result paired with its IEEE 754 status flags.
#[derive(Clone, Debug)]
pub struct Rounded {
    /// The correctly-rounded value.
    pub value: Expect,
    /// The five mandatory IEEE 754-2019 §7 flags this operation raises.
    pub status: Status,
}

impl Rounded {
    /// The expected value as a re-parseable string.
    #[must_use]
    pub fn decimal_string(&self) -> String {
        self.value.to_decimal_string()
    }
}

// ---------------------------------------------------------------------------
// Independent rounding decision (IEEE 754-2019 §4.3.3)

/// Decide whether the kept coefficient is incremented, transcribed
/// directly from the IEEE 754-2019 §4.3.3 rounding-direction table.
///
/// This is intentionally a *fresh* implementation, not a call into
/// `ferrodec_ieee::should_round_up`: the oracle must not inherit a bug
/// from the code it audits. `ferrodec`'s decision function is proven
/// equivalent to this table separately (S6 / ADR-0021).
///
/// * `last_kept_lsb` — low digit of the kept coefficient (tie-break).
/// * `round_digit` — first dropped digit, `0..=9`.
/// * `sticky` — any non-zero digit below `round_digit`.
#[must_use]
pub fn round_up_decision(
    rm: RoundingMode,
    neg: bool,
    last_kept_lsb: u8,
    round_digit: u8,
    sticky: bool,
) -> bool {
    if round_digit == 0 && !sticky {
        return false; // exact: no direction rounds away from an exact value
    }
    match rm {
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => !neg,
        RoundingMode::TowardNegative => neg,
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::NearestEven => match round_digit.cmp(&5) {
            Ordering::Less => false,
            Ordering::Greater => true,
            Ordering::Equal => sticky || (last_kept_lsb & 1) == 1,
        },
    }
}

// ---------------------------------------------------------------------------
// Core: round an exact magnitude to the format

fn digit_count(n: &BigUint) -> u32 {
    if *n == BigUint::ZERO {
        1
    } else {
        // `to_str_radix` has no leading zeros for a non-zero value.
        u32::try_from(n.to_str_radix(10).len()).expect("digit count fits u32")
    }
}

fn pow10(k: u32) -> BigUint {
    BigUint::from(10u32).pow(k)
}

/// Round an exact value `(-1)^neg · n · 10^e0` (with `extra_sticky`
/// recording non-zero digits *below* `n`'s last digit, used by divide /
/// square-root) to `fmt` under `rm`, selecting the cohort with the
/// General Decimal Arithmetic "ideal exponent" `ideal`.
///
/// This single routine is the correctly-rounded oracle for every
/// arithmetic operation; the per-op wrappers only differ in how they
/// form `n`, `e0`, and `ideal`.
#[must_use]
pub fn round_exact(
    neg: bool,
    n: BigUint,
    e0: i32,
    extra_sticky: bool,
    ideal: i32,
    fmt: Format,
    rm: RoundingMode,
) -> Rounded {
    let prec = fmt.precision;
    let qmin = fmt.qmin();
    let qmax = fmt.qmax();

    // Exact zero: value is ±0 at the ideal exponent clamped into range.
    if n == BigUint::ZERO && !extra_sticky {
        let exp = ideal.clamp(qmin, qmax);
        return Rounded {
            value: Expect::Finite {
                neg,
                coeff: BigUint::ZERO,
                exp,
            },
            status: Status::OK,
        };
    }

    let d = digit_count(&n);
    // Adjusted exponent of the exact value as if precision were
    // unbounded — the GDA tininess test keys on this (detected before
    // rounding), not on the rounded result's exponent. A product like
    // `(10^-6142) · 0.1` lands just below 10^Emin and is tiny even
    // though rounding bumps it back up to exactly 10^Emin.
    let adjusted_unbounded = e0 + i32::try_from(d).expect("digit count fits i32") - 1;
    // Digits to drop: enough for precision, and enough to lift the
    // exponent to `qmin` (subnormal / tiny path).
    let drop_prec = i64::from(d).saturating_sub(i64::from(prec)).max(0);
    let drop_qmin = i64::from(qmin - e0).max(0);
    let drop = drop_prec.max(drop_qmin);
    let drop_u32 = u32::try_from(drop).expect("drop fits u32");

    let (mut kept, exact) = if drop == 0 {
        (n, !extra_sticky)
    } else {
        let divisor = pow10(drop_u32);
        let q = &n / &divisor;
        let rem = n % &divisor;
        // First dropped digit and the sticky bit below it.
        let (round_digit, low_sticky) = if drop_u32 == 0 {
            (0u8, false)
        } else {
            let hp = pow10(drop_u32 - 1);
            let rd = (&rem / &hp)
                .to_str_radix(10)
                .parse::<u8>()
                .expect("single digit");
            let low = &rem % &hp;
            (rd, low != BigUint::ZERO)
        };
        let sticky = low_sticky || extra_sticky;
        let last_lsb = (&q % BigUint::from(10u32))
            .to_str_radix(10)
            .parse::<u8>()
            .expect("single digit");
        let mut k = q;
        if round_up_decision(rm, neg, last_lsb, round_digit, sticky) {
            k += 1u32;
        }
        let is_exact = rem == BigUint::ZERO && !extra_sticky;
        (k, is_exact)
    };

    let mut exp = e0 + i32::try_from(drop).expect("drop fits i32");

    // Carry-out of the rounding (e.g. 9...9 + 1 -> 10...0): one extra
    // digit; shed the trailing zero and bump the exponent.
    if digit_count(&kept) > prec {
        kept /= 10u32;
        exp += 1;
    }

    // High-exponent clamp: pad trailing zeros (value-preserving) to pull
    // the exponent down into range while staying within `precision`.
    while exp > qmax && digit_count(&kept) < prec && kept != BigUint::ZERO {
        kept *= 10u32;
        exp -= 1;
    }
    if exp > qmax {
        // Genuine overflow. Disposition by rounding mode and sign.
        let to_inf = match rm {
            RoundingMode::NearestEven | RoundingMode::NearestAway => true,
            RoundingMode::TowardZero => false,
            RoundingMode::TowardPositive => !neg,
            RoundingMode::TowardNegative => neg,
        };
        if to_inf {
            return Rounded {
                value: Expect::Infinity { neg },
                status: Status::OVERFLOW | Status::INEXACT,
            };
        }
        return Rounded {
            value: Expect::Finite {
                neg,
                coeff: &pow10(prec) - 1u32,
                exp: qmax,
            },
            status: Status::OVERFLOW | Status::INEXACT,
        };
    }

    let mut status = if exact { Status::OK } else { Status::INEXACT };

    if exact {
        // Cohort selection toward the ideal exponent, staying inside the
        // representable window `[qmin, qmax]`. Pad trailing zeros to
        // lower the exponent toward `ideal`; strip trailing zeros to
        // raise it toward `ideal`. The value is unchanged either way,
        // and crucially the exponent must not leave `[qmin, qmax]` (a
        // pad below `qmin` followed by the clamp below would otherwise
        // change the value).
        while exp > ideal && exp > qmin && digit_count(&kept) < prec && kept != BigUint::ZERO {
            kept *= 10u32;
            exp -= 1;
        }
        while exp < ideal
            && exp < qmax
            && (&kept % BigUint::from(10u32)) == BigUint::ZERO
            && kept != BigUint::ZERO
        {
            kept /= 10u32;
            exp += 1;
        }
    }

    if exp < qmin {
        exp = qmin;
    }

    // Underflow: the exact result is tiny (its unbounded adjusted
    // exponent is below `emin`) and inexact. decTest raises Underflow
    // only together with Inexact, never on an exact subnormal.
    let tiny = kept == BigUint::ZERO || adjusted_unbounded < fmt.emin;
    if tiny && !exact {
        status |= Status::UNDERFLOW;
    }

    Rounded {
        value: Expect::Finite {
            neg,
            coeff: kept,
            exp,
        },
        status,
    }
}

// ---------------------------------------------------------------------------
// Per-operation oracles
//
// `add`/`sub`/`mul`/`fma` form the exact integer result; `div`/`sqrt`
// expand to `precision + 2` digits with an exact remainder. All route
// through `round_exact`, so all are exact correctly-rounded oracles.

fn aligned_sum(a: &Dec, b: &Dec) -> (bool, BigUint, i32) {
    // value = sa·Ca·10^ea + sb·Cb·10^eb, aligned to e = min(ea, eb).
    let e = a.exp.min(b.exp);
    let ca = &a.coeff * pow10(u32::try_from(a.exp - e).expect("fits"));
    let cb = &b.coeff * pow10(u32::try_from(b.exp - e).expect("fits"));
    // Signed combine via magnitude comparison (BigUint is unsigned).
    match (a.neg, b.neg) {
        (false, false) => (false, ca + cb, e),
        (true, true) => (true, ca + cb, e),
        (false, true) => {
            if ca >= cb {
                (false, ca - cb, e)
            } else {
                (true, cb - ca, e)
            }
        }
        (true, false) => {
            if cb >= ca {
                (false, cb - ca, e)
            } else {
                (true, ca - cb, e)
            }
        }
    }
}

/// Sign of an exact-zero additive result per IEEE 754-2019 §6.3.
///
/// When both addends are zero with the *same* sign, the result keeps
/// that sign (`(-0) + (-0) = -0`). Otherwise — opposite-sign zeros, or
/// the exact cancellation of two non-zero operands — the result is
/// `+0`, except under `roundTowardNegative` where it is `-0`.
fn additive_zero_sign(
    a_neg: bool,
    a_zero: bool,
    b_neg: bool,
    b_zero: bool,
    rm: RoundingMode,
) -> bool {
    if a_zero && b_zero && a_neg == b_neg {
        a_neg
    } else {
        matches!(rm, RoundingMode::TowardNegative)
    }
}

/// Correctly-rounded `a + b`.
#[must_use]
pub fn add(a: &Dec, b: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let ideal = a.exp.min(b.exp);
    let (mut neg, mag, e0) = aligned_sum(a, b);
    if mag == BigUint::ZERO {
        neg = additive_zero_sign(a.neg, a.is_zero(), b.neg, b.is_zero(), rm);
    }
    round_exact(neg, mag, e0, false, ideal, fmt, rm)
}

/// Correctly-rounded `a - b`.
#[must_use]
pub fn sub(a: &Dec, b: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let nb = Dec {
        neg: !b.neg,
        coeff: b.coeff.clone(),
        exp: b.exp,
    };
    add(a, &nb, fmt, rm)
}

/// Correctly-rounded `a × b`.
#[must_use]
pub fn mul(a: &Dec, b: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let neg = a.neg ^ b.neg;
    let mag = &a.coeff * &b.coeff;
    let e0 = a.exp + b.exp;
    round_exact(neg, mag, e0, false, e0, fmt, rm)
}

/// Correctly-rounded `a × b + c` with a single rounding (FMA).
#[must_use]
pub fn fma(a: &Dec, b: &Dec, c: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let prod = Dec {
        neg: a.neg ^ b.neg,
        coeff: &a.coeff * &b.coeff,
        exp: a.exp + b.exp,
    };
    let ideal = prod.exp.min(c.exp);
    let prod_zero = prod.coeff == BigUint::ZERO;
    let (mut neg, mag, e0) = aligned_sum(&prod, c);
    if mag == BigUint::ZERO {
        neg = additive_zero_sign(prod.neg, prod_zero, c.neg, c.is_zero(), rm);
    }
    round_exact(neg, mag, e0, false, ideal, fmt, rm)
}

/// Correctly-rounded `a ÷ b` for finite non-zero `b`.
///
/// Expands the quotient to `precision + 2` significant digits with an
/// exact integer remainder; the remainder being non-zero is the exact
/// sticky bit, so `round_exact` delivers the truly correctly-rounded
/// value (not a tolerance-bounded approximation).
#[must_use]
pub fn div(a: &Dec, b: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let neg = a.neg ^ b.neg;
    if a.is_zero() {
        // 0 / finite-nonzero = signed zero, ideal exponent ea - eb.
        return round_exact(neg, BigUint::ZERO, 0, false, a.exp - b.exp, fmt, rm);
    }
    let ideal = a.exp - b.exp;
    // Choose a working exponent low enough that the integer quotient has
    // at least precision + 2 digits, so the round + sticky decision is
    // exact. work_exp is the exponent of the generated integer N:
    //   value = (Ca·10^ea) / (Cb·10^eb)
    //         = (Ca / Cb) · 10^(ea-eb)
    // Scale numerator by 10^shift so floor division yields >= prec+2
    // digits; then N has exponent (ea - eb - shift).
    let da = digit_count(&a.coeff);
    let db = digit_count(&b.coeff);
    // (da - db) approximates digits(Ca/Cb); pad to prec + 3 for safety.
    let want = i64::from(fmt.precision) + 3;
    let have = i64::from(da) - i64::from(db);
    let shift = u32::try_from((want - have).max(1)).expect("shift fits u32");
    let num = &a.coeff * pow10(shift);
    let n = &num / &b.coeff;
    let rem = &num % &b.coeff;
    let work_exp = ideal - i32::try_from(shift).expect("shift fits i32");
    round_exact(neg, n, work_exp, rem != BigUint::ZERO, ideal, fmt, rm)
}

/// Correctly-rounded `sqrt(x)` for finite `x >= 0`.
///
/// Computes `floor(sqrt(x · 10^(2k)))` with `k` large enough that the
/// integer root has `>= precision + 2` digits; the root being inexact
/// (its square differing from the scaled radicand) is the exact sticky
/// bit. The GDA ideal exponent for square root is `floor(e / 2)`.
#[must_use]
pub fn sqrt(x: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    if x.is_zero() {
        // sqrt(±0) = ±0 with exponent floor(e / 2).
        return round_exact(x.neg, BigUint::ZERO, 0, false, x.exp.div_euclid(2), fmt, rm);
    }
    let ideal = x.exp.div_euclid(2);
    // value = sqrt(Cx) · 10^(ex/2). Scale the radicand by 10^(2s) so the
    // integer square root carries >= prec + 3 digits. Keep `ex - 2s`
    // even so the 10-power is an exact 10^((ex-2s)/2) on the root.
    let dx = digit_count(&x.coeff);
    // digits(sqrt(Cx)) ~= ceil(dx / 2); pad to prec + 3.
    let want = 2 * (i64::from(fmt.precision) + 3);
    let s_raw = ((want - i64::from(dx)).max(2) + 1) / 2;
    let mut two_s = u32::try_from(2 * s_raw).expect("scale fits u32");
    // Make (x.exp - 2s) even so 10^((x.exp - 2s)/2) is exact.
    if (x.exp - i32::try_from(two_s).expect("fits")).rem_euclid(2) != 0 {
        two_s += 1;
    }
    let radicand = &x.coeff * pow10(two_s);
    let root = radicand.sqrt();
    let exact_sq = &root * &root == radicand;
    let work_exp = (x.exp - i32::try_from(two_s).expect("fits")) / 2;
    round_exact(false, root, work_exp, !exact_sq, ideal, fmt, rm)
}

/// IEEE 754-2019 remainder `x REM y` (the round-to-nearest-even
/// variant, decTest `remainder`). `r = x − n·y` where `n` is `x/y`
/// rounded to the nearest integer, ties to even. The result is
/// *always exact* (a difference of scaled integers), with
/// `|r| ≤ |y|/2`; the GDA ideal exponent is `min(exp x, exp y)`.
/// Caller guarantees `y` is finite and non-zero.
#[must_use]
pub fn rem(x: &Dec, y: &Dec, fmt: Format, rm: RoundingMode) -> Rounded {
    let e = x.exp.min(y.exp);
    // Magnitudes scaled to the common exponent `e`.
    let mx = &x.coeff * pow10(u32::try_from(x.exp - e).expect("fits"));
    let my = &y.coeff * pow10(u32::try_from(y.exp - e).expect("fits"));
    if x.is_zero() {
        // 0 REM y = 0 with x's sign, ideal exponent min(ex, ey).
        return round_exact(x.neg, BigUint::ZERO, 0, false, e, fmt, rm);
    }
    // GDA `remainder` Division_impossible: if the truncated integer
    // quotient has more than `precision` digits, the operation is
    // undefined and yields NaN + Invalid_operation
    // (speleotrove.com/decimal/daops.html#refrema). decTest and
    // `ferrodec` both do this; the oracle must too rather than
    // returning the bare mathematical remainder.
    let q = &mx / &my;
    if digit_count(&q) > fmt.precision {
        return Rounded {
            value: Expect::Nan,
            status: Status::INVALID,
        };
    }
    let r0 = &mx % &my;
    let two_r = &r0 * 2u32;
    let n = match two_r.cmp(&my) {
        Ordering::Less => q,
        Ordering::Greater => q + 1u32,
        Ordering::Equal => {
            if (&q % 2u32) == BigUint::ZERO {
                q
            } else {
                q + 1u32
            }
        }
    };
    // n·y has the same sign as x (sign(n)=sx^sy, sign(y)=sy ⇒ sx), so
    // r = x − n·y is a same-sign subtraction of magnitudes at exp `e`.
    let n_y = &n * &my;
    let (neg, mag) = if mx >= n_y {
        (x.neg, &mx - &n_y)
    } else {
        (!x.neg, &n_y - &mx)
    };
    let neg = if mag == BigUint::ZERO { x.neg } else { neg };
    round_exact(neg, mag, e, false, e, fmt, rm)
}

// ---------------------------------------------------------------------------
// Hand-verified unit vectors
//
// A wrong oracle gives false confidence, so it is pinned against
// hand-worked cases before any property test consumes it: the
// preferred-exponent cohort rules, every rounding mode at a tie, the
// 9...9 + 1 carry-out, subnormal underflow, and overflow disposition.

#[cfg(test)]
mod tests {
    use super::*;

    const D128: Format = Format::DECIMAL128;

    fn d(s: &str) -> Dec {
        parse_decimal(s).expect("finite literal")
    }

    fn ne() -> RoundingMode {
        RoundingMode::NearestEven
    }

    #[test]
    fn parse_preserves_cohort() {
        let x = d("1.00");
        assert_eq!(x.coeff, BigUint::from(100u32));
        assert_eq!(x.exp, -2);
        let y = d("0.001");
        assert_eq!(y.coeff, BigUint::from(1u32));
        assert_eq!(y.exp, -3);
        let z = d("1.234E-100");
        assert_eq!(z.coeff, BigUint::from(1234u32));
        assert_eq!(z.exp, -103);
        assert!(parse_decimal("NaN").is_none());
        assert!(parse_decimal("-Infinity").is_none());
    }

    #[test]
    fn add_preserves_preferred_exponent() {
        // 1.0 + 2.0 = 3.0 : ideal exponent min(-1,-1) = -1.
        let r = add(&d("1.0"), &d("2.0"), D128, ne());
        assert_eq!(r.decimal_string(), "30E-1");
        assert!(r.status.is_ok());
    }

    #[test]
    fn mul_preferred_exponent_is_sum() {
        // 1.20 * 1.20 = 1.4400 : ideal exponent -2 + -2 = -4.
        let r = mul(&d("1.20"), &d("1.20"), D128, ne());
        assert_eq!(r.decimal_string(), "14400E-4");
        assert!(r.status.is_ok());
    }

    #[test]
    fn mul_strips_to_ideal_when_exact() {
        // 2 * 0.5 = 1.0 : ideal exponent 0 + -1 = -1, value 10E-1.
        let r = mul(&d("2"), &d("0.5"), D128, ne());
        assert_eq!(r.decimal_string(), "10E-1");
    }

    #[test]
    fn round_half_even_tie_breaks_to_even() {
        // 34 digits of 1 then a trailing 5: exact tie at precision.
        let big = format!("{}5", "1".repeat(34));
        let r = round_exact(
            false,
            BigUint::parse_bytes(big.as_bytes(), 10).unwrap(),
            0,
            false,
            0,
            D128,
            ne(),
        );
        // 35-digit ...1115 ties; last kept digit 1 is odd -> round up.
        if let Expect::Finite { coeff, .. } = &r.value {
            assert_eq!(digit_count(coeff), 34);
        } else {
            panic!("finite");
        }
        assert!(r.status.inexact());
    }

    #[test]
    fn carry_out_grows_then_sheds_a_digit() {
        // (10^34 - 1) rounded up by a non-zero tail -> 10^34, one extra
        // digit dropped, exponent bumped.
        let nines = "9".repeat(35); // 35 nines: drop 1, rounds up
        let r = round_exact(
            false,
            BigUint::parse_bytes(nines.as_bytes(), 10).unwrap(),
            0,
            false,
            0,
            D128,
            ne(),
        );
        if let Expect::Finite { coeff, exp, .. } = &r.value {
            assert_eq!(coeff, &(pow10(33))); // 1 followed by 33 zeros
            assert_eq!(*exp, 2);
        } else {
            panic!("finite");
        }
        assert!(r.status.inexact());
    }

    #[test]
    fn overflow_disposition_by_mode() {
        // A full-precision coefficient one decade above MAX: no padding
        // room (already `precision` digits) -> genuine overflow. Mode
        // picks Inf vs MAX.
        let over = &pow10(34) - 1u32; // 34 nines
        let huge_exp = D128.qmax() + 1;
        let to_nearest = round_exact(false, over.clone(), huge_exp, false, huge_exp, D128, ne());
        assert!(matches!(to_nearest.value, Expect::Infinity { neg: false }));
        assert!(to_nearest.status.overflow());

        let truncate = round_exact(
            false,
            over,
            huge_exp,
            false,
            huge_exp,
            D128,
            RoundingMode::TowardZero,
        );
        match truncate.value {
            Expect::Finite { coeff, exp, .. } => {
                assert_eq!(coeff, &pow10(34) - 1u32);
                assert_eq!(exp, D128.qmax());
            }
            Expect::Infinity { .. } => panic!("toward-zero clamps to MAX"),
            Expect::Nan => panic!("overflow is never NaN"),
        }
        assert!(truncate.status.overflow());
    }

    #[test]
    fn subnormal_inexact_raises_underflow() {
        // 1.5e-6176 rounds to the smallest subnormal and is inexact.
        let r = round_exact(false, BigUint::from(15u32), -6177, false, -6177, D128, ne());
        assert!(r.status.inexact());
        assert!(r.status.underflow());
    }

    #[test]
    fn div_is_exact_correctly_rounded() {
        // 1 / 3 = 0.333...3 (34 threes), inexact, ideal exponent 0 - 0.
        let r = div(&d("1"), &d("3"), D128, ne());
        if let Expect::Finite { coeff, exp, neg } = &r.value {
            assert!(!neg);
            assert_eq!(digit_count(coeff), 34);
            assert_eq!(coeff.to_str_radix(10), "3".repeat(34));
            assert_eq!(*exp, -34);
        } else {
            panic!("finite");
        }
        assert!(r.status.inexact());
        // 1 / 2 = 0.5 exactly, ideal exponent 0.
        let h = div(&d("1"), &d("2"), D128, ne());
        assert_eq!(h.decimal_string(), "5E-1");
        assert!(h.status.is_ok());
    }

    #[test]
    fn sqrt_is_exact_correctly_rounded() {
        // sqrt(2) inexact; sqrt(9) = 3 exact with ideal exponent 0.
        let two = sqrt(&d("2"), D128, ne());
        assert!(two.status.inexact());
        if let Expect::Finite { coeff, .. } = &two.value {
            assert_eq!(digit_count(coeff), 34);
            assert!(coeff.to_str_radix(10).starts_with("1414213562373095"));
        } else {
            panic!("finite");
        }
        let nine = sqrt(&d("9"), D128, ne());
        assert_eq!(nine.decimal_string(), "3E0");
        assert!(nine.status.is_ok());
    }

    #[test]
    fn rem_is_exact_ieee_remainder() {
        // 10 REM 3 = 1 (n=3); 7 REM 2 = -1 (n=4, ties-to-even up);
        // 5.5 REM 2 = -0.5 (n=3? 5.5/2=2.75 -> 3; 5.5-6=-0.5).
        let r = rem(&d("10"), &d("3"), D128, ne());
        assert_eq!(r.decimal_string(), "1E0");
        assert!(r.status.is_ok());
        let r = rem(&d("7"), &d("2"), D128, ne());
        // 7/2 = 3.5 ties to even -> 4; 7 - 8 = -1.
        assert_eq!(r.decimal_string(), "-1E0");
        let r = rem(&d("5.5"), &d("2"), D128, ne());
        // ideal exponent min(-1, 0) = -1; 5.5 - 3*2 = -0.5.
        assert_eq!(r.decimal_string(), "-5E-1");
    }

    #[test]
    fn directional_modes_do_not_cross_the_true_value() {
        // 2 / 3 = 0.666...  toward zero truncates, toward +inf bumps.
        let down = div(&d("2"), &d("3"), D128, RoundingMode::TowardZero);
        let up = div(&d("2"), &d("3"), D128, RoundingMode::TowardPositive);
        if let (Expect::Finite { coeff: cd, .. }, Expect::Finite { coeff: cu, .. }) =
            (&down.value, &up.value)
        {
            assert_eq!(cu, &(cd + 1u32));
        } else {
            panic!("finite");
        }
    }
}
