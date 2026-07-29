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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Exp,
    Exp2,
    Pow,
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

/// One generated input (binary functions carry `y`).
pub struct Sample {
    pub x: Decimal128,
    pub y: Option<Decimal128>,
    pub x_str: String,
    pub y_str: Option<String>,
}

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .expect("generated sample must parse")
        .0
}

/// Overflow / underflow band (thousandths) per exp family function:
/// `(pos_lo, pos_hi, neg_lo, neg_hi)` in units of the argument.
fn exp_bands(func: Func) -> (u64, u64, u64, u64) {
    match func {
        // e^x: overflow gate at 14150, underflow at 14221.
        Func::Exp => (14000, 14160, 14100, 14300),
        // 2^x: overflow near 20413, deepest subnormal near 20520.
        Func::Exp2 => (20300, 20420, 20350, 20600),
        _ => unreachable!("exp-edge stratum on a non exp family function"),
    }
}

/// Build sample `i`'s input(s) for `(func, stratum)` from the draw
/// stream.
#[must_use]
pub fn gen_sample(func: Func, stratum: Stratum, mut d: Draws) -> Sample {
    match stratum {
        Stratum::Decades { lo, hi } => {
            let span = u64::try_from(i64::from(hi) - i64::from(lo) + 1).unwrap();
            let e = lo + i32::try_from(d.below(span)).unwrap();
            let coef = d.coefficient(34);
            let x_str = format!("{coef}E{}", e - 33);
            Sample {
                x: parse(&x_str),
                y: None,
                x_str,
                y_str: None,
            }
        }
        Stratum::ExpEdge => {
            let (pos_lo, pos_hi, neg_lo, neg_hi) = exp_bands(func);
            let negative = d.next_u64() & 1 == 1;
            let (lo, hi) = if negative {
                (neg_lo, neg_hi)
            } else {
                (pos_lo, pos_hi)
            };
            // whole.milli in the band, then 26 more coefficient
            // digits: an exactly 34 digit value inside the band.
            let m = lo * 1000 + d.below((hi - lo) * 1000);
            let mut tail = String::with_capacity(26);
            for _ in 0..26 {
                tail.push(char::from(b'0' + u8::try_from(d.below(10)).unwrap()));
            }
            let sign = if negative { "-" } else { "" };
            let x_str = format!("{sign}{m}{tail}E-29");
            Sample {
                x: parse(&x_str),
                y: None,
                x_str,
                y_str: None,
            }
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
            Sample {
                x: parse(&x_str),
                y: Some(parse(&y_str)),
                x_str,
                y_str: Some(y_str),
            }
        }
    }
}

/// The kernel's 50 digit intermediate for this sample, via the public
/// extended entry points. `None` when the mirror path cannot produce
/// one (it never fires for in stratum samples; the caller counts it).
#[must_use]
pub fn mirror_extended(func: Func, s: &Sample) -> Option<Extended> {
    match func {
        Func::Sin => Some(sincos::sincos_extended::<Decimal128>(s.x).0),
        Func::Cos => Some(sincos::sincos_extended::<Decimal128>(s.x).1),
        Func::Tan => {
            let (sin_e, cos_e, _) = sincos::sincos_extended::<Decimal128>(s.x);
            Some(sin_e.div::<Decimal128>(cos_e))
        }
        Func::Exp => Some(ferrodec_transcend::exp::exp_extended(
            Extended::from_format(s.x),
        )),
        Func::Exp2 => Some(ferrodec_transcend::exp::exp_extended(
            Extended::from_format(s.x).mul(consts::ln2_ext()),
        )),
        Func::Pow => {
            let y = s.y?;
            Some(ferrodec_transcend::exp::exp_extended(
                Extended::from_format(y).mul(ln::ln_extended::<Decimal128>(s.x)),
            ))
        }
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
