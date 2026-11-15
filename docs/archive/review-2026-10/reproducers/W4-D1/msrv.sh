#!/usr/bin/env bash
cd /Users/parnell/Development/ferrodec
export CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-D1/msrv
D=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/D1
: > $D/msrv.tsv
rustc +1.84 --version >> $D/msrv.tsv
for c in ferrodec-ieee ferrodec-multiword ferrodec-transcend ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal ferrodec-test-support ferrodec-campaign; do
  F=--all-features; [ $c = ferrodec-test-support ] && F="--features transcend-oracle"
  cargo +1.84 check -p $c --lib $F --locked > $D/logs/msrv_$c.log 2>&1; a=$?
  cargo +1.84 check -p $c --all-targets $F --locked > $D/logs/msrv_${c}_all.log 2>&1; b=$?
  echo -e "$c\tlib=$a\tall-targets=$b" >> $D/msrv.tsv
done
echo DONE >> $D/msrv.tsv
