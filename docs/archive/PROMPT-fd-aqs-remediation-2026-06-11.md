# fd-aqs P2/P3 remediation kickoff prompt, 2026-06-11

> **Archival note.** One shot engagement artifact, archived under
> `docs/archive/` per repository convention. This is the kickoff
> prompt that drove the fd-aqs P2/P3 remediation arc (`fd-aqs.10`
> through `fd-aqs.15`), written 2026-06-11 against `main` = `82ff739`.
> The arc it describes is complete: signed merge `5f6ee99`, version
> bumps `311e9be`, integration merge `595c467`; the bd epic `fd-aqs`
> and all fifteen children are closed. Claims below reflect the state
> at the time of writing, not the live repository.

  Pick up the remaining fd-aqs remediation queue in ferrodec (~/Development/ferrodec).

  ## Context

  A 2026-06-09 full-codebase review found confirmed correctness bugs and filed them as
  the bd epic fd-aqs (the beads database is preconfigured via BEADS_DIR; run `bd ready`
  and `bd show fd-aqs.<n>` — every issue carries file:line sites, runtime witnesses, and
  acceptance criteria). The full review report is IN TREE at
  docs/archive/REPORT-rigorous-review-2026-06-09.md — read it before starting.

  Slices fd-aqs.5–.8 (the transcendental kernel arc) are ALREADY DONE and merged: signed
  merge c17bb0d on main, ADR-0050 and ADR-0051 plus an ADR-0047 amendment. Do not touch
  those ADRs' claims, the new anchor-band corpus (tests/vectors/transcend/anchor_bands/),
  or its generator (tools/gen_anchor_band_vectors.py) unless an issue explicitly requires
  it. The project memory files
  (ferrodec-rigorous-review-findings-2026-06-09, ferrodec-fd-aqs-kernel-arc-2026-06-10)
  carry the status and several gotchas learned during the arc.

  ## Your queue, in recommended order

  1. fd-aqs.1 (P0): MIN_POSITIVE_NORMAL encodes the wrong value on all three fixed
     formats (1E-33 / 1E-15 / 1E-6 instead of 1E-6143 / 1E-383 / 1E-95). The biased
     exponent should be PRECISION - 1, not BIAS - PRECISION + 1. Fix all three crates and
     pin by VALUE (to_bits equality against try_new(1, E_MIN) or parse, plus
     next_down(MIN_POSITIVE_NORMAL).is_subnormal()), not by class.
  2. fd-aqs.2 (P0): Decimal64::quantize returns (NaN, INVALID) for representable pads
     10-entry table. Extend to 16 entries, replace the runtime guard with a compile-time
     assert against PRECISION, add regression tests at pad 10 and 15.
     (.1 + .2 can share one slice/commit pair; they are the patch-release payload.)
  3. fd-aqs.3 (P0): ferrodec-decimal — (ea - min_e) as u32 in arith.rs combine_finite
     wraps for fma (product exponents reach ±4.3e9); quantize.rs and divrem.rs
     materialize 10^(exponent gap) BEFORE the precision validity check (multi-GB allocs
     from in-range operands). Guard the gap against precision-plus-margin before any
     mul_pow10 on operand-derived gaps and short-circuit to the op's invalid/overflow
     path. In tests, do NOT actually allocate gigabytes: assert the guard fires on
     moderate-but-oversize gaps. The 27591-vector decTest conformance suite for this
     crate must stay 0-fail — note it has NO CI lane until .9 lands, so run
     `cargo test -p ferrodec-decimal` locally every time.
  4. fd-aqs.4 (P1): enforce precision >= 1 in ferrodec-decimal's Context. NonZeroU32 is
     the principled fix but breaks the 1.0.1 API; a documented saturating clamp in
     round_finite is the non-breaking fallback. ASK PARNELL (AskUserQuestion) before
     choosing — do not unilaterally break a 1.x API.
  5. fd-aqs.9 (P1): harness + CI. strip_comment cuts `--` inside quoted operands — note
     there are TWO copies (ferrodec-test-support/src/conformance.rs and the root
     tests/conformance.rs); fixing it should recover 6 vectors (toSci '--1', '1E--1' in
     ddBase/dsBase/base), which will SHIFT the per-file pins — update them deliberately.
     Add CI lanes for ferrodec-decimal, ferrodec-multiword, and ferrodec-transcend tests,
     and add the `dpd` feature to the root and d64 cargo-kani invocations. Fix the README
     "CI runs the full suite" claim.
  6. fd-aqs.10 (P2): verification strengthening. Still open after the kernel arc:
     per-(func,mode) exact pins for tests/transcend_vectors.rs (currently len > 500
     floor); SHA256SUMS coverage for the transcend corpus — NOTE this should now ALSO
     cover the new anchor_bands/ subdirectory (post-dates the issue text); acosh
     metamorphic probes inside the log1p region (x in [1+1e-6, 1.01)); the
     kani::cover!(false) arms in src/verify/encode.rs become asserts; the pi/2 reduction
     doc mismatch (argred.rs module header says 80 digits, the data path uses the
     38-digit PI_OVER_TWO_COEF_38 — either widen or amend the ADR-0032 derivation
     honestly); stale faithful-contract prose in frozen.rs:11-13 (do not disturb the
     load_anchor_bands/load_from additions below it) and differential.rs (band should
     tighten from within_k 2 to 0).
  7. fd-aqs.11 (P2): vendor the missing dq decTest files for d128 (dqBase, dqCopy*,
     dqRemainder, dqToIntegral, dqPlus, dqMinMag, dqMaxMag); fix the harness Plus impl
     (identity+OK today — mishandles sNaN) when wiring dqPlus; extend SHA256SUMS and
     per-file pins; testing.md's §1 decTest blind-spot text is stale (bitwise passes
     since ADR-0031) — but do NOT rewrite the band-corpus or residual-frontier sections
     added by ADR-0050/0051.
  8. fd-aqs.12 (P2): from_f64 family divergence (parent shortest-round-trip vs sibling
     {:.17e} re-round with a double-rounding hazard) — ASK PARNELL: port
     shortest-round-trip to siblings, or document the divergence. Plus to_f64/to_f32
     spurious INEXACT on exact conversions.
  9. fd-aqs.13, .14, .15 (P2/P3): serde deserialize_any vs bincode; parse NaN payload cap
     [10^33, 2^110); seal DecimalFormat; multiword hardening (the Algorithm D add-back
     witness from the report: dividend limbs [0,0,500000000,499999999], divisor
     [1,0,500000000] little-endian; make from_ascii_digits total); the doc-nit sweep.

  ## Workflow (house rules — these are load-bearing)

  - ONE feature branch for the whole arc, UNSIGNED commits throughout; Parnell signs the
    merge once at the end. Never push unsigned commits to main. When the arc is done,
    STOP and prompt him with the slice state, draft merge message, and exact command —
    he is not sitting at the YubiKey.
  - One commit per slice, body = intro paragraph + section per area + verification
    totals + `Co-Authored-By:` trailer for your model. One concern per commit (refactor
    XOR behavior). Reproduce before fixing: write the failing test first.
  - Close each bead with `bd close fd-aqs.<n>` when its slice commits. Beads gotcha:
    `bd create --deps blocks:X` means the NEW issue blocks X; use
    `bd dep add <dependent> <blocker>` for depends-on.
  - Gates before every commit: `cargo fmt --all --check` and
    `cargo clippy --workspace --all-targets --features
    "transcendentals,fmt,dpd,serde,num-traits" -- -D warnings`, plus the relevant test
    sweeps: root + siblings with `--features "transcendentals,fmt,dpd"`,
    `-p ferrodec-decimal`, `-p ferrodec-multiword`, `-p ferrodec-transcend
    --all-features`.
  - Exact pins, never floors: expected counts per bucket, updated deliberately in the
    same commit that changes them.
  - Never edit source while a cargo test sweep is running, and never regenerate
    committed vector corpora mid-sweep (the binaries read them at run time).
  - If you write Python oracle/generator code: CPython decimal's unary ops (abs, +x)
    round at the AMBIENT context (28 digits default) — use copy_abs and explicit
    localcontext; libmpdec ln/log10/power are correctly rounded under ROUND_HALF_EVEN
    ONLY; mpmath nstr is unreliable past ~30 digits (use mp.libmp.to_str).

  ## Versioning

  Do not bump versions mid-arc. After .1/.2 land, the release shape to propose to
  Parnell: one 3.3.1 patch across ferrodec / ferrodec-decimal64 / ferrodec-decimal32
  (carrying both your fixes and the already-merged kernel arc), and a ferrodec-decimal
  patch or minor depending on the .3/.4 API outcome. Publishing is always Parnell's hand.
