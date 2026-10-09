#!/bin/bash
set -u
S=/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad
cd /Users/parnell/Development/ferrodec
L="module_name_repetitions missing_errors_doc must_use_candidate cast_possible_truncation cast_lossless cast_sign_loss cast_possible_wrap cast_precision_loss match_same_arms similar_names float_cmp single_match_else unreadable_literal items_after_statements many_single_char_names too_many_lines too_many_arguments assertions_on_constants trivially_copy_pass_by_ref fn_params_excessive_bools should_implement_trait manual_let_else enum_variant_names missing_panics_doc"
W=()
for l in $L; do W+=(-W "clippy::$l"); done
for p in ferrodec ferrodec-decimal64 ferrodec-decimal32 ferrodec-decimal ferrodec-multiword ferrodec-ieee; do
  CARGO_TARGET_DIR=$S/target-C4 cargo clippy -p $p --lib --all-features --message-format=json -- "${W[@]}" > $S/C4/clippy-$p.json 2> $S/C4/clippy-$p.err
  echo "$p exit $?"
done
