//! Exact decimal significant-digit rounding shared by the Arb frozen
//! corpus consumers (fd-cb6, ADR-0026; the keystone fd-tgg unit-tests).
//!
//! The proof tier turns an Arb (and, in Phase 3, MPFR) high-precision
//! result into a frozen correctly-rounded decimal value by rounding to
//! `prec` significant digits, ties to even. That decimal step is the
//! single keystone the whole tier's correctness rests on. It lived
//! inline in the `mpfr-gate` test, untestable without `rug`/MPFR; it
//! is hoisted here so a default-on meta-test
//! (`tests/round_dec.rs`) can exercise it in lockstep with the Python
//! `round_half_even_sig` in `tools/gen_transcend_vectors.py`, against a
//! shared committed case table. Pure `std`: no `rug`, no C-FFI, no
//! oracle in this path.

/// Decimal value as (negative, significant digits big-endian, power
/// of ten of the last digit). `digits` has no leading zeros; the
/// number is `(-1)^neg * digits * 10^exp`. Zero is `(false, [], 0)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dec {
    /// `true` for a negative value.
    pub neg: bool,
    /// Significant digits, most significant first, no leading zeros
    /// and (post-canonicalisation) no trailing zeros.
    pub digits: Vec<u8>,
    /// Power of ten of the least-significant digit.
    pub exp: i64,
}

/// Parse a decimal string (`[-+]?d*[.d*]?([eE]exp)?`) into canonical
/// [`Dec`] form: leading and trailing zeros stripped, the
/// least-significant digit's exponent tracked. Zero collapses to
/// `(false, [], 0)`.
///
/// # Panics
///
/// Panics if the exponent suffix is not a valid `i64`. The corpus and
/// the case table are checked-in data, so a parse failure is a real
/// breakage, not a runtime condition.
#[must_use]
pub fn parse_dec(s: &str) -> Dec {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (mant, e) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().expect("exponent")),
        None => (body, 0),
    };
    let (int_part, frac_part) = match mant.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mant, ""),
    };
    let mut digits: Vec<u8> = Vec::with_capacity(int_part.len() + frac_part.len());
    for c in int_part.bytes().chain(frac_part.bytes()) {
        digits.push(c - b'0');
    }
    // exponent of the least-significant digit
    let mut exp = e - frac_part.len() as i64;
    // strip leading zeros
    while digits.len() > 1 && digits[0] == 0 {
        digits.remove(0);
    }
    // strip trailing zeros (raising the exponent), canonical form
    while digits.len() > 1 && *digits.last().unwrap() == 0 {
        digits.pop();
        exp += 1;
    }
    if digits == [0] {
        return Dec {
            neg: false,
            digits: Vec::new(),
            exp: 0,
        };
    }
    Dec { neg, digits, exp }
}

/// IEEE 754-2019 §4.3 rounding direction. A local mirror of the
/// kernel's `RoundingMode` so this module stays a self-contained
/// tested unit with no crate coupling; consumers map their
/// `RoundingMode` onto it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    /// To nearest, ties to even (the corpus's historical default).
    NearestEven,
    /// To nearest, ties away from zero.
    NearestAway,
    /// Toward zero (truncate the discarded digits).
    TowardZero,
    /// Toward positive infinity (ceiling).
    TowardPositive,
    /// Toward negative infinity (floor).
    TowardNegative,
}

/// Apply the rounding decision to `kept` and re-canonicalise: handle
/// the all-nines carry, strip trailing zeros (raising `exp`), and
/// collapse an all-zero result to canonical zero. The carry branch is
/// the bug-prone part the fd-tgg case table pins, so both `round_sig`
/// and `round_directed_sig` share this one implementation of it.
fn finish(neg: bool, mut kept: Vec<u8>, mut exp: i64, round_up: bool, prec: usize) -> Dec {
    if round_up {
        let mut i = kept.len();
        loop {
            if i == 0 {
                // All kept digits were 9: 999..9 + 1 carries into a
                // new leading digit. Inserting a high digit does not
                // move the least-significant digit's exponent; only
                // re-trimming the now-extra low digit raises it by
                // one (10000000 @ e-7  ->  1000000 @ e-6, i.e. 1.0).
                kept.insert(0, 1);
                if kept.len() > prec {
                    kept.pop();
                    exp += 1;
                }
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    while kept.len() > 1 && *kept.last().unwrap() == 0 {
        kept.pop();
        exp += 1;
    }
    if kept.iter().all(|&x| x == 0) {
        return Dec {
            neg: false,
            digits: Vec::new(),
            exp: 0,
        };
    }
    Dec {
        neg,
        digits: kept,
        exp,
    }
}

/// Round `d` to at most `prec` significant digits, ties to even,
/// returned in the same canonical (trailing-zero-stripped) form so
/// two equal values compare equal. Thin wrapper over
/// [`round_directed_sig`] with [`Round::NearestEven`]; kept as the
/// named keystone the proof tier and the fd-tgg meta-test reference.
#[must_use]
pub fn round_sig(d: &Dec, prec: usize) -> Dec {
    round_directed_sig(d, prec, Round::NearestEven)
}

/// Round `d` to at most `prec` significant digits under `mode`,
/// returned in canonical (trailing-zero-stripped) form. The directed
/// modes round on the *sign-aware magnitude*: ceiling rounds a
/// positive value's discarded remainder up but truncates a negative
/// one, and floor is its mirror; truncation never increments;
/// nearest-away rounds an exact half away from zero. A value already
/// exact at `prec` significant digits is returned unchanged for every
/// mode.
#[must_use]
pub fn round_directed_sig(d: &Dec, prec: usize, mode: Round) -> Dec {
    if d.digits.len() <= prec {
        return Dec {
            neg: d.neg,
            digits: d.digits.clone(),
            exp: d.exp,
        };
    }
    let kept: Vec<u8> = d.digits[..prec].to_vec();
    let dropped_exp = d.exp + (d.digits.len() - prec) as i64;
    let next = d.digits[prec];
    let sticky = d.digits[prec + 1..].iter().any(|&x| x != 0);
    // Any discarded value at all (the directed predicate); a tie is
    // `next == 5 && !sticky`.
    let has_rem = next != 0 || sticky;
    let last_odd = kept.last().is_some_and(|&l| l % 2 == 1);
    let round_up = match mode {
        Round::NearestEven => next > 5 || (next == 5 && (sticky || last_odd)),
        Round::NearestAway => next >= 5,
        Round::TowardZero => false,
        Round::TowardPositive => !d.neg && has_rem,
        Round::TowardNegative => d.neg && has_rem,
    };
    finish(d.neg, kept, dropped_exp, round_up, prec)
}

/// Cohort-insensitive value equality: same sign, same significant
/// digits, same least-significant-digit exponent (both sides are
/// canonical, so this is exact decimal-value equality).
#[must_use]
pub fn same_value(a: &Dec, b: &Dec) -> bool {
    a.neg == b.neg && a.digits == b.digits && a.exp == b.exp
}

/// Adjusted decimal exponent: the power of ten of the most-significant
/// digit. Zero has magnitude `0`.
#[must_use]
pub fn decimal_magnitude(input: &Dec) -> i64 {
    if input.digits.is_empty() {
        return 0;
    }
    input.exp + input.digits.len() as i64 - 1
}
