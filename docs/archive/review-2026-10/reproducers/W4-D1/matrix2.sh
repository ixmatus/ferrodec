#!/usr/bin/env bash
set -u -o pipefail
export CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-D1
D=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/D1
cd /Users/parnell/Development/ferrodec
: > $D/matrix2.tsv
row(){ printf "%s\t%s\t%s\t%s\t%s\n" "$@" >> $D/matrix2.tsv; }
# combos, build thumbv6m + clippy host
for c in ferrodec-transcend ferrodec ferrodec-decimal64 ferrodec-decimal32; do
  for f in trig,exp-log trig,hyperbolic trig,pow trig-pi,trig trig-pi,exp-log trig-pi,pow exp-log,pow hyperbolic,pow; do
    tag="${c}__$(echo $f | tr , +)"
    cargo build -p $c --lib --target thumbv6m-none-eabi --no-default-features --features $f > $D/logs/$tag.build.log 2>&1; b=$?
    cargo clippy -p $c --lib --no-default-features --features $f -- -D warnings > $D/logs/$tag.clippy.log 2>&1; k=$?
    row $c "$f" thumbv6m "build=$b" "clippy=$k"
  done
done
# format crate's own lints, dead_code allowed (transcend dead code masks them)
for c in ferrodec ferrodec-decimal64 ferrodec-decimal32; do
  for f in trig-pi exp-log hyperbolic; do
    tag="${c}__${f}__Adead"
    cargo clippy -p $c --lib --no-default-features --features $f -- -D warnings -A dead_code > $D/logs/$tag.clippy.log 2>&1; k=$?
    row $c "$f(-A dead_code)" host "-" "clippy=$k"
  done
done
# all-features, all-targets clippy host
for c in ferrodec-ieee ferrodec-multiword ferrodec-transcend ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal ferrodec-test-support ferrodec-campaign; do
  tag="${c}__allfeat_alltargets"
  if [ $c = ferrodec-test-support ]; then FF="--features transcend-oracle"; else FF="--all-features"; fi
  cargo clippy -p $c --all-targets $FF -- -D warnings > $D/logs/$tag.clippy.log 2>&1; k=$?
  row $c "$FF --all-targets" host "-" "clippy=$k"
done
# thumbv8m builds
for c in ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal ferrodec-transcend ferrodec-multiword ferrodec-ieee; do
  for f in default transcendentals; do
    if [ $f = default ]; then FA=""; else FA="--features $f"; fi
    [ $c = ferrodec-decimal ] && [ $f = transcendentals ] && FA="--features interop,binary-float"
    [ $c = ferrodec-multiword ] && [ $f = transcendentals ] && FA="--features alloc"
    [ $c = ferrodec-ieee ] && [ $f = transcendentals ] && continue
    tag="${c}__v8m_${f}"
    cargo build -p $c --lib --target thumbv8m.main-none-eabi $FA > $D/logs/$tag.build.log 2>&1; b=$?
    row $c "${FA:-default}" thumbv8m.main "build=$b" "-"
  done
done
# std leakage probe: thumbv6m with every no_std-relevant feature on
for c in ferrodec ferrodec-decimal64 ferrodec-decimal32; do
  tag="${c}__v6m_allshipping"
  cargo build -p $c --lib --target thumbv6m-none-eabi --features transcendentals,unbounded-ladder,binary-float,ops,serde,num-traits,dpd,telemetry > $D/logs/$tag.build.log 2>&1; b=$?
  row $c "all shipping features" thumbv6m "build=$b" "-"
done
echo DONE >> $D/matrix2.tsv
