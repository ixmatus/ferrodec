# D3 fleet working tree captures (splice artifact)

Captured 2026-10-08 on branch `review-2026-10`, before the six D3 agent
worktrees under `.claude/worktrees/` were removed. Every agent branch
(`worktree-agent-*`) was zero commits ahead of `main` (tip `c27b773`),
so each agent's deliverable existed only as uncommitted changes in its
worktree; the supervisor spliced those into `main` by hand in the D3
landing (`c27b773..8959007`). Memory records that the three way splice
ate byte identical bodies once and interleaved same signature functions,
and that `exact.rs` was rebuilt deterministically afterwards.

Per agent id: `<id>.status` (the worktree's `git status --short`),
`<id>.diff` (`git diff HEAD` from inside the worktree, whose `HEAD` is
the base `c27b773` the agent was cut from, so the agent's full delivered
change against that base), and
`<id>.untracked/` (any files the agent added that git did not track).

Reader A1 (Wave 2) compares each function body in the merged
`ferrodec-transcend/src/exact.rs` against the body the agent delivered
here; any body lost or interleaved is a finding.
