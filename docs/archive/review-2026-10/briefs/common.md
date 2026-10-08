# Common frame for every reader (binding)

You are one reader in a rigorous review of the `ferrodec` workspace
at `/Users/parnell/Development/ferrodec`, a `no_std` IEEE 754-2019
decimal floating point family: Decimal128 (root crate), Decimal64 and
Decimal32 siblings, an arbitrary precision crate, a shared
transcendental kernel with a correctly rounded escalation ladder, a
multiword integer substrate, and test support. The coordinator holds
every verdict; your job is to read, extract, and attack, and to
report in a form the coordinator can verify at the cited line.

## 1. Baseline gate (first action, before anything else)

Run `git -C /Users/parnell/Development/ferrodec rev-parse HEAD` and
compare it to `{{BASELINE}}`. If they differ, stop immediately and
report the mismatch; do nothing else. Work in that checkout at that
tip. Do not create worktrees or branches.

## 2. Build isolation

Every cargo invocation you make uses
`CARGO_TARGET_DIR={{SCRATCH}}/target-{{READER}}`. Five readers share
the machine; a shared `target/` serializes on the lock. You may run
`cargo test`, `cargo clippy`, `cargo doc`, `cargo kani`, and the
Python oracles against the repository tree.

## 3. Read only on the repository

No edits to any file under the repository, no `git add`, no
`git commit`, no `git stash`, no `cargo publish`, no `cargo fmt`.
Scratch reproducers live under `{{SCRATCH}}/{{READER}}/` as a
separate cargo project with `path = "/Users/parnell/Development/ferrodec/..."`
dependencies, or as Python scripts run with `python3 -I`. The
coordinator checks `git status` after your wave; a dirty tree
discards your output.

Never read under `.claude/worktrees/` or any `target/` directory.

## 4. Read first

Before judging anything: `docs/testing.md` (the verification map and
its stated residual frontier), the ADRs your scope names, and the
June review report `docs/archive/REPORT-rigorous-review-2026-06-09.md`
for your area (so defects already fixed are not re-reported and
June's blind spots are known to you). Your scope section lists the
files you own; files outside it belong to another reader in this
wave, and you do not report on them except to hand a lead to the
coordinator in "least sure".

## 5. Output schema (exact)

Write your report to `{{SCRATCH}}/{{READER}}/report.md` and also
return it as your final message. Sections in this order:

### Findings

A table first, one row per finding, then one subsection per finding.

| id | file:line | claim (one sentence) | severity | confidence | closure |

- `id`: `{{READER}}-1`, `{{READER}}-2`, ...
- `file:line`: repository relative path and the line the claim
  anchors to.
- severity you propose (the coordinator decides): P0 shipped wrong
  value, or a panic reachable from the public API; P1 spec
  divergence or an unsound argument with no wrong value yet
  witnessed; P2 engineering quality; P3 documentation.
- confidence: `reproduced` (you ran it and have the output),
  `traced` (a code path trace, not run), `hypothesis` (see the cap
  below).
- closure (required for P1 to P3, optional for P0): the file, the
  edit class (one of: constant, table, cfg, type, control flow,
  test, doc, tooling), and the test or derivation that would
  discharge it. A finding without a closure is report prose and
  never enters the remediation epic.

Per finding subsection: the evidence. For `reproduced`: the exact
command, where the reproducer lives, and the actual output lines.
For `traced`: the code path, function by function, with lines. For
every P0 or P1: a paragraph "why the stack missed it" naming the
instrument (test suite, oracle, corpus, pin, CI lane) that should
have caught it and why it did not; cite the instrument trust table
if you were given one.

**Hypothesis cap.** At most five `hypothesis` rows, and each must
name the splitting observation (what single measurement or test
distinguishes it from the boring explanation) and the exact command
that would make it. Everything else you suspect but did not check
goes to "Least sure".

### Least sure

What you could not check and why, with the file and line, so the
coordinator can decide whether to look. No cap.

### Found clean

What you read and found sound, by file, with the method you used
(read only; traced; executed against which oracle; replayed which
corpus). A clean verdict carries its blind spot: say what your
method could not have seen.

### Design notes

At most five shape opinions (a module should be structured
differently, a type should carry an invariant, a duplication should
be collapsed), each with the evidence that would make it a finding.
Not findings.

## 6. Falsify, do not confirm

A correctness claim without a reproducer is a hypothesis and counts
against the cap. Oracles available on this machine:
`fractions.Fraction` and CPython `decimal` (libmpdec) for rational
cases; mpmath at 120 dps (`import mpmath; mpmath.mp.dps = 120`);
Arb through python-flint for transcendental near boundary cases via
`~/.cache/pfloat-arb-oracle/venv/bin/python3`; the decTest vectors
under `tests/vectors/` for GDA semantics; the frozen corpora with
their `.prov` files for certified hard cases. State which oracle
decided each reproduced finding.

## 7. Provenance

Derive reproducers from the IEEE 754-2019 text, the GDA
specification, the cited papers, or the failing case itself. Never
recall another implementation's code; never paste from one.

## 8. Severity discipline

Report what you can defend at the line. Forty findings that are
mostly style is a worse report than eight that each change what the
maintainer does next. If you reach forty, rank and stop.

## 9. Scope boundary

The review records; it does not fix. Do not propose patches as
findings; the closure field is where the shape of a fix goes.
