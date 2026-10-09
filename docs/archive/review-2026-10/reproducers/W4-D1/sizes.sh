#!/usr/bin/env bash
cd /Users/parnell/Development/ferrodec
export CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-D1/sizes
D=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/D1
: > $D/sizes.tsv
# warm deps
cargo build -q --release --target thumbv6m-none-eabi -p ferrodec-transcend --features transcendentals >/dev/null 2>&1
for c in ferrodec-ieee ferrodec-multiword ferrodec-transcend ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal; do
  case $c in ferrodec-ieee|ferrodec-multiword) sets="none";; ferrodec-transcend) sets="none transcendentals";; ferrodec-decimal) sets="fmt";; *) sets="none fmt transcendentals";; esac
  for f in $sets; do
    FA=""; [ $f != none ] && FA="--features $f"
    cargo build -q --release --target thumbv6m-none-eabi -p $c --lib --no-default-features $FA >/dev/null 2>&1
    cargo clean -q --release --target thumbv6m-none-eabi -p $c >/dev/null 2>&1
    s=$(date +%s.%N 2>/dev/null || python3 -c 'import time;print(time.time())')
    s=$(python3 -c 'import time;print(time.time())')
    cargo build -q --release --target thumbv6m-none-eabi -p $c --lib --no-default-features $FA >/dev/null 2>&1; rc=$?
    e=$(python3 -c 'import time;print(time.time())')
    lib=$(echo $c | tr - _)
    r=$CARGO_TARGET_DIR/thumbv6m-none-eabi/release/lib$lib.rlib
    sz=$(wc -c < $r | tr -d ' ')
    txt=$(rust-size -A $r 2>/dev/null | awk '$1 ~ /^\.text/ {t+=$2} $1 ~ /^\.rodata/ {r+=$2} END {print t"\t"r}')
    printf "%s\t%s\trc=%s\t%.1fs\trlib=%s\ttext,rodata=%s\n" $c $f $rc $(python3 -c "print($e-$s)") $sz "$txt" >> $D/sizes.tsv
  done
done
echo DONE >> $D/sizes.tsv
