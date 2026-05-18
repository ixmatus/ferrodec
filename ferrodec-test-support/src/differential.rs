//! Process-and-protocol harness for the Python/libmpdec differential
//! (Track 3, plan 2026-05-17). Format-agnostic on purpose: it only
//! ships a batch of requests to `tools/diff_oracle.py` and parses the
//! responses. The per-format value / faithful / status comparison
//! lives in each crate's `tests/differential.rs`, which has the
//! concrete decimal type.
//!
//! Local-only and opt-in: the only callers are the
//! `#![cfg(feature = "differential")]` test binaries, so a default
//! `cargo test` (and CI) never spawns Python even though this module
//! always compiles (it is std-only, no extra dependency). A nightly
//! job to run the differential is a deferred follow-up, not wired
//! here.
//!
//! `run_batch` returns `None` when no usable `python3` / `python` with
//! a libmpdec `decimal` is found. Callers must treat `None` as *skip
//! with a diagnostic*, never as a failure: the differential is a
//! corroborating local check, not a gate.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};
use std::thread;

/// One differential request. `op` is one of `add`, `sub`, `mul`,
/// `div`, `fma`, `sqrt`, `exp`, `ln`, `log10`, `pow`. `round` is the
/// `RoundingMode` debug name (`NearestEven`, ...). `args` are exact
/// decimal strings (the format's `{:e}` form is ideal).
pub struct Request {
    pub op: &'static str,
    pub prec: u32,
    pub emax: i32,
    pub emin: i32,
    pub round: &'static str,
    pub args: Vec<String>,
}

/// libmpdec's answer and its IEEE signal flags for one request. Flags
/// are a packed bitfield (`InvalidOperation`=1, `DivisionByZero`=2,
/// `Inexact`=4, `Overflow`=8, `Underflow`=16) read through accessors.
#[derive(Debug, Clone)]
pub struct Response {
    /// `str(Decimal)` of the result (may be `NaN` / `Infinity`).
    pub value: String,
    bits: u8,
}

impl Response {
    /// libmpdec raised `InvalidOperation`.
    pub fn invalid(&self) -> bool {
        self.bits & 1 != 0
    }
    /// libmpdec raised `DivisionByZero`.
    pub fn divbyzero(&self) -> bool {
        self.bits & 2 != 0
    }
    /// libmpdec raised `Inexact`.
    pub fn inexact(&self) -> bool {
        self.bits & 4 != 0
    }
    /// libmpdec raised `Overflow`.
    pub fn overflow(&self) -> bool {
        self.bits & 8 != 0
    }
    /// libmpdec raised `Underflow`.
    pub fn underflow(&self) -> bool {
        self.bits & 16 != 0
    }
}

/// Absolute path to the driver, resolved from this crate's manifest
/// dir (`<ws>/ferrodec-test-support`) to `<ws>/tools/diff_oracle.py`.
fn driver_path() -> String {
    format!("{}/../tools/diff_oracle.py", env!("CARGO_MANIFEST_DIR"))
}

/// First `python3`/`python` whose `decimal` is libmpdec, or `None`.
fn find_python() -> Option<&'static str> {
    for cand in ["python3", "python"] {
        let ok = Command::new(cand)
            .arg(driver_path())
            .arg("--selfcheck")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .is_some_and(|o| String::from_utf8_lossy(&o.stdout).starts_with("OK "));
        if ok {
            return Some(cand);
        }
    }
    None
}

/// Ship `reqs` to the driver in one process and parse the responses.
/// `None` ⇒ no usable Python (skip, do not fail). Panics only on a
/// genuine protocol break (response/request count mismatch or a
/// malformed flag field), which is a harness bug, not a kernel defect.
pub fn run_batch(reqs: &[Request]) -> Option<Vec<Response>> {
    if reqs.is_empty() {
        return Some(Vec::new());
    }
    let py = find_python()?;

    let mut input = String::with_capacity(reqs.len() * 48);
    for r in reqs {
        input.push_str(r.op);
        let _ = write!(input, "\t{}\t{}\t{}\t{}", r.prec, r.emax, r.emin, r.round);
        for a in &r.args {
            input.push('\t');
            input.push_str(a);
        }
        input.push('\n');
    }

    let mut child = Command::new(py)
        .arg(driver_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Feed stdin on a separate thread while reading stdout here. A
    // single-threaded write-all-then-read deadlocks once the batch
    // exceeds a pipe buffer (~64 KiB): Python blocks writing responses
    // we have not started reading, so it stops reading our requests,
    // so our write blocks too.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let bytes = input.into_bytes();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
        // `stdin` drops here, signalling EOF to the driver.
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut text = String::new();
    let read_ok = stdout.read_to_string(&mut text).is_ok();
    let _ = writer.join();
    let status = child.wait().ok()?;
    if !read_ok || !status.success() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        reqs.len(),
        "differential protocol: {} responses for {} requests",
        lines.len(),
        reqs.len()
    );

    let mut resp = Vec::with_capacity(lines.len());
    for line in lines {
        let (value, bits) = line
            .rsplit_once('\t')
            .expect("differential response is `value TAB flagbits`");
        let bits: u8 = bits.parse().expect("flagbits is a small integer");
        resp.push(Response {
            value: value.to_string(),
            bits,
        });
    }
    Some(resp)
}
