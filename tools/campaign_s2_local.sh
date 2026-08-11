#!/usr/bin/env bash
# S2 local deep-margin campaign launcher (ADR-0059, fd-4zo.19).
#
# The S1 machinery at the S2 priority set: sin cos tan exp ln pow
# log10 sinh cosh at Decimal128, exp sin cos at Decimal64. Idempotent
# and resumable exactly like campaign_s1_local.sh (per-shard
# checkpoints; rerunning resumes incomplete shards and skips complete
# ones), so the campaign can run across as many nights as its depth
# needs.
#
# Depth defaults total ~2.7e9 evaluations, ~32 wall-hours at 9
# workers on the calibrated 8 P-core rates: a long weekend of nights,
# or one continuous run. The bead's 1e9-1e10 PER FUNCTION ambition
# is 10-100 nights of local compute; these defaults are the honest
# local slice, and every knob is an environment override so a deeper
# rerun is a relaunch with bigger N (completed shards resume past
# their old n only if N grows; a grown N reopens the shard and the
# stream continues deterministically from the checkpoint).
#
# Strata notes (the S2 additions):
# - ln/log10 decades span the full exponent range: their hard cases
#   are not concentrated in a thin spot.
# - sinh/cosh decades START AT -7: below that the value hugs its
#   anchor (x, resp. 1) deeper than the 16-digit drop window, every
#   sample "survives" vacuously through the anchor seam, and the
#   first smoke run produced 34% survivor spam. The anchor band is
#   the residual channel's territory (certified by S4), not a
#   sampling question. Their exp-edge bands cover the e^|x|/2
#   saturation approach on both signs.
# - exp gains a body stratum (decades -20..4) beside S1's exp-edge.
# - pow keeps S1's pow-edge only: its 2-D body has no designed
#   stratum yet, a stated cap, not an oversight.
# - d64 shards use the s2d64 campaign label so their prng streams
#   never collide with same-named d128 jobs.
#
# Results land out of tree (XDG data dir). Certification and the
# corpus freeze are separate steps (campaign_certify.py; the S2
# corpus lands under tests/vectors/transcend/campaign/ with
# MANIFEST.json when the run completes).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${S2_OUT:-$HOME/.local/share/ferrodec-campaign/s2}"
JOBS="${S2_JOBS:-9}"
CKPT="${S2_CKPT:-1000000}"

N_TRIG="${S2_N_TRIG:-300000000}"   # per trig function (one shardset)
N_EXP="${S2_N_EXP:-150000000}"     # per exp stratum (edge + body)
N_LN="${S2_N_LN:-250000000}"       # ln and log10
N_POW="${S2_N_POW:-200000000}"
N_HYP="${S2_N_HYP:-100000000}"     # per hyperbolic stratum
N_D64="${S2_N_D64:-100000000}"     # per d64 job
SHARDS="${S2_SHARDS:-10}"

mkdir -p "$OUT"
cargo build --release -q -p ferrodec-campaign
BIN="$ROOT/target/release/sweep"
GIT_SHA="$(git -C "$ROOT" rev-parse --short HEAD)"

JOBLIST="$OUT/joblist.txt"
: > "$JOBLIST"
emit() { # campaign func fmt stratum extra n shard
    local campaign="$1" func="$2" fmt="$3" stratum="$4" extra="$5" n="$6" shard="$7"
    echo "--campaign $campaign --func $func --format $fmt --stratum $stratum $extra --n $n --shard $shard --checkpoint-every $CKPT --resume --out $OUT/${campaign}_${func}_${fmt}_${stratum}_s${shard}.tsv" >> "$JOBLIST"
}

pershard() { echo $(( $1 / SHARDS )); }

for f in sin cos tan; do
    for s in $(seq 0 $((SHARDS - 1))); do
        emit s2 "$f" d128 decades "--decade-lo 15 --decade-hi 6140" "$(pershard "$N_TRIG")" "$s"
    done
done
for s in $(seq 0 $((SHARDS - 1))); do
    emit s2 exp d128 exp-edge "" "$(pershard "$N_EXP")" "$s"
    emit s2 exp d128 decades "--decade-lo -20 --decade-hi 4" "$(pershard "$N_EXP")" "$s"
    emit s2 ln d128 decades "--decade-lo -6100 --decade-hi 6100" "$(pershard "$N_LN")" "$s"
    emit s2 log10 d128 decades "--decade-lo -6100 --decade-hi 6100" "$(pershard "$N_LN")" "$s"
    emit s2 pow d128 pow-edge "" "$(pershard "$N_POW")" "$s"
    emit s2 sinh d128 decades "--decade-lo -7 --decade-hi 4" "$(pershard "$N_HYP")" "$s"
    emit s2 sinh d128 exp-edge "" "$(pershard "$N_HYP")" "$s"
    emit s2 cosh d128 decades "--decade-lo -7 --decade-hi 4" "$(pershard "$N_HYP")" "$s"
    emit s2 cosh d128 exp-edge "" "$(pershard "$N_HYP")" "$s"
    emit s2d64 exp d64 exp-edge "" "$(pershard "$N_D64")" "$s"
    emit s2d64 exp d64 decades "--decade-lo -20 --decade-hi 2" "$(pershard "$N_D64")" "$s"
    emit s2d64 sin d64 decades "--decade-lo 15 --decade-hi 370" "$(pershard "$N_D64")" "$s"
    emit s2d64 cos d64 decades "--decade-lo 15 --decade-hi 370" "$(pershard "$N_D64")" "$s"
done

total_jobs=$(wc -l < "$JOBLIST" | tr -d ' ')
echo "s2 campaign: $total_jobs shards, $JOBS workers, out=$OUT, git=$GIT_SHA, started $(date '+%Y-%m-%d %H:%M:%S')"
rm -f "$OUT/DONE"
{
    echo "campaign=s2 git=$GIT_SHA started=$(date '+%Y-%m-%d %H:%M:%S')"
    echo "N_TRIG=$N_TRIG N_EXP=$N_EXP N_LN=$N_LN N_POW=$N_POW N_HYP=$N_HYP N_D64=$N_D64 SHARDS=$SHARDS thr=1e-6"
} >> "$OUT/RUNLOG"

while IFS= read -r args; do
    while [ "$(jobs -pr | wc -l | tr -d ' ')" -ge "$JOBS" ]; do sleep 2; done
    # shellcheck disable=SC2086
    "$BIN" $args &
done < "$JOBLIST"
wait

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
    echo "s2 campaign: $incomplete shard(s) incomplete; rerun this script to resume"
    exit 1
fi

date '+%Y-%m-%d %H:%M:%S' > "$OUT/DONE"
echo "s2 campaign: all shards complete $(date '+%Y-%m-%d %H:%M:%S')"
echo "next: certify survivors (campaign_certify.py), freeze the corpus + MANIFEST.json, pins, disclosure diff (fd-4zo.19)"
