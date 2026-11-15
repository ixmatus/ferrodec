#!/usr/bin/env bash
set -u -o pipefail
export CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-D1
D=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/D1
cd /Users/parnell/Development/ferrodec
FMT="none fmt trig trig-pi exp-log hyperbolic pow transcendentals telemetry unbounded-ladder unbounded-ladder,transcendentals telemetry,transcendentals binary-float ops serde num-traits dpd"
: > $D/matrix.tsv
for c in ferrodec-ieee ferrodec-multiword ferrodec-transcend ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal; do
  case $c in
    ferrodec|ferrodec-decimal64|ferrodec-decimal32) L="$FMT";;
    ferrodec-decimal) L="none fmt binary-float interop";;
    ferrodec-transcend) L="none fmt trig trig-pi exp-log hyperbolic pow transcendentals telemetry unbounded-ladder unbounded-ladder,transcendentals telemetry,transcendentals";;
    ferrodec-multiword) L="none alloc";;
    *) L="none";;
  esac
  for f in $L; do
    if [ "$f" = none ]; then FA=""; else FA="--features $f"; fi
    tag="${c}__$(echo $f | tr , +)"
    cargo build -p $c --lib --target thumbv6m-none-eabi --no-default-features $FA > $D/logs/$tag.build.log 2>&1; b=$?
    bw=$(grep -c '^warning' $D/logs/$tag.build.log)
    cargo clippy -p $c --lib --no-default-features $FA -- -D warnings > $D/logs/$tag.clippy.log 2>&1; k=$?
    printf "%s\t%s\tthumbv6m\t%s\t%s\t%s\n" $c "$f" $b $bw $k >> $D/matrix.tsv
  done
done
echo DONE >> $D/matrix.tsv
