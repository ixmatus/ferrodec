//! Runtime arbitrary precision constants on the [`DecBig`] substrate.
//!
//! The unbounded rung of the ferrodec transcendental ladder picks its
//! working precision at run time, so a stored constant table cannot
//! serve it: a table caps the precision the rung can reach. These
//! generators compute each constant the rung needs to any requested
//! depth instead, and each carries an explicit bound on its truncation
//! plus rounding error so the rung can fold that bound into its own
//! error budget.
//!
//! # What a generator returns
//!
//! A generator returns the first `n` significant decimal digits of its
//! constant `c` as the integer `floor(c · 10^(n − 1 − E))`, where
//! `E = floor(log10 c)` is the constant's decimal exponent. Four of the
//! eight constants sit above 1 (`π`, `ln 10`, `e`, `1/ln 2`; `E = 0`,
//! scale `10^(n − 1)`) and four below it (`2/π`, `ln 2`, `tan(π/8)`,
//! `1/ln 10`; `E = −1`, scale `10^n`), so every function states its own
//! scale rather than leaving the reader to infer it. The returned
//! integer always has exactly `n` decimal digits.
//!
//! # Error bounds
//!
//! Each generator documents a bound in units of its last digit. The
//! bounds share a shape: an internal computation at `GUARD = 24` extra
//! digits, whose accumulated error the function bounds at that working
//! scale, then one truncation back to `n` digits. Every working error
//! derived below stays under `10^7` across the supported range of `n`,
//! and the guard divides it by `10^24`, so under `10^-17` of a last
//! digit survives the scaling. The final truncation is the only
//! material term, and it moves the result by less than one. Every
//! generator therefore lands within **1** unit of the last digit, and
//! [`tan_pi_over_eight_digits`] is exact.
//!
//! # The series lemma
//!
//! One private routine sums both odd power reciprocal series, from the
//! definitions
//!
//! ```text
//! atan(1/m)  = Σ (−1)^k / ((2k+1) · m^(2k+1))
//! atanh(1/m) = Σ   1    / ((2k+1) · m^(2k+1))
//! ```
//!
//! in scaled integers at working scale `S`: the powers come from
//! `T_0 = floor(10^S / m)` and `T_(k+1) = floor(T_k / m²)`, and the
//! summed terms are `u_k = floor(T_k / (2k+1))`. Write
//! `t_k = 10^S / m^(2k+1)` for the exact scaled power and
//! `δ_k = t_k − T_k ≥ 0` for its error. Then, for every `m ≥ 2`:
//!
//! * `δ_0 < 1` and `δ_(k+1) < δ_k / m² + 1`, so
//!   `δ_k < m² / (m² − 1) ≤ 4/3`.
//! * The term division truncates by under 1, so each summed term falls
//!   under `4/3 + 1 = 7/3` short of its exact value, and never exceeds
//!   it.
//! * The loop stops at the first `K` with `T_K = 0`, which forces
//!   `t_K = δ_K < 4/3`, so the omitted tail is under
//!   `(4/3) · m²/(m² − 1) ≤ 16/9 < 2`. (For the alternating series the
//!   first omitted term alone bounds the tail, which is smaller still;
//!   the derivation uses the weaker positive series bound for both.)
//! * `T_k` reaches zero once `m^(2k+1) > 10^S`, so
//!   `K ≤ (S · ln 10 / ln m + 1) / 2 ≤ 1.66 · S + 0.5`, the worst case
//!   being `m = 2`.
//!
//! Adding the per term shortfalls and the tail, the routine's result
//! sits within `(7/3)(1.66 S + 0.5) + 2 < 4S + 5` of `f(1/m) · 10^S`.
//! The generators below call that bound `E(S)`; at the contract cap
//! (`S ≤ MAX_DIGITS + GUARD = 100_024`) it stays under `4.1 · 10^5`.
//!
//! # Contract
//!
//! `n` must lie in [`MIN_DIGITS`]`..=`[`MAX_DIGITS`]; outside that range
//! a generator panics. The floor is a contract rather than a
//! mathematical limit (the derivations hold well below it); nothing in
//! the ladder asks for fewer digits than a format carries. The cap
//! keeps the bounds above theorems rather than assurances, since it
//! caps the term counts and with them the working error at a value the
//! guard digits swallow with 17 decimal orders to spare. Raising the
//! cap needs only a recheck of that margin.
//!
//! Run time grows quadratically in `n`, since the divisions are
//! schoolbook in the limb count, and the ladder calls a generator once
//! per escalation. The module optimises for an auditable error argument
//! rather than for speed.
//!
//! # Provenance
//!
//! Machin's formula (`π = 16·atan(1/5) − 4·atan(1/239)`, John Machin,
//! 1706), the two odd power reciprocal series, and `e = Σ 1/k!` are
//! classical. The scaled integer recurrences here were derived from
//! those series, and the error bounds from the recurrences; no
//! implementation was transcribed. Each logarithm identity is verified
//! inside its own doc comment from `atanh(x) = ½ ln((1+x)/(1−x))`.

use crate::decbig::DecBig;

/// Smallest digit count a generator accepts.
pub const MIN_DIGITS: u64 = 8;

/// Largest digit count a generator accepts. See the module's contract
/// section for why the cap exists and what raising it costs.
pub const MAX_DIGITS: u64 = 100_000;

/// Extra digits carried through every internal computation. The module
/// header shows the accumulated working error stays under `10^7`, so
/// 24 guard digits leave under `10^-17` of a last digit behind.
const GUARD: u32 = 24;

/// Which odd power reciprocal series to sum.
#[derive(Clone, Copy)]
enum Series {
    /// `atan(1/m)`: signs alternate.
    Atan,
    /// `atanh(1/m)`: every term adds.
    Atanh,
}

/// Validate a requested digit count and narrow it for the `DecBig`
/// scaling entry points, which take `u32` exponents.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
fn checked_digits(n: u64) -> u32 {
    assert!(
        (MIN_DIGITS..=MAX_DIGITS).contains(&n),
        "bigconst: {n} digits requested, outside the supported range {MIN_DIGITS}..={MAX_DIGITS}"
    );
    u32::try_from(n).expect("digit count fits u32 under the cap")
}

/// `f(1/m) · 10^scale` truncated to an integer, for `f` one of `atan`
/// and `atanh` and `m ≥ 2`. The result sits within `4·scale + 5` of the
/// exact scaled value; the module's series lemma derives that bound.
fn odd_reciprocal_series(m: u32, scale: u32, kind: Series) -> DecBig {
    debug_assert!(m >= 2, "the series needs m >= 2 to converge");
    let m_squared = DecBig::from_u64(u64::from(m) * u64::from(m));
    // `T_0 = floor(10^scale / m)`, then `T_(k+1) = floor(T_k / m²)`.
    let mut power = DecBig::pow10(scale).div_rem(&DecBig::from_u32(m)).0;
    // The alternating case accumulates the two signs separately, since
    // `DecBig` is unsigned. Terms decrease monotonically, so the even
    // indexed sum always dominates the odd indexed one and the final
    // subtraction never underflows.
    let mut positive = DecBig::zero();
    let mut negative = DecBig::zero();
    let mut k: u64 = 0;
    while !power.is_zero() {
        let term = power.div_rem(&DecBig::from_u64(2 * k + 1)).0;
        match kind {
            Series::Atanh => positive = positive.add(&term),
            Series::Atan if k % 2 == 0 => positive = positive.add(&term),
            Series::Atan => negative = negative.add(&term),
        }
        power = power.div_rem(&m_squared).0;
        k += 1;
    }
    positive.sub(&negative)
}

/// `π · 10^scale` truncated to an integer, by Machin's formula. The
/// result sits within `20 · E(scale) = 80·scale + 100` of the exact
/// scaled value: the two series enter multiplied by 16 and by 4.
fn pi_at_scale(scale: u32) -> DecBig {
    let atan_fifth = odd_reciprocal_series(5, scale, Series::Atan);
    let atan_239th = odd_reciprocal_series(239, scale, Series::Atan);
    // 16·atan(1/5) ≈ 3.166 dominates 4·atan(1/239) ≈ 0.0167, so the
    // subtraction stays inside the unsigned domain.
    atan_fifth
        .mul(&DecBig::from_u32(16))
        .sub(&atan_239th.mul(&DecBig::from_u32(4)))
}

/// `e · 10^scale` truncated to an integer, from `e = Σ 1/k!` summed as
/// `T_0 = 10^scale`, `T_k = floor(T_(k−1) / k)`.
///
/// The result sits within `2·scale + 65` of the exact scaled value.
/// Writing `e_k = 10^scale / k!` and `δ_k = e_k − T_k`: `δ_0 = 0` and
/// `δ_k < δ_(k−1)/k + 1 ≤ 2` for `k ≥ 1`, so each term falls under 2
/// short. The loop stops at the first `K` with `T_K = 0`, which needs
/// `K! > 10^scale` and therefore `K ≤ scale + 31`, and leaves a tail
/// under `e_K · 1.1 < 3`. Summing gives `2(scale + 31) + 3`.
fn e_at_scale(scale: u32) -> DecBig {
    let mut term = DecBig::pow10(scale);
    let mut sum = DecBig::zero();
    let mut k: u64 = 1;
    while !term.is_zero() {
        sum = sum.add(&term);
        term = term.div_rem(&DecBig::from_u64(k)).0;
        k += 1;
    }
    sum
}

/// `π` to `n` significant digits: `floor(π · 10^(n − 1))`.
///
/// Machin's formula `π = 16·atan(1/5) − 4·atan(1/239)` runs the scaled
/// series twice at working scale `S = n − 1 + 24`.
///
/// # Error
///
/// The two series contribute `16·E(S)` and `4·E(S)`, so the working
/// error stays under `20·(4S + 5) = 80S + 100`, which the contract cap
/// holds under `8.1 · 10^6`. Dividing by `10^24` leaves under `10^-17`
/// of a last digit; the final truncation moves the result by under one.
/// The returned integer therefore differs from `floor(π · 10^(n − 1))`
/// by at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::pi_digits;
/// assert_eq!(pi_digits(10).to_string(), "3141592653");
/// ```
#[must_use]
pub fn pi_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // π ∈ [1, 10), so the first `n` digits are `floor(π · 10^(n − 1))`.
    pi_at_scale(n - 1 + GUARD).div_rem_pow10(GUARD).0
}

/// `2/π` to `n` significant digits: `floor((2/π) · 10^n)`.
///
/// The Payne-Hanek reduction needs a window of `2/π`, which this
/// derives by dividing into computed `π`:
/// `(2/π) · 10^n = 2 · 10^(n + S) / (π · 10^S)` with `S = n + 24`.
///
/// # Error
///
/// Let `A` be the scaled π from that formula, so `A = π·10^S + θ` with
/// `|θ| ≤ 80S + 100 < 8.1 · 10^6`. Perturbing a quotient's divisor
/// moves it by the quotient's own relative share of the perturbation:
/// `|N/A − N/(π·10^S)| = (N/(π·10^S)) · |θ| / A`, which with
/// `N = 2·10^(n + S)` is `0.64 · 10^n · |θ| / (3 · 10^(n + 24))`, under
/// `10^-17`. The truncating division then moves the result by under
/// one, so the returned integer differs from `floor((2/π) · 10^n)` by
/// at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::two_over_pi_digits;
/// assert_eq!(two_over_pi_digits(10).to_string(), "6366197723");
/// ```
#[must_use]
pub fn two_over_pi_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // 2/π ∈ [0.1, 1), so the first `n` digits are
    // `floor((2/π) · 10^n)`. The guard rides in the π depth rather than
    // in a second truncation: dividing `2 · 10^(n + S)` by `π · 10^S`
    // lands on the output scale directly.
    let scale = n + GUARD;
    let numerator = DecBig::from_u32(2).mul_pow10(n + scale);
    numerator.div_rem(&pi_at_scale(scale)).0
}

/// `ln 2` to `n` significant digits: `floor(ln 2 · 10^n)`.
///
/// From `atanh(x) = ½ ln((1+x)/(1−x))`: at `x = 1/3` the argument of
/// the logarithm is `(4/3)/(2/3) = 2`, so `ln 2 = 2·atanh(1/3)`. The
/// series runs at working scale `S = n + 24`.
///
/// # Error
///
/// The single series enters multiplied by 2, so the working error stays
/// under `2·(4S + 5) = 8S + 10`, which the contract cap holds under
/// `8.1 · 10^5`. Dividing by `10^24` leaves under `10^-18` of a last
/// digit; the final truncation moves the result by under one. The
/// returned integer therefore differs from `floor(ln 2 · 10^n)` by at
/// most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::ln2_digits;
/// assert_eq!(ln2_digits(10).to_string(), "6931471805");
/// ```
#[must_use]
pub fn ln2_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // ln 2 ∈ [0.1, 1), so the first `n` digits are
    // `floor(ln 2 · 10^n)`.
    let scale = n + GUARD;
    odd_reciprocal_series(3, scale, Series::Atanh)
        .mul(&DecBig::from_u32(2))
        .div_rem_pow10(GUARD)
        .0
}

/// `ln 10` to `n` significant digits: `floor(ln 10 · 10^(n − 1))`.
///
/// From `atanh(x) = ½ ln((1+x)/(1−x))`: at `x = 1/2` the argument is
/// `(3/2)/(1/2) = 3`, so `ln 3 = 2·atanh(1/2)`; at `x = 1/19` it is
/// `(20/19)/(18/19) = 10/9`, so `ln(10/9) = 2·atanh(1/19)`. Then
/// `ln 10 = 2·ln 3 + ln(10/9)` because `9 · (10/9) = 10`, which in
/// series form is `4·atanh(1/2) + 2·atanh(1/19)`. Both series run at
/// working scale `S = n − 1 + 24`.
///
/// The decomposition trades a third series against convergence: the
/// `1/2` series is the slowest the module runs (1.66 terms per working
/// digit), and `1/19` converges fast enough that the pair still beats
/// any single atanh argument reachable from `ln 10` alone.
///
/// # Error
///
/// The two series enter multiplied by 4 and by 2, so the working error
/// stays under `6·(4S + 5) = 24S + 30`, which the contract cap holds
/// under `2.5 · 10^6`. Dividing by `10^24` leaves under `10^-17` of a
/// last digit; the final truncation moves the result by under one. The
/// returned integer therefore differs from `floor(ln 10 · 10^(n − 1))`
/// by at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::ln10_digits;
/// assert_eq!(ln10_digits(10).to_string(), "2302585092");
/// ```
#[must_use]
pub fn ln10_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // ln 10 ∈ [1, 10), so the first `n` digits are
    // `floor(ln 10 · 10^(n − 1))`.
    let scale = n - 1 + GUARD;
    let two_ln3 = odd_reciprocal_series(2, scale, Series::Atanh).mul(&DecBig::from_u32(4));
    let ln_ten_ninths = odd_reciprocal_series(19, scale, Series::Atanh).mul(&DecBig::from_u32(2));
    two_ln3.add(&ln_ten_ninths).div_rem_pow10(GUARD).0
}

/// `e` to `n` significant digits: `floor(e · 10^(n − 1))`.
///
/// `e = Σ 1/k!` summed in scaled integers at working scale
/// `S = n − 1 + 24`.
///
/// # Error
///
/// The sum's own bound is `2S + 65` (derived where the summation
/// lives), which the contract cap holds under `2.1 · 10^5`. Dividing by
/// `10^24` leaves under `10^-18` of a last digit; the final truncation
/// moves the result by under one. The returned integer therefore
/// differs from `floor(e · 10^(n − 1))` by at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::e_digits;
/// assert_eq!(e_digits(10).to_string(), "2718281828");
/// ```
#[must_use]
pub fn e_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // e ∈ [1, 10), so the first `n` digits are `floor(e · 10^(n − 1))`.
    e_at_scale(n - 1 + GUARD).div_rem_pow10(GUARD).0
}

/// `tan(π/8)` to `n` significant digits: `floor(tan(π/8) · 10^n)`.
///
/// The half angle identity gives `tan(π/8) = √2 − 1` exactly, so
/// [`DecBig::isqrt`] carries the whole computation.
///
/// # Error
///
/// None: the result is exact. `isqrt(2 · 10^(2n))` is
/// `floor(√(2 · 10^(2n))) = floor(√2 · 10^n)`, and subtracting the
/// integer `10^n` from a floor preserves it, so the returned value is
/// `floor((√2 − 1) · 10^n)` with no guard digits and no truncation
/// error to bound. Guard digits would only add a truncation this route
/// avoids, so the generator carries none.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::tan_pi_over_eight_digits;
/// assert_eq!(tan_pi_over_eight_digits(10).to_string(), "4142135623");
/// ```
#[must_use]
pub fn tan_pi_over_eight_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // tan(π/8) ≈ 0.414 ∈ [0.1, 1), so the first `n` digits are
    // `floor(tan(π/8) · 10^n)`.
    let (root, _) = DecBig::from_u32(2).mul_pow10(2 * n).isqrt();
    root.sub(&DecBig::pow10(n))
}

/// `1/ln 2` to `n` significant digits: `floor((1/ln 2) · 10^(n − 1))`.
///
/// The ladder's base 2 pipelines need the reciprocal directly, and a
/// division into the computed original keeps the whole error argument
/// inside this module (the [`two_over_pi_digits`] pattern):
/// `(1/ln 2) · 10^(n − 1) = 10^(n − 1 + S) / (ln 2 · 10^S)` with
/// `S = n + 24`.
///
/// # Error
///
/// Let `A` be the scaled `ln 2` from the `2·atanh(1/3)` series at
/// working scale `S`, so `A = ln 2 · 10^S − δ` with `0 ≤ δ < 8S + 10`
/// (twice the series lemma's `E(S)`), under `8.1 · 10^5` at the
/// contract cap. Perturbing a quotient's divisor moves it by the
/// quotient's own relative share of the perturbation:
/// `|N/A − N/(ln 2 · 10^S)| = (N/(ln 2 · 10^S)) · δ / A`, which with
/// `N = 10^(n − 1 + S)` is `1.443 · 10^(n − 1) · δ / (0.693 · 10^S)`,
/// under `10^-18`. The truncating division then moves the result by
/// under one, so the returned integer differs from
/// `floor((1/ln 2) · 10^(n − 1))` by at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::inv_ln2_digits;
/// assert_eq!(inv_ln2_digits(10).to_string(), "1442695040");
/// ```
#[must_use]
pub fn inv_ln2_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // 1/ln 2 ≈ 1.443 ∈ [1, 10), so the first `n` digits are
    // `floor((1/ln 2) · 10^(n − 1))`. The guard rides in the divisor
    // depth: dividing `10^(n − 1 + S)` by `ln 2 · 10^S` lands on the
    // output scale directly.
    let scale = n + GUARD;
    let divisor = odd_reciprocal_series(3, scale, Series::Atanh).mul(&DecBig::from_u32(2));
    DecBig::pow10(n - 1 + scale).div_rem(&divisor).0
}

/// `1/ln 10` to `n` significant digits: `floor((1/ln 10) · 10^n)`.
///
/// The decade split of the `exp` pipeline needs the reciprocal
/// directly; as with [`inv_ln2_digits`] the division into the computed
/// original keeps the error argument local:
/// `(1/ln 10) · 10^n = 10^(n + S) / (ln 10 · 10^S)` with `S = n + 24`.
///
/// # Error
///
/// Let `A` be the scaled `ln 10` from the `4·atanh(1/2) + 2·atanh(1/19)`
/// series pair at working scale `S`, so `A = ln 10 · 10^S − δ` with
/// `0 ≤ δ < 24S + 30` (six times the series lemma's `E(S)`), under
/// `2.5 · 10^6` at the contract cap. The divisor perturbation moves the
/// quotient `N/A` with `N = 10^(n + S)` by
/// `0.434 · 10^n · δ / (2.302 · 10^S)`, under `10^-17`. The truncating
/// division then moves the result by under one, so the returned integer
/// differs from `floor((1/ln 10) · 10^n)` by at most **1**.
///
/// # Panics
///
/// Panics when `n` falls outside [`MIN_DIGITS`]`..=`[`MAX_DIGITS`].
///
/// ```
/// use ferrodec_multiword::bigconst::inv_ln10_digits;
/// assert_eq!(inv_ln10_digits(10).to_string(), "4342944819");
/// ```
/// The first `n` decimal digits of `1/π` (ADR-0061, the pi-scaled
/// inverse family's closing constant).
///
/// `1/π ≈ 0.318 ∈ [0.1, 1)`, so the first `n` digits are
/// `floor((1/π) · 10^n)`, computed as a division into the computed
/// `π` at guarded depth: `floor(10^(n + scale − 1) / pi_digits(scale))`
/// with `scale = n + GUARD`. The divisor is `floor(π · 10^(scale − 1))`,
/// within one ulp of `π · 10^(scale − 1)`, so the quotient's error
/// stays below one unit in the `n`-th digit for every `n` the GUARD
/// covers — the same argument [`inv_ln10_digits`] carries.
///
/// ```
/// use ferrodec_multiword::bigconst::inv_pi_digits;
/// assert_eq!(inv_pi_digits(10).to_string(), "3183098861");
/// ```
#[must_use]
pub fn inv_pi_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    let scale = n + GUARD;
    let divisor = pi_digits(u64::from(scale));
    DecBig::pow10(n + scale - 1).div_rem(&divisor).0
}

#[must_use]
pub fn inv_ln10_digits(n: u64) -> DecBig {
    let n = checked_digits(n);
    // 1/ln 10 ≈ 0.434 ∈ [0.1, 1), so the first `n` digits are
    // `floor((1/ln 10) · 10^n)`. The guard rides in the divisor depth,
    // as in [`inv_ln2_digits`].
    let scale = n + GUARD;
    let two_ln3 = odd_reciprocal_series(2, scale, Series::Atanh).mul(&DecBig::from_u32(4));
    let ln_ten_ninths = odd_reciprocal_series(19, scale, Series::Atanh).mul(&DecBig::from_u32(2));
    let divisor = two_ln3.add(&ln_ten_ninths);
    DecBig::pow10(n + scale).div_rem(&divisor).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use core::cmp::Ordering;

    // Oracle pins generated by tools/gen_bigconst_oracle.py (mpmath at
    // `n + 40` and `n + 90` decimal places, cross-checked against each
    // other). Each entry is (digit count, expected digit string).
    const PI_PINS: [(u64, &str); 4] = [
        (50, "31415926535897932384626433832795028841971693993751"),
        (120, "314159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798214808651328230664"),
        (220, "3141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067982148086513282306647093844609550582231725359408128481117450284102701938521105559644622948954930381964428810975665933446"),
        (500, "31415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679821480865132823066470938446095505822317253594081284811174502841027019385211055596446229489549303819644288109756659334461284756482337867831652712019091456485669234603486104543266482133936072602491412737245870066063155881748815209209628292540917153643678925903600113305305488204665213841469519415116094330572703657595919530921861173819326117931051185480744623799627495673518857527248912279381830119491"),
    ];
    const TWO_OVER_PI_PINS: [(u64, &str); 4] = [
        (50, "63661977236758134307553505349005744813783858296182"),
        (120, "636619772367581343075535053490057448137838582961825794990669376235587190536906140360455211065012343824291370907031832147"),
        (220, "6366197723675813430755350534900574481378385829618257949906693762355871905369061403604552110650123438242913709070318321475716473844583146115118696429267993569169598677496363102923109855877012307548695715848695906467734495"),
        (500, "63661977236758134307553505349005744813783858296182579499066937623558719053690614036045521106501234382429137090703183214757164738445831461151186964292679935691695986774963631029231098558770123075486957158486959064677344956096689451604732952045689079902286376184756034761069582448195764374775137634211489239978577360099468939095783844359329238713229962466794585121879779460875152629914626785696415598349655739443993547239679984977150234068471543372447007506864218619014795203895784145903733507223720997"),
    ];
    const LN2_PINS: [(u64, &str); 4] = [
        (50, "69314718055994530941723212145817656807550013436025"),
        (120, "693147180559945309417232121458176568075500134360255254120680009493393621969694715605863326996418687542001481020570685733"),
        (220, "6931471805599453094172321214581765680755001343602552541206800094933936219696947156058633269964186875420014810205706857336855202357581305570326707516350759619307275708283714351903070386238916734711233501153644979552391204"),
        (500, "69314718055994530941723212145817656807550013436025525412068000949339362196969471560586332699641868754200148102057068573368552023575813055703267075163507596193072757082837143519030703862389167347112335011536449795523912047517268157493206515552473413952588295045300709532636664265410423915781495204374043038550080194417064167151864471283996817178454695702627163106454615025720740248163777338963855069526066834113727387372292895649354702576265209885969320196505855476470330679365443254763274495125040606"),
    ];
    const LN10_PINS: [(u64, &str); 4] = [
        (50, "23025850929940456840179914546843642076011014886287"),
        (120, "230258509299404568401799145468436420760110148862877297603332790096757260967735248023599720508959829834196778404228624863"),
        (220, "2302585092994045684017991454684364207601101488628772976033327900967572609677352480235997205089598298341967784042286248633409525465082806756666287369098781689482907208325554680843799894826233198528393505308965377732628846"),
        (500, "23025850929940456840179914546843642076011014886287729760333279009675726096773524802359972050895982983419677840422862486334095254650828067566662873690987816894829072083255546808437998948262331985283935053089653777326288461633662222876982198867465436674744042432743651550489343149393914796194044002221051017141748003688084012647080685567743216228355220114804663715659121373450747856947683463616792101806445070648000277502684916746550586856935673420670581136429224554405758925724208241314695689016758940"),
    ];
    const E_PINS: [(u64, &str); 4] = [
        (50, "27182818284590452353602874713526624977572470936999"),
        (120, "271828182845904523536028747135266249775724709369995957496696762772407663035354759457138217852516642742746639193200305992"),
        (220, "2718281828459045235360287471352662497757247093699959574966967627724076630353547594571382178525166427427466391932003059921817413596629043572900334295260595630738132328627943490763233829880753195251019011573834187930702154"),
        (500, "27182818284590452353602874713526624977572470936999595749669676277240766303535475945713821785251664274274663919320030599218174135966290435729003342952605956307381323286279434907632338298807531952510190115738341879307021540891499348841675092447614606680822648001684774118537423454424371075390777449920695517027618386062613313845830007520449338265602976067371132007093287091274437470472306969772093101416928368190255151086574637721112523897844250569536967707854499699679468644549059879316368892300987931"),
    ];
    const INV_LN2_PINS: [(u64, &str); 4] = [
        (50, "14426950408889634073599246810018921374266459541529"),
        (120, "144269504088896340735992468100189213742664595415298593413544940693110921918118507988552662289350634449699751830965254425"),
        (220, "1442695040888963407359924681001892137426645954152985934135449406931109219181185079885526622893506344496997518309652544255593101687168359642720662158223479336274537369884718493630701387663532015533894318916664837643128615"),
        (500, "14426950408889634073599246810018921374266459541529859341354494069311092191811850798855266228935063444969975183096525442555931016871683596427206621582234793362745373698847184936307013876635320155338943189166648376431286154240474784222894979047950915303513385880549688658930969963680361105110756308441454272158283449418919339085777157900441712802468483413745226951823690112390940344599685399061134217228862780291580106300619767624456526059950737532406256558154759381783052397255107248130771562675458075"),
    ];
    const INV_LN10_PINS: [(u64, &str); 4] = [
        (50, "43429448190325182765112891891660508229439700580366"),
        (120, "434294481903251827651128918916605082294397005803666566114453783165864649208870774729224949338431748318706106744766303733"),
        (220, "4342944819032518276511289189166050822943970058036665661144537831658646492088707747292249493384317483187061067447663037336416792871589639065692210646628122658521270865686703295933708696588266883311636077384905142844348666"),
        (500, "43429448190325182765112891891660508229439700580366656611445378316586464920887077472922494933843174831870610674476630373364167928715896390656922106466281226585212708656867032959337086965882668833116360773849051428443486667686465860851355614821234876534354343573172538356222813956030486466523660955393773561763234319167109914115978949629935124579349263576554690776710824191504799109896749001032775376535702700873285509517314406746979518995135940880404239315188681084025446540897970298632868287626241440"),
    ];
    const TAN_PI_OVER_EIGHT_PINS: [(u64, &str); 4] = [
        (50, "41421356237309504880168872420969807856967187537694"),
        (120, "414213562373095048801688724209698078569671875376948073176679737990732478462107038850387534327641572735013846230912297024"),
        (220, "4142135623730950488016887242096980785696718753769480731766797379907324784621070388503875343276415727350138462309122970249248360558507372126441214970999358314132226659275055927557999505011527820605714701095599716059702745"),
        (500, "41421356237309504880168872420969807856967187537694807317667973799073247846210703885038753432764157273501384623091229702492483605585073721264412149709993583141322266592750559275579995050115278206057147010955997160597027453459686201472851741864088919860955232923048430871432145083976260362799525140798968725339654633180882964062061525835239505474575028775996172983557522033753185701135437460340849884716038689997069900481503054402779031645424782306849293691862158057846311159666871301301561856898723723"),
    ];

    /// Absolute difference, since `DecBig` is unsigned.
    fn abs_diff(a: &DecBig, b: &DecBig) -> DecBig {
        if a.cmp_ref(b) == Ordering::Less {
            b.sub(a)
        } else {
            a.sub(b)
        }
    }

    /// Check a generator against its oracle pins, allowing `slack` units
    /// of the last digit (the generator's documented bound).
    fn check_pins(name: &str, generator: fn(u64) -> DecBig, pins: &[(u64, &str)], slack: u128) {
        for &(n, expected) in pins {
            let want = DecBig::from_ascii_digits(expected.as_bytes());
            assert_eq!(want.decimal_digit_count(), n, "{name}: bad pin string");
            let got = generator(n);
            assert_eq!(got.decimal_digit_count(), n, "{name}({n}) digit count");
            let off = abs_diff(&got, &want)
                .to_u128()
                .expect("difference fits u128");
            assert!(
                off <= slack,
                "{name}({n}) off the oracle by {off} (bound {slack})"
            );
        }
    }

    #[test]
    fn pi_matches_oracle() {
        check_pins("pi_digits", pi_digits, &PI_PINS, 1);
    }

    #[test]
    fn two_over_pi_matches_oracle() {
        check_pins(
            "two_over_pi_digits",
            two_over_pi_digits,
            &TWO_OVER_PI_PINS,
            1,
        );
    }

    #[test]
    fn ln2_matches_oracle() {
        check_pins("ln2_digits", ln2_digits, &LN2_PINS, 1);
    }

    #[test]
    fn ln10_matches_oracle() {
        check_pins("ln10_digits", ln10_digits, &LN10_PINS, 1);
    }

    #[test]
    fn e_matches_oracle() {
        check_pins("e_digits", e_digits, &E_PINS, 1);
    }

    #[test]
    fn tan_pi_over_eight_matches_oracle() {
        // Slack zero: the isqrt route is exact.
        check_pins(
            "tan_pi_over_eight_digits",
            tan_pi_over_eight_digits,
            &TAN_PI_OVER_EIGHT_PINS,
            0,
        );
    }

    #[test]
    fn inv_ln2_matches_oracle() {
        check_pins("inv_ln2_digits", inv_ln2_digits, &INV_LN2_PINS, 1);
    }

    #[test]
    fn inv_ln10_matches_oracle() {
        check_pins("inv_ln10_digits", inv_ln10_digits, &INV_LN10_PINS, 1);
    }

    #[test]
    fn ln2_times_inv_ln2_is_one() {
        // With `P = ln 2 · 10^300 + a` and `Q = (1/ln 2) · 10^299 + b`,
        // where `|a|, |b| < 2` (one unit of documented bound plus one
        // of truncation), the product sits within
        // `2 · 1.443 · 10^299 + 2 · 0.694 · 10^300 + 4 < 1.7 · 10^300`
        // of `10^599`. The assertion allows `2 · 10^300`.
        let product = ln2_digits(300).mul(&inv_ln2_digits(300));
        let target = DecBig::pow10(599);
        let slack = DecBig::from_u32(2).mul_pow10(300);
        let off = abs_diff(&product, &target);
        assert_eq!(off.cmp_ref(&slack), Ordering::Less, "ln2 · (1/ln2) drifted");
    }

    #[test]
    fn ln10_times_inv_ln10_is_one() {
        // With `P = ln 10 · 10^299 + a` and `Q = (1/ln 10) · 10^300 + b`,
        // `|a|, |b| < 2`, the product sits within
        // `2 · 0.435 · 10^300 + 2 · 2.303 · 10^299 + 4 < 1.4 · 10^300`
        // of `10^599`. The assertion allows `2 · 10^300`.
        let product = ln10_digits(300).mul(&inv_ln10_digits(300));
        let target = DecBig::pow10(599);
        let slack = DecBig::from_u32(2).mul_pow10(300);
        let off = abs_diff(&product, &target);
        assert_eq!(
            off.cmp_ref(&slack),
            Ordering::Less,
            "ln10 · (1/ln10) drifted"
        );
    }

    #[test]
    fn pi_times_two_over_pi_is_two() {
        // With `P = π·10^299 + a` and `Q = (2/π)·10^300 + b`, where
        // `|a|, |b| < 2` (one unit of documented bound plus one of
        // truncation), the product is
        // `2·10^599 + a·(2/π)·10^300 + b·π·10^299 + a·b`, so it sits
        // within `2·0.637·10^300 + 2·3.1416·10^299 + 4 < 1.92·10^300`
        // of `2·10^599`. The assertion allows `2·10^300`.
        let product = pi_digits(300).mul(&two_over_pi_digits(300));
        let target = DecBig::from_u32(2).mul_pow10(599);
        let slack = DecBig::from_u32(2).mul_pow10(300);
        let off = abs_diff(&product, &target);
        assert_eq!(off.cmp_ref(&slack), Ordering::Less, "π · (2/π) drifted");
    }

    #[test]
    fn tan_pi_over_eight_squares_to_two() {
        // `T = floor((√2 − 1)·10^300)` exactly, so `S = T + 10^300` is
        // `floor(√2·10^300) = √2·10^300 − f` with `f ∈ [0, 1)`. Then
        // `2·10^600 − S² = 2f√2·10^300 − f²`, which lies in
        // `[0, 2.83·10^300)`. The assertion allows `3·10^300` and also
        // pins the sign, since `S` never exceeds `√2·10^300`.
        let root = tan_pi_over_eight_digits(300).add(&DecBig::pow10(300));
        let square = root.mul(&root);
        let target = DecBig::from_u32(2).mul_pow10(600);
        assert_eq!(
            square.cmp_ref(&target),
            Ordering::Less,
            "the scaled root overshot √2"
        );
        let slack = DecBig::from_u32(3).mul_pow10(300);
        assert_eq!(
            target.sub(&square).cmp_ref(&slack),
            Ordering::Less,
            "the scaled root undershot √2"
        );
    }

    #[test]
    fn deeper_runs_extend_shallower_ones() {
        // A generator's digits are a prefix of its own deeper run: the
        // 120-digit π truncated to 50 digits is the 50-digit π, up to
        // the documented unit. This catches a scale slip that agreeing
        // pins at a single depth could not. Both decimal exponents are
        // covered: π sits above 1, ln 2 below it. The theorem-backed
        // bound is 2 (each side within 1 of its own floor); the ≤ 1
        // asserted here is a deterministic pin of the observed
        // behavior, safe because nothing here is randomized.
        for (name, shallow, deep) in [
            ("pi_digits", pi_digits(50), pi_digits(120)),
            ("ln2_digits", ln2_digits(50), ln2_digits(120)),
            ("inv_ln2_digits", inv_ln2_digits(50), inv_ln2_digits(120)),
            ("inv_ln10_digits", inv_ln10_digits(50), inv_ln10_digits(120)),
            ("inv_pi_digits", inv_pi_digits(50), inv_pi_digits(120)),
        ] {
            let truncated = deep.div_rem_pow10(70).0;
            let off = abs_diff(&shallow, &truncated)
                .to_u128()
                .expect("difference fits u128");
            assert!(off <= 1, "{name}: depths disagree by {off}");
        }
    }

    #[test]
    fn minimum_digit_count_is_supported() {
        assert_eq!(MIN_DIGITS, 8);
        for (name, got) in [
            ("pi_digits", pi_digits(MIN_DIGITS)),
            ("two_over_pi_digits", two_over_pi_digits(MIN_DIGITS)),
            ("ln2_digits", ln2_digits(MIN_DIGITS)),
            ("ln10_digits", ln10_digits(MIN_DIGITS)),
            ("e_digits", e_digits(MIN_DIGITS)),
            (
                "tan_pi_over_eight_digits",
                tan_pi_over_eight_digits(MIN_DIGITS),
            ),
            ("inv_ln2_digits", inv_ln2_digits(MIN_DIGITS)),
            ("inv_ln10_digits", inv_ln10_digits(MIN_DIGITS)),
            ("inv_pi_digits", inv_pi_digits(MIN_DIGITS)),
        ] {
            assert_eq!(got.decimal_digit_count(), MIN_DIGITS, "{name} digit count");
        }
        assert_eq!(pi_digits(MIN_DIGITS).to_string(), "31415926");
        assert_eq!(e_digits(MIN_DIGITS).to_string(), "27182818");
    }

    #[test]
    #[should_panic(expected = "outside the supported range")]
    fn below_the_minimum_panics() {
        let _ = pi_digits(MIN_DIGITS - 1);
    }

    #[test]
    #[should_panic(expected = "outside the supported range")]
    fn above_the_maximum_panics() {
        // Rejected before any work starts, so the test costs nothing.
        let _ = ln10_digits(MAX_DIGITS + 1);
    }
}
