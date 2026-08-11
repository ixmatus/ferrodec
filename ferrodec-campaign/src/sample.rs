//! S1 sample construction and the kernel mirror paths.
//!
//! Each stratum targets one of the admitted thin spots (ADR-0059):
//! high decade trig (full 34 digit coefficients over the decades the
//! sampled corpus never reached), the exp family overflow and
//! underflow edges (the `k · ln 10` amplification band), and the `pow`
//! edge strip (`y · log10 x` near the overflow and underflow
//! boundaries).
//!
//! The **mirror** is the campaign's reconstruction of the kernel's 50
//! digit intermediate through the public `*_extended` entry points.
//! The mirror is only the filter; the certifier always compares Arb
//! truth against the **production** outputs recorded alongside. Any
//! drift between mirror and production is itself recorded (a
//! divergence line, counted), never silently dropped: a wrong mirror
//! must surface as statistics, not as a lost sample.

use crate::prng::Draws;
use ferrodec::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_transcend::extended::Extended;
use ferrodec_transcend::{consts, ln, sincos};

pub const MODES: [RoundingMode; 5] = [
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
    RoundingMode::TowardZero,
];

/// The format a shard targets (fd-4zo.19 S2): drives the sample
/// coefficient width, the boundary drop width, and which crate's
/// production surface is judged. `Decimal32` is deliberately absent:
/// its transcendental correctness is the exhaustive program's
/// (ADR-0033/0034), not a sampling question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetFmt {
    D128,
    D64,
}

impl TargetFmt {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TargetFmt::D128 => "d128",
            TargetFmt::D64 => "d64",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "d128" => TargetFmt::D128,
            "d64" => TargetFmt::D64,
            _ => return None,
        })
    }

    /// Coefficient digits = the boundary measurement's target
    /// precision.
    #[must_use]
    pub fn digits(self) -> u32 {
        match self {
            TargetFmt::D128 => 34,
            TargetFmt::D64 => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Exp,
    Exp2,
    Pow,
    // S2 additions (fd-4zo.19): the rest of the priority set.
    Ln,
    Log10,
    Sinh,
    Cosh,
}

impl Func {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Exp => "exp",
            Func::Exp2 => "exp2",
            Func::Pow => "pow",
            Func::Ln => "ln",
            Func::Log10 => "log10",
            Func::Sinh => "sinh",
            Func::Cosh => "cosh",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "exp" => Func::Exp,
            "exp2" => Func::Exp2,
            "pow" => Func::Pow,
            "ln" => Func::Ln,
            "log10" => Func::Log10,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Stratum {
    /// Trig: decade `e` uniform in `[lo, hi]`, full 34 digit
    /// coefficient (`x = coef · 10^(e-33)`).
    Decades { lo: i32, hi: i32 },
    /// Exp family: 34 digit values inside the per function overflow
    /// band (positive side) or underflow band (negative side).
    ExpEdge,
    /// `pow`: full precision `x`, 16 digit `y` steered so
    /// `y · log10 x` lands in `±[6100, 6152]`.
    PowEdge,
}

impl Stratum {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Stratum::Decades { .. } => "decades",
            Stratum::ExpEdge => "exp-edge",
            Stratum::PowEdge => "pow-edge",
        }
    }
}

/// One generated input (binary functions carry `y`; `d64` shards
/// carry the same value parsed at the target width, exact by
/// construction since the generator draws 16-digit coefficients for
/// them).
pub struct Sample {
    pub x: Decimal128,
    pub x64: Option<ferrodec_decimal64::Decimal64>,
    pub y: Option<Decimal128>,
    pub x_str: String,
    pub y_str: Option<String>,
}

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .expect("generated sample must parse")
        .0
}

/// Overflow / underflow band (thousandths) per exp family function
/// and target format: `(pos_lo, pos_hi, neg_lo, neg_hi)` in units of
/// the argument.
fn exp_bands(func: Func, fmt: TargetFmt) -> (u64, u64, u64, u64) {
    match (func, fmt) {
        // e^x: overflow gate at 14150, underflow at 14221.
        (Func::Exp, TargetFmt::D128) => (14000, 14160, 14100, 14300),
        // 2^x: overflow near 20413, deepest subnormal near 20520.
        (Func::Exp2, TargetFmt::D128) => (20300, 20420, 20350, 20600),
        // sinh/cosh saturate to e^|x|/2: overflow near
        // ln(2*10^6145) = 14150.6 (both signs; sinh is odd, cosh
        // even, so the negative band mirrors the positive one and
        // there is no subnormal side of interest).
        (Func::Sinh | Func::Cosh, TargetFmt::D128) => (14000, 14155, 14000, 14155),
        // Decimal64 e^x: overflow near ln(10^385) = 886.5, deepest
        // subnormal near ln(10^-398) = -916.4.
        (Func::Exp, TargetFmt::D64) => (870, 887, 880, 920),
        _ => unreachable!("exp-edge stratum has no band for this (func, fmt)"),
    }
}

/// Build sample `i`'s input(s) for `(func, stratum, fmt)` from the
/// draw stream. `d64` shards draw 16-digit coefficients and carry
/// the target-width parse alongside the `Decimal128` one.
#[must_use]
pub fn gen_sample(func: Func, stratum: Stratum, fmt: TargetFmt, mut d: Draws) -> Sample {
    let digits = fmt.digits();
    match stratum {
        Stratum::Decades { lo, hi } => {
            let span = u64::try_from(i64::from(hi) - i64::from(lo) + 1).unwrap();
            let e = lo + i32::try_from(d.below(span)).unwrap();
            let coef = d.coefficient(digits);
            let x_str = format!("{coef}E{}", e - (digits as i32 - 1));
            finish(fmt, x_str, None)
        }
        Stratum::ExpEdge => {
            let (pos_lo, pos_hi, neg_lo, neg_hi) = exp_bands(func, fmt);
            let negative = d.next_u64() & 1 == 1;
            let (lo, hi) = if negative {
                (neg_lo, neg_hi)
            } else {
                (pos_lo, pos_hi)
            };
            // whole.milli in the band, then enough more coefficient
            // digits for an exactly `digits`-wide value inside the
            // band (the whole.milli prefix is 7-8 digits at d128
            // bands, 6 at d64's).
            let m = lo * 1000 + d.below((hi - lo) * 1000);
            let prefix_len = m.to_string().len();
            let tail_len = digits as usize - prefix_len;
            let mut tail = String::with_capacity(tail_len);
            for _ in 0..tail_len {
                tail.push(char::from(b'0' + u8::try_from(d.below(10)).unwrap()));
            }
            let sign = if negative { "-" } else { "" };
            let frac_exp = -(i32::try_from(tail_len).unwrap() + 3);
            let x_str = format!("{sign}{m}{tail}E{frac_exp}");
            finish(fmt, x_str, None)
        }
        Stratum::PowEdge => {
            // x = coef · 10^(e-33), e in ±[2, 6000]; y (16 digits)
            // steered so y·log10(x) lands in ±[6100, 6152]. f64
            // steering only: exactness is irrelevant to the aim.
            let mag = 2 + i32::try_from(d.below(5999)).unwrap();
            let e = if d.next_u64() & 1 == 1 { -mag } else { mag };
            let coef = d.coefficient(34);
            let x_str = format!("{coef}E{}", e - 33);
            let lead: f64 = coef[..17].parse::<f64>().unwrap() / 1e16;
            let log10x = f64::from(e) + lead.log10();
            let target = (6100.0 + 52.0 * (d.below(1_000_000) as f64 / 1e6))
                * if d.next_u64() & 1 == 1 { -1.0 } else { 1.0 };
            let y = target / log10x;
            let y_str = format!("{y:.15e}").replace('e', "E");
            assert!(
                matches!(fmt, TargetFmt::D128),
                "pow-edge is a Decimal128 stratum"
            );
            finish(fmt, x_str, Some(y_str))
        }
    }
}

/// Assemble a [`Sample`], parsing at both widths for `d64` shards
/// (exact: the generator drew a 16-digit coefficient).
fn finish(fmt: TargetFmt, x_str: String, y_str: Option<String>) -> Sample {
    let x64 = match fmt {
        TargetFmt::D128 => None,
        TargetFmt::D64 => Some(
            ferrodec_decimal64::Decimal64::parse_str(&x_str, RoundingMode::NearestEven)
                .expect("generated d64 sample must parse")
                .0,
        ),
    };
    Sample {
        x: parse(&x_str),
        x64,
        y: y_str.as_deref().map(parse),
        x_str,
        y_str,
    }
}

/// The kernel's 50 digit intermediate for this sample, via the public
/// extended entry points. `None` when the mirror path cannot produce
/// one (it never fires for in stratum samples; the caller counts it).
#[must_use]
pub fn mirror_extended(func: Func, s: &Sample, fmt: TargetFmt) -> Option<Extended> {
    match fmt {
        TargetFmt::D128 => mirror_extended_at::<Decimal128>(func, s.x, s.y),
        TargetFmt::D64 => mirror_extended_at::<ferrodec_decimal64::Decimal64>(
            func,
            s.x64.expect("d64 shard sample carries x64"),
            None,
        ),
    }
}

fn mirror_extended_at<F: ferrodec_transcend::DecimalFormat>(
    func: Func,
    x: F,
    y: Option<Decimal128>,
) -> Option<Extended> {
    match func {
        Func::Sin => Some(sincos::sincos_extended::<F>(x).0),
        Func::Cos => Some(sincos::sincos_extended::<F>(x).1),
        Func::Tan => {
            let (sin_e, cos_e, _) = sincos::sincos_extended::<F>(x);
            Some(sin_e.div::<F>(cos_e))
        }
        Func::Exp => Some(ferrodec_transcend::exp::exp_extended(
            Extended::from_format(x),
        )),
        Func::Exp2 => Some(ferrodec_transcend::exp::exp_extended(
            Extended::from_format(x).mul(consts::ln2_ext()),
        )),
        Func::Pow => {
            let y = y?;
            Some(ferrodec_transcend::exp::exp_extended(
                Extended::from_format(y).mul(ln::ln_extended::<F>(x)),
            ))
        }
        // S2 additions (fd-4zo.19).
        Func::Ln => Some(ln::ln_extended::<F>(x)),
        Func::Log10 => Some(ln::ln_extended::<F>(x).mul(consts::inv_ln10_ext())),
        Func::Sinh => Some(ferrodec_transcend::hyperbolic::sinh_extended(
            Extended::from_format(x),
        )),
        Func::Cosh => Some(ferrodec_transcend::hyperbolic::cosh_extended(
            Extended::from_format(x),
        )),
    }
}

/// One production evaluation at `rm`. These, not the mirror, are what
/// the certifier judges.
#[must_use]
pub fn production_at(func: Func, s: &Sample, rm: RoundingMode) -> (Decimal128, Status) {
    match func {
        Func::Sin => s.x.sin(rm),
        Func::Cos => s.x.cos(rm),
        Func::Tan => s.x.tan(rm),
        Func::Exp => s.x.exp(rm),
        Func::Exp2 => s.x.exp2(rm),
        Func::Pow => s.x.pow(s.y.expect("pow sample carries y"), rm),
        Func::Ln => s.x.ln(rm),
        Func::Log10 => s.x.log10(rm),
        Func::Sinh => s.x.sinh(rm),
        Func::Cosh => s.x.cosh(rm),
    }
}

/// Decimal64 production evaluation for `d64` shards (fd-4zo.19):
/// only the S2 priority set (`exp`, `sin`, `cos`) is wired; the
/// launcher never schedules other functions at this width.
pub fn production_at_d64(
    func: Func,
    s: &Sample,
    rm: RoundingMode,
) -> (ferrodec_decimal64::Decimal64, Status) {
    let x = s.x64.expect("d64 shard sample carries x64");
    match func {
        Func::Sin => x.sin(rm),
        Func::Cos => x.cos(rm),
        Func::Exp => x.exp(rm),
        other => unreachable!("d64 production not wired for {other:?}"),
    }
}

/// The `NearestEven` production result rendered for the sweep's class
/// gate and divergence check: (display string, is-nonnormal).
#[must_use]
pub fn production_ne(fmt: TargetFmt, func: Func, s: &Sample) -> (String, bool) {
    match fmt {
        TargetFmt::D128 => {
            let v = production_at(func, s, RoundingMode::NearestEven).0;
            (
                format!("{v}"),
                !v.is_finite() || v.is_zero() || v.is_subnormal(),
            )
        }
        TargetFmt::D64 => {
            let v = production_at_d64(func, s, RoundingMode::NearestEven).0;
            (
                format!("{v}"),
                !v.is_finite() || v.is_zero() || v.is_subnormal(),
            )
        }
    }
}

/// The production outputs across all five rounding modes, in `MODES`
/// order. Survivor line rendering only: the sweep's hot path pays for
/// exactly one production call per sample (the calibration finding
/// that rescoped fd-4zo.3; four extra modes per sample tripled the
/// cost for data only survivors need).
#[must_use]
pub fn production_outputs(func: Func, s: &Sample) -> [(Decimal128, Status); 5] {
    MODES.map(|rm| production_at(func, s, rm))
}

/// [`production_outputs`] rendered as the TSV's `value#status` cells,
/// format dispatched, so the sweep's line writer stays width
/// agnostic.
#[must_use]
pub fn production_cells(fmt: TargetFmt, func: Func, s: &Sample) -> [String; 5] {
    match fmt {
        TargetFmt::D128 => {
            production_outputs(func, s).map(|(v, st)| format!("{v}#{:02x}", st.bits()))
        }
        TargetFmt::D64 => MODES.map(|rm| {
            let (v, st) = production_at_d64(func, s, rm);
            format!("{v}#{:02x}", st.bits())
        }),
    }
}
