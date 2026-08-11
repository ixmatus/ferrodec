//! The sampling loop: generate, run the kernel, measure, record.
//!
//! Counter mode sampling (see [`crate::prng`]) makes the checkpoint a
//! single integer and any index range reproducible, so an interrupted
//! overnight run resumes with `--resume` and shard outputs aggregate
//! idempotently. Every sample either contributes an evaluation or is
//! counted in a named skip bucket; nothing is silently dropped (the
//! no-silent-caps house rule).

use crate::margin::{boundary_distances, BoundaryDistances};
use crate::prng::StreamKey;
use crate::sample::{
    gen_sample, mirror_extended, production_cells, production_ne, Func, Sample, Stratum, TargetFmt,
};
use crate::{u256_to_decimal, u256_to_f64};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;
use std::time::Instant;

pub struct Config {
    pub campaign: String,
    pub func: Func,
    pub fmt: TargetFmt,
    pub stratum: Stratum,
    pub n: u64,
    pub shard: u32,
    /// Threshold: survivor when distance < `thr_num · 10^-thr_pow` ULP.
    pub thr_num: u128,
    pub thr_pow: u32,
    pub out: PathBuf,
    pub checkpoint_every: u64,
    pub resume: bool,
    /// Unconditional substream mode: emit an `A` line for every
    /// measured sample regardless of margin, so the certifier judges
    /// the kernel with no filter in the loop (the correlated failure
    /// counter; ADR-0059 S1). Costs all five modes per sample; meant
    /// for the 10^5..10^6 substream, not the main sweep.
    pub emit_all: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub evals: u64,
    pub survivors: u64,
    pub skip_nonnormal: u64,
    pub skip_no_mirror: u64,
    pub mirror_divergence: u64,
    pub min_grid_ulp: f64,
    pub min_tie_ulp: f64,
}

fn ckpt_path(out: &std::path::Path) -> PathBuf {
    let mut p = out.to_path_buf();
    p.set_extension("ckpt");
    p
}

fn hist_path(out: &std::path::Path) -> PathBuf {
    let mut p = out.to_path_buf();
    p.set_extension("hist");
    p
}

/// Snapshot the running histogram and counters to `<out>.hist`
/// (truncating write) so an interrupted overnight shard loses at most
/// one checkpoint interval of margin curve, matching the survivor
/// lines' incremental durability. Survivors stream to the TSV as
/// found; this file is the live view of everything else.
fn write_hist_snapshot(
    out: &std::path::Path,
    through: u64,
    hist: &BTreeMap<i64, (u64, f64, f64)>,
    sum: &Summary,
) -> io::Result<()> {
    let mut s = format!("# partial through i={through}\n");
    for (b, (count, g, t)) in hist {
        let _ = writeln!(s, "# H {b} {count} {g:.3e} {t:.3e}");
    }
    let _ = writeln!(
        s,
        "# F evals={} survivors={} skip_nonnormal={} skip_no_mirror={} diverged={} min_grid={:.3e} min_tie={:.3e}",
        sum.evals,
        sum.survivors,
        sum.skip_nonnormal,
        sum.skip_no_mirror,
        sum.mirror_divergence,
        sum.min_grid_ulp,
        sum.min_tie_ulp,
    );
    fs::write(hist_path(out), s)
}

fn margin_ulp(x2: ferrodec_multiword::U256, w: u32) -> f64 {
    u256_to_f64(x2) / (2.0 * 10f64.powi(i32::try_from(w).unwrap()))
}

/// One evaluated sample's disposition.
enum Disposition {
    /// Result not a normal finite number: margins undefined here
    /// (saturation and subnormal handling are S1 probe refinements,
    /// counted, never silently dropped).
    NonNormal,
    /// Mirror path produced no intermediate.
    NoMirror,
    Measured {
        d: BoundaryDistances,
        survivor: bool,
        diverged: bool,
        line: Option<String>,
    },
}

fn evaluate(cfg: &Config, i: u64, s: &Sample) -> Disposition {
    // Hot path: one production call (NearestEven, for the class gate
    // and the mirror divergence invariant) plus the mirror. The four
    // remaining modes are evaluated only when a line is emitted.
    let (ne_str, nonnormal) = production_ne(cfg.fmt, cfg.func, s);
    if nonnormal {
        return Disposition::NonNormal;
    }
    let Some(ext) = mirror_extended(cfg.func, s, cfg.fmt) else {
        return Disposition::NoMirror;
    };
    let Some(d) = boundary_distances(ext, cfg.fmt.digits()) else {
        return Disposition::NoMirror;
    };
    let mirror_ne_str = match cfg.fmt {
        TargetFmt::D128 => format!(
            "{}",
            ext.to_format::<ferrodec::Decimal128>(0, ferrodec_ieee::RoundingMode::NearestEven)
                .0
        ),
        TargetFmt::D64 => format!(
            "{}",
            ext.to_format::<ferrodec_decimal64::Decimal64>(
                0,
                ferrodec_ieee::RoundingMode::NearestEven
            )
            .0
        ),
    };
    let diverged = mirror_ne_str != ne_str;
    let within = d.within_ulp(cfg.thr_num, cfg.thr_pow);
    let survivor = within.grid || within.tie;
    let line = if survivor || diverged || cfg.emit_all {
        let cells = production_cells(cfg.fmt, cfg.func, s);
        let tag = if diverged {
            "D"
        } else if survivor {
            "S"
        } else {
            "A"
        };
        let mut l = format!(
            "{tag}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            cfg.func.name(),
            i,
            s.x_str,
            s.y_str.as_deref().unwrap_or("-"),
            d.w,
            u256_to_decimal(d.grid_x2),
            u256_to_decimal(d.tie_x2),
        );
        for cell in &cells {
            let _ = write!(l, "\t{cell}");
        }
        Some(l)
    } else {
        None
    };
    Disposition::Measured {
        d,
        survivor,
        diverged,
        line,
    }
}

/// Run the configured shard. Appends to `out` on resume, truncates
/// otherwise; checkpoints the next index every `checkpoint_every`
/// evaluations and at completion.
pub fn run(cfg: &Config) -> io::Result<Summary> {
    let key = StreamKey::derive(
        &cfg.campaign,
        cfg.func.name(),
        cfg.stratum.name(),
        cfg.shard,
    );
    let start: u64 = if cfg.resume {
        fs::read_to_string(ckpt_path(&cfg.out))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    if start >= cfg.n {
        // A completed shard relaunched by an idempotent driver: do not
        // append duplicate footer lines or reset anything.
        println!(
            "shard already complete: {} (checkpoint {} >= n {})",
            cfg.out.display(),
            start,
            cfg.n
        );
        return Ok(Summary::default());
    }
    if let Some(dir) = cfg.out.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(cfg.resume)
        .truncate(!cfg.resume)
        .write(true)
        .open(&cfg.out)?;
    if start == 0 {
        writeln!(
            out,
            "# ferrodec-campaign sweep: campaign={} func={} fmt={} stratum={} shard={} n={} thr={}e-{}",
            cfg.campaign,
            cfg.func.name(),
            cfg.fmt.name(),
            cfg.stratum.name(),
            cfg.shard,
            cfg.n,
            cfg.thr_num,
            cfg.thr_pow,
        )?;
    }

    let mut sum = Summary {
        min_grid_ulp: f64::INFINITY,
        min_tie_ulp: f64::INFINITY,
        ..Summary::default()
    };
    // Histogram bucket: trig decade, or 0 for the edge strata.
    let mut hist: BTreeMap<i64, (u64, f64, f64)> = BTreeMap::new();

    for i in start..cfg.n {
        let s = gen_sample(cfg.func, cfg.stratum, cfg.fmt, key.draws(i));
        match evaluate(cfg, i, &s) {
            Disposition::NonNormal => sum.skip_nonnormal += 1,
            Disposition::NoMirror => sum.skip_no_mirror += 1,
            Disposition::Measured {
                d,
                survivor,
                diverged,
                line,
            } => {
                sum.evals += 1;
                let (g, t) = (margin_ulp(d.grid_x2, d.w), margin_ulp(d.tie_x2, d.w));
                sum.min_grid_ulp = sum.min_grid_ulp.min(g);
                sum.min_tie_ulp = sum.min_tie_ulp.min(t);
                let bucket = match cfg.stratum {
                    Stratum::Decades { .. } => i64::from(decade_of(&s.x_str)),
                    _ => 0,
                };
                let e = hist
                    .entry(bucket)
                    .or_insert((0, f64::INFINITY, f64::INFINITY));
                e.0 += 1;
                e.1 = e.1.min(g);
                e.2 = e.2.min(t);
                if survivor {
                    sum.survivors += 1;
                }
                if diverged {
                    sum.mirror_divergence += 1;
                }
                if let Some(l) = line {
                    writeln!(out, "{l}")?;
                }
            }
        }
        if cfg.checkpoint_every > 0 && (i + 1) % cfg.checkpoint_every == 0 {
            // Histogram snapshot BEFORE the checkpoint index: a crash
            // between the two writes costs a redundant snapshot, never
            // a gap.
            write_hist_snapshot(&cfg.out, i + 1, &hist, &sum)?;
            fs::write(ckpt_path(&cfg.out), format!("{}\n", i + 1))?;
        }
    }

    for (b, (count, g, t)) in &hist {
        writeln!(out, "# H {b} {count} {g:.3e} {t:.3e}")?;
    }
    writeln!(
        out,
        "# F evals={} survivors={} skip_nonnormal={} skip_no_mirror={} diverged={} min_grid={:.3e} min_tie={:.3e}",
        sum.evals,
        sum.survivors,
        sum.skip_nonnormal,
        sum.skip_no_mirror,
        sum.mirror_divergence,
        sum.min_grid_ulp,
        sum.min_tie_ulp,
    )?;
    write_hist_snapshot(&cfg.out, cfg.n, &hist, &sum)?;
    fs::write(ckpt_path(&cfg.out), format!("{}\n", cfg.n))?;
    Ok(sum)
}

/// The decade of a `<coef>E<exp>` sample string (coef is 34 digits).
fn decade_of(x_str: &str) -> i32 {
    let e: i32 = x_str[x_str.find('E').unwrap() + 1..].parse().unwrap();
    e + 33
}

/// Timed calibration: evaluate for ~`secs` seconds without recording,
/// and report the sustained single thread evaluation rate.
#[must_use]
pub fn calibrate(cfg: &Config, secs: f64) -> f64 {
    let key = StreamKey::derive(
        &cfg.campaign,
        cfg.func.name(),
        cfg.stratum.name(),
        cfg.shard,
    );
    let start = Instant::now();
    let mut i = 0u64;
    while start.elapsed().as_secs_f64() < secs {
        for _ in 0..256 {
            let s = gen_sample(cfg.func, cfg.stratum, cfg.fmt, key.draws(i));
            let _ = evaluate(cfg, i, &s);
            i += 1;
        }
    }
    i as f64 / start.elapsed().as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(n: u64, out: PathBuf, resume: bool) -> Config {
        Config {
            campaign: "test".into(),
            func: Func::Sin,
            fmt: TargetFmt::D128,
            stratum: Stratum::Decades { lo: 15, hi: 40 },
            n,
            shard: 0,
            thr_num: 1,
            thr_pow: 1, // loose threshold so tiny runs produce survivors
            out,
            checkpoint_every: 50,
            resume,
            emit_all: false,
        }
    }

    #[test]
    fn resume_is_equivalent_to_one_shot() {
        let dir = std::env::temp_dir().join("fd-campaign-test");
        let (a, b) = (dir.join("oneshot.tsv"), dir.join("resumed.tsv"));
        let one = run(&cfg(200, a.clone(), false)).unwrap();

        // Interrupted run: stop at 120 (simulate by n=120), then
        // resume to 200 against the same output.
        run(&cfg(120, b.clone(), false)).unwrap();
        let two = run(&cfg(200, b.clone(), true)).unwrap();

        let strip = |p: &PathBuf| {
            let s = fs::read_to_string(p).unwrap();
            s.lines()
                .filter(|l| l.starts_with('S') || l.starts_with('D'))
                .map(String::from)
                .collect::<Vec<_>>()
        };
        let (la, lb) = (strip(&a), strip(&b));
        assert_eq!(la, lb, "survivor lines must be range independent");
        assert_eq!(one.evals, 200 - one.skip_nonnormal - one.skip_no_mirror);
        // The resumed second leg covered exactly 80 indices.
        assert_eq!(
            two.evals + two.skip_nonnormal + two.skip_no_mirror,
            80,
            "resume must continue from the checkpoint, not restart"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn samples_are_valid_and_deterministic() {
        for (f, st) in [
            (Func::Sin, Stratum::Decades { lo: 15, hi: 6140 }),
            (Func::Exp, Stratum::ExpEdge),
            (Func::Exp2, Stratum::ExpEdge),
            (Func::Pow, Stratum::PowEdge),
        ] {
            let key = StreamKey::derive("test", f.name(), st.name(), 1);
            for i in 0..50 {
                let s1 = gen_sample(f, st, TargetFmt::D128, key.draws(i));
                let s2 = gen_sample(f, st, TargetFmt::D128, key.draws(i));
                assert_eq!(s1.x_str, s2.x_str);
                assert_eq!(s1.y_str, s2.y_str);
                assert!(s1.x.is_finite());
            }
        }
    }
}
