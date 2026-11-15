# W4-C3 (reader C3): clarity and pragmatism, `ferrodec-transcend`

- `trio.py`: the extended trio table (per function line counts in
  `extended.rs`, `extended2.rs`, `extended_dyn.rs`, similarity after type
  name normalisation). `longfn.py`: functions over 80 lines. `casts.py`
  and `cast_sample.txt`: the 48 cast sample with its bound class.
- `cohort/`: C3-9, replays the root shim test's 144 round trips and
  compares `to_string`; 82 change the cohort (`1E+1 → 10`) while the
  test's comment claims it guards against a cohort shift. Relative path
  dependency; build with `CARGO_TARGET_DIR` outside the tree.
- `copy_changes.diff`: what the reader changed in a `git archive` copy
  of the crate for C3-3, C3-4 and C3-11 (the ten "reached at M8"
  allows and the three file wide `#![allow(dead_code)]` removed; a
  Decimal32 dev dependency added); `real_format_probe.rs` is the in
  crate integration test that reproduces A3-1 and A5-1 from inside
  `ferrodec-transcend` (C3-11). Coordinator reruns of all three
  identical to the reader's.
