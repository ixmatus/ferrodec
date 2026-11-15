# W4-C4 (reader C4): clarity and pragmatism, the format crates and substrates

- `run_clippy.sh` and `count.py`: every workspace allowed clippy lint
  re enabled per crate with `cargo clippy -p <crate> --lib --all-features
  --message-format=json -- -W clippy::<lint>`, library sites counted
  (the allow list table). `run_clippy_all.sh` and `countall.py`: the
  same over all targets. The coordinator reran `missing_panics_doc`
  (interop.rs:38/45/52 and arith.rs:104 flagged), `cast_possible_wrap`
  (477 over the dependency closure against the reader's 472 total),
  `float_cmp` (zero sites) and `--force-warn dead_code`.
- `fnlen.py`: brace matched function lengths excluding inline test
  modules (the large function table). `expects.py`: the production
  `expect`/`unwrap` grouping with the `#[cfg(all(test, …))]` rule that
  `counts.py` lacks (C4-13).
- `build_ex.log`: every example and bench built with `--all-features`,
  zero warnings.
