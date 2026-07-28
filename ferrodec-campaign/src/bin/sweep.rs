//! S1 sweep CLI.
//!
//! ```text
//! cargo run --release -p ferrodec-campaign --bin sweep -- \
//!   --campaign s1 --func sin --stratum decades --decade-lo 15 \
//!   --decade-hi 6140 --n 1000000000 --shard 0 \
//!   --out out/s1_sin_shard0.tsv [--resume] [--calibrate 10]
//! ```
//!
//! Hand rolled argument parsing: a `publish = false` campaign driver
//! does not earn a CLI dependency.

use ferrodec_campaign::sample::{Func, Stratum};
use ferrodec_campaign::sweep::{calibrate, run, Config};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match parse_args() {
        Ok((cfg, calibrate_secs)) => {
            if let Some(secs) = calibrate_secs {
                let rate = calibrate(&cfg, secs);
                println!(
                    "calibrate: func={} stratum={} {:.0} evals/s single thread ({:.1} s)",
                    cfg.func.name(),
                    cfg.stratum.name(),
                    rate,
                    secs
                );
                ExitCode::SUCCESS
            } else {
                match run(&cfg) {
                    Ok(s) => {
                        println!(
                            "done: evals={} survivors={} diverged={} skip_nonnormal={} skip_no_mirror={} min_grid={:.3e} min_tie={:.3e}",
                            s.evals,
                            s.survivors,
                            s.mirror_divergence,
                            s.skip_nonnormal,
                            s.skip_no_mirror,
                            s.min_grid_ulp,
                            s.min_tie_ulp
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("sweep failed: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{e}\n\nsee the module doc in src/bin/sweep.rs for usage");
            ExitCode::FAILURE
        }
    }
}

fn parse_args() -> Result<(Config, Option<f64>), String> {
    let mut func = None;
    let mut stratum_name = String::from("decades");
    let mut decade_lo = 15i32;
    let mut decade_hi = 6140i32;
    let mut n = 1_000_000u64;
    let mut shard = 0u32;
    let mut thr_num = 1u128;
    let mut thr_pow = 6u32;
    let mut campaign = String::from("s1");
    let mut out = PathBuf::from("out/sweep.tsv");
    let mut checkpoint_every = 10_000_000u64;
    let mut resume = false;
    let mut calibrate_secs = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let next = |i: &mut usize| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("missing value after {}", args[*i - 1]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--func" => func = Func::parse(&next(&mut i)?),
            "--stratum" => stratum_name = next(&mut i)?,
            "--decade-lo" => decade_lo = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--decade-hi" => decade_hi = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--n" => n = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--shard" => shard = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--thr-num" => thr_num = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--thr-pow" => thr_pow = next(&mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--campaign" => campaign = next(&mut i)?,
            "--out" => out = PathBuf::from(next(&mut i)?),
            "--checkpoint-every" => {
                checkpoint_every = next(&mut i)?.parse().map_err(|e| format!("{e}"))?;
            }
            "--resume" => resume = true,
            "--calibrate" => {
                calibrate_secs = Some(next(&mut i)?.parse().map_err(|e| format!("{e}"))?);
            }
            other => return Err(format!("unknown argument {other}")),
        }
        i += 1;
    }

    let func = func.ok_or("missing required --func (sin cos tan exp exp2 pow)")?;
    let stratum = match stratum_name.as_str() {
        "decades" => Stratum::Decades {
            lo: decade_lo,
            hi: decade_hi,
        },
        "exp-edge" => Stratum::ExpEdge,
        "pow-edge" => Stratum::PowEdge,
        other => return Err(format!("unknown stratum {other}")),
    };
    Ok((
        Config {
            campaign,
            func,
            stratum,
            n,
            shard,
            thr_num,
            thr_pow,
            out,
            checkpoint_every,
            resume,
        },
        calibrate_secs,
    ))
}
