#!/usr/bin/env bash
cd /Users/parnell/Development/ferrodec
export CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-D1
D=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/D1
while IFS='|' read c f; do
  t=$(echo "$c$f" | tr -d ' -,=')
  cargo test -p $c $f -- --list > $D/logs/list_$t.txt 2>&1; rc=$?
  echo "$c|$f|rc=$rc|$(grep -c ': test$' $D/logs/list_$t.txt)"
done <<'L'
ferrodec-decimal32|--features fmt
ferrodec-decimal32|--features transcendentals
ferrodec-decimal32|--all-features
ferrodec-decimal64|--features transcendentals
ferrodec-decimal64|--all-features
ferrodec|--features transcendentals
ferrodec|--all-features
ferrodec-multiword|
ferrodec-multiword|--all-features
ferrodec-transcend|--features transcendentals
ferrodec-transcend|--all-features
L
