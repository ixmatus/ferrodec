#!/usr/bin/env bash
# S1 local overnight probe launcher (ADR-0059, fd-4zo.5).
#
# Idempotent: every shard runs with --resume against its own
# checkpoint, so rerunning this script after an interruption continues
# where each shard stopped and completed shards exit immediately.
# Survivor and divergence lines stream to each shard's TSV as found;
# the margin histogram snapshots to <shard>.hist at every checkpoint
# (default every 10^6 samples, ~8 minutes of work at calibrated
# rates), so an interruption loses at most one interval of curve.
#
# Depth (the fd-4zo.3 calibration rescope): 1e8 per function for
# sin/cos/tan (decades) and exp/exp2 (exp-edge), 3e7 for pow
# (pow-edge); ~67 core-hours, ~7.5 wall-hours at 9 workers on an
# 8 P-core machine. Override via environment: S1_OUT, S1_JOBS,
# S1_N_TRIG / S1_N_EXP / S1_N_POW (per shard), S1_SHARDS_*.
#
# Results land OUT of the repository tree (XDG data dir, like the
# beads database) so nothing pollutes git status and nothing lives in
# a session-scoped temp dir.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${S1_OUT:-$HOME/.local/share/ferrodec-campaign/s1}"
JOBS="${S1_JOBS:-9}"
N_TRIG="${S1_N_TRIG:-10000000}"
N_EXP="${S1_N_EXP:-10000000}"
N_POW="${S1_N_POW:-5000000}"
SHARDS_TRIG="${S1_SHARDS_TRIG:-10}"
SHARDS_EXP="${S1_SHARDS_EXP:-10}"
SHARDS_POW="${S1_SHARDS_POW:-6}"
CKPT="${S1_CKPT:-1000000}"

mkdir -p "$OUT"
cargo build --release -q -p ferrodec-campaign
BIN="$ROOT/target/release/sweep"
GIT_SHA="$(git -C "$ROOT" rev-parse --short HEAD)"

JOBLIST="$OUT/joblist.txt"
: > "$JOBLIST"
emit() { # func stratum n shard
    echo "--campaign s1 --func $1 --stratum $2 --n $3 --shard $4 --checkpoint-every $CKPT --resume --out $OUT/$1_$2_s$4.tsv" >> "$JOBLIST"
}
# The unconditional substream first (protected line item, ADR-0059 S1):
# every sample emitted regardless of margin under a separate campaign
# label so its counter streams decorrelate from the main sweep. Small
# jobs; they finish early and certify independently.
N_SUB="${S1_N_SUB:-100000}"
sub() { # func stratum
    echo "--campaign s1-sub --emit-all --func $1 --stratum $2 --n $N_SUB --shard 0 --checkpoint-every $CKPT --resume --out $OUT/$1_$2_sub.tsv" >> "$JOBLIST"
}
sub sin decades
sub cos decades
sub tan decades
sub exp exp-edge
sub exp2 exp-edge
sub pow pow-edge

for f in sin cos tan; do
    for s in $(seq 0 $((SHARDS_TRIG - 1))); do emit "$f" decades "$N_TRIG" "$s"; done
done
for f in exp exp2; do
    for s in $(seq 0 $((SHARDS_EXP - 1))); do emit "$f" exp-edge "$N_EXP" "$s"; done
done
for s in $(seq 0 $((SHARDS_POW - 1))); do emit pow pow-edge "$N_POW" "$s"; done

total_jobs=$(wc -l < "$JOBLIST" | tr -d ' ')
echo "s1 probe: $total_jobs shards, $JOBS workers, out=$OUT, git=$GIT_SHA, started $(date '+%Y-%m-%d %H:%M:%S')"
rm -f "$OUT/DONE"

# Plain bash job pool (macOS xargs -I rejects long replacement
# strings, and bash 3.2 has no wait -n). Job lines word-split
# intentionally; no value contains spaces by construction.
while IFS= read -r args; do
    while [ "$(jobs -pr | wc -l | tr -d ' ')" -ge "$JOBS" ]; do sleep 2; done
    # shellcheck disable=SC2086
    "$BIN" $args &
done < "$JOBLIST"
wait

# Completeness audit: DONE only when every shard's checkpoint equals
# its n (a crashed shard is INCOMPLETE and the rerun resumes it).
incomplete=0
while IFS= read -r args; do
    n=$(awk '{for(i=1;i<NF;i++) if($i=="--n") print $(i+1)}' <<< "$args")
    out=$(awk '{for(i=1;i<NF;i++) if($i=="--out") print $(i+1)}' <<< "$args")
    ck="${out%.tsv}.ckpt"
    got="none"
    [ -f "$ck" ] && got="$(tr -d '[:space:]' < "$ck")"
    if [ "$got" != "$n" ]; then
        echo "INCOMPLETE: $out (checkpoint $got != $n)"
        incomplete=$((incomplete + 1))
    fi
done < "$JOBLIST"
if [ "$incomplete" -ne 0 ]; then
    echo "s1 probe: $incomplete shard(s) incomplete; rerun this script to resume"
    exit 1
fi

date '+%Y-%m-%d %H:%M:%S' > "$OUT/DONE"
echo "s1 probe: all shards complete $(date '+%Y-%m-%d %H:%M:%S')"
echo "next: certify survivors + substream, then aggregate (fd-4zo.5)"
