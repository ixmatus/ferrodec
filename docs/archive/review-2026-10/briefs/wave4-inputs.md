# Wave 4 inputs (every C3, C4, D1, D4 reader reads this after common.md)

Wave 3 closed at the tip named in your launch message. The three
correctness waves are done; you read for architecture, clarity,
provenance and the record. What they established and hand to you:

- **Verdict records.** `wave1-verification.md` (instruments and
  documents), `wave2-verification.md` (the transcendental kernel),
  `wave3-verification.md` (the format crates and substrates). Each
  ends with a "Handed to Waves 3 and 4" or "Handed to Wave 4" list;
  those items are yours by owner. Reader reports are verbatim under
  `readers/`; their "Design notes" sections are shape opinions with
  evidence that the correctness readers could not pursue, and they
  are your raw material.
- **Ten P0 beads are open** (`bd list --status open`): the Newton seed
  panics (fd-0o9), `Decimal128::fma` directed modes (fd-4dhj),
  `sinh`/`cosh` overflow gate (fd-b6zt), the Decimal128 `exp`
  underflow gate (fd-mq5e), Decimal32 `atan2` through the subnormal
  seed (fd-h3jk), the `atan2` anchor seam (fd-gu75), the BID bit layout
  (B1-1), `Decimal64::fma` overlap path (B2-1), `ferrodec-decimal`
  `power` exactness (B4-1/2/3), the interop NaN payload panic (B4-7).
  Do not re-report them; do say what structure let each class exist
  (three copies, a borrowed constant, a prose precondition, a string
  bridge) when your scope touches it.
- **Structural facts the correctness waves established, by owner:**
  - C3: the Newton seed defect lives in all three `extended*.rs`
    copies and the fix for the step count exists only in the dynamic
    copy (P1B-2, A2-2, A4-6, A5-6; fd-k9t); the cancellation depth
    literal `33` is false and appears in more than eight places
    (P1B-1, A4-1); four copies of the `2^α 5^β` assembly (A1); 46
    items dead under `trig-pi` alone (A1-1); every rung 2 budget rests
    on derivation alone (A3's budget table); `EXP`'s reduction term is
    mis-sized and omitted from six itemizations (A3-4, A5-9);
    `Extended::parse_str`, `recip`, `sqrt` public with prose
    preconditions (A3-9, A5-6); gate constants borrowed from `exp`
    (A5-1, A5-2, B2's gate table).
  - C4: the root `fma` has four regimes chosen by comment proven gates
    where two P0/P1 defects sit (P1B-3, B1-2); the Decimal64 `fma` has
    three alignment paths of which one lacks the direction fix (B2-1);
    eleven hand written special case dispatchers diverge where the
    shared kernels do not (B2's dispatch table, B2 design note 1);
    the sibling `drop_excess_digits` loops once per dropped digit over
    a 2·10^9 exponent gap (B3-1); the I/O boundary bridges through
    strings (`from_f64` via `{:e}`, interop via `Display` then
    `parse_str`: B3-4, B4-7, B4-8); `Context` validates nothing and a
    dozen `as i32` sites wrap (B4-5, B4-6); `Work` zeros can carry a
    positionless sticky (B4-3); the one ulp `power` band in the
    conformance runner (B4-12); d32 literals where d64 derives from
    `PRECISION` (B2-10); the coordinator's pin probe: a wrong
    Decimal32 quantize table entry passes every suite.
  - D1: three `bid.rs` transcriptions of one layout, all wrong against
    §3.5.2 (B1-1); the siblings' num-traits adapters lack
    `from_i128`/`from_u128` (B2-8, B3-12); `%` means two different
    operations across the crates (B3-9, D3-4); `ToPrimitive` rounds
    where `f64` truncates (B3-10); `to_f64(rm)` ignores `rm` on every
    crate (B3-11, B4-8); `serde_bid` cannot round trip JSON on
    Decimal128 (B3-6); `Sum` folds from `+0` (B3-15); the multiword
    tests run none of `DecBig`'s without `--all-features` (coordinator
    pin probe); the `DecimalFormat` trait's NaN contract contradicts
    the root (B2-4); scaleb's envelope differs across crates (B1-3,
    B2-5); the policy choices (GDA versus IEEE: scaleb, abs/neg with
    status, minNum, Division_impossible) are undocumented as choices
    (B1 design note 4).
  - D4: CLAMPED attributed to IEEE §7.4 and generated into the registry
    (B5-7); the `inv_pi_digits` doc merge (B5-6); the registry has no
    entries for the sources the review leaned on (Knuth 4.3.1, Lauter
    and Lefèvre, Niven, Payne and Hanek, astro-float's `abs_cmp`, the
    oracle versions); the README's BID claim is false at the encoding
    level (B1-1), which touches the disclosure section (diff proposal
    only).
- **Recurring shape across the waves, for all four of you:** a fact
  stated in prose (a precondition, a bound, a layout, a contract) that
  the type system could have carried, found false by the first reader
  who measured it. When your scope contains such a sentence, say which
  type would have made the sentence unnecessary.
- **Phase 1 artifacts:** `counts.md`, `mirror_summary.md`,
  `adr_density.md`, `s2_coverage.md`, `splice/`.
