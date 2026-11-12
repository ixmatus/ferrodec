# CI-kani-0.68: what the Kani job did under 0.68.0, 0.67.0 and the pin

The report's Method section names a fifth CI cause: the Kani job on
PR #1 was cancelled at GitHub's six hour limit. The pinning commit
(`be64a0f`) and the report first said Kani 0.68.0 "hung the install
step". These three log excerpts show what happened, and they are kept
here because GitHub deletes job logs after ninety days.

Each file is `gh run view <run> --job <job> --log` for the `kani
harnesses` job of `ixmatus/ferrodec`, cut down to the lines that name
an action sub step, the Kani version, the first harness and its
verdict, a completion tally, or the cancellation. The timestamps are
the service's.

| file | run | Kani | what it shows |
|---|---|---|---|
| `august-0.67.0-run-31749959445.txt` | `main` at `0a5be4c`, 2026-08-13 | 0.67.0 (the action's `latest` then) | install 16 s; the action then runs `cargo-kani`: first harness verified in 7.2 s, 84 harnesses in 22 min; then the workflow's four invocations: 1, 90, 67, 103 harnesses |
| `october-0.68.0-run-37947625260.txt` | PR #1 at `b93bac5`, 2026-10-09 | 0.68.0 (`latest` had floated) | install 17 s; the action's `cargo-kani` run prints `Checking harness verify::addsub::special_status_only_invalid_for_snan...` and nothing more until the cancellation six hours later; `cargo-kani`, `kani-driver` and `cbmc` are alive when the runner cleans up |
| `november-0.67.0-pinned-run-43156222866.txt` | PR #1 at `be64a0f`, 2026-11-12 UTC | 0.67.0 (pinned) | install 14 s; first harness verified in 3.1 s, 84 harnesses in 38 min; then 1, 90, 67, 103; the job and all 27 jobs of the run green |

## What the step is

`model-checking/kani-github-action` at tag `v1.1` (commit
`f838096619a707b0f6b2118cf435eaccfa33e51f`; `action.yml` is licensed
Apache-2.0 OR MIT) is a composite action with three steps that run
here: "Install Rust", "Install Kani" (`src/install-kani.sh <version>`)
and "Run Kani", whose script is the `command` input followed by the
`args` input, `cargo-kani` and nothing by default. So the workflow's
`uses:` line proves the root crate's default feature harnesses before
the four `cargo kani` lines beneath it run. On a third August run
(31503694142, 2026-08-11) the same step took 43 minutes.

## What is and is not established

Established: the installer never hung; the six hours were spent in a
proving run; that run made no visible progress past its first harness
under 0.68.0; the same harness takes seconds under 0.67.0 on the same
runner class; the pin restored a green job.

Not established: why. The October excerpt has no CBMC banner after the
harness line, where the August log prints one eleven milliseconds
later, so even whether CBMC began solving is not visible. Nobody has
run 0.68.0 on this tree locally. fd-e8a9.44 holds the diagnosis, the
question of whether the action's own 84 harness run is a subset of
the 90 the explicit root invocation proves (if so it costs 22 to 43
runner minutes per run and buys nothing), and the registry entry Kani
and the action still lack.
