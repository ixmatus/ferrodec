#!/usr/bin/env bash
# Normalized mirror diffs for the 2026-10 deep review (read only).
#
#   docs/archive/review-2026-10/mirror_diff.sh <out-dir>
#
# Writes one unified diff per mirrored file pair under <out-dir> and
# prints a summary table (pair, file, diff lines). Normalization is
# deliberately light: only the format type and crate name tokens are
# unified (Decimal128/64/32 -> DecimalN, d128/d64/d32 -> dN) and, for the
# extended trio, the rung type names (Extended2 / ExtendedDyn ->
# Extended). Width constants, limb types, and table sizes are left as
# they are: those differences are what the readers classify.
set -euo pipefail
cd "$(dirname "$0")/../../.."
OUT="${1:?usage: mirror_diff.sh <out-dir>}"
mkdir -p "$OUT"

norm_fmt() { sed -E 's/Decimal(128|64|32)/DecimalN/g; s/decimal(128|64|32)/decimalN/g; s/\bd(128|64|32)\b/dN/g; s/ferrodec_decimal(64|32)/ferrodec/g'; }
norm_ext() { sed -E 's/ExtendedDyn/Extended/g; s/Extended2/Extended/g; s/extended_dyn/extended/g; s/extended2/extended/g'; }

pair() { # label normalizer left right
    local label="$1" norm="$2" l="$3" r="$4"
    local name; name="$(echo "$l" | tr '/' '_')"
    local d="$OUT/$label"; mkdir -p "$d"
    local n
    n=$(diff -u <($norm < "$l") <($norm < "$r") > "$d/$name.diff" || true; grep -c '^[-+][^-+]' "$d/$name.diff" || true)
    printf '| %s | %s | %s | %s |\n' "$label" "$l" "$r" "$n"
}

echo "| pair | left | right | changed lines |"
echo "|---|---|---|---|"
# Three-way format mirrors over the common file set.
for f in $(cd src && find . -name '*.rs' | sort); do
    f="${f#./}"
    [ -f "ferrodec-decimal64/src/$f" ] && pair root-vs-d64 norm_fmt "src/$f" "ferrodec-decimal64/src/$f"
    [ -f "ferrodec-decimal32/src/$f" ] && pair root-vs-d32 norm_fmt "src/$f" "ferrodec-decimal32/src/$f"
done
for f in $(cd ferrodec-decimal64/src && find . -name '*.rs' | sort); do
    f="${f#./}"
    [ -f "ferrodec-decimal32/src/$f" ] && pair d64-vs-d32 norm_fmt "ferrodec-decimal64/src/$f" "ferrodec-decimal32/src/$f"
done
# Sibling wrapper modules against the root's math/ shims.
for m in exp:exp hyper:hyperbolic trig:sincos trig_pi:trig_pi pow:pow powr:powr rootn:rootn rsqrt:rsqrt hypot:hypot compound:compound; do
    s="${m%%:*}"; r="${m##*:}"
    pair wrappers-d64-vs-root norm_fmt "ferrodec-decimal64/src/ops/$s.rs" "src/math/$r.rs"
    pair wrappers-d32-vs-root norm_fmt "ferrodec-decimal32/src/ops/$s.rs" "src/math/$r.rs"
done
pair wrappers-d64-vs-root norm_fmt ferrodec-decimal64/src/transcend_impl.rs src/math/format_impl.rs
pair wrappers-d32-vs-root norm_fmt ferrodec-decimal32/src/transcend_impl.rs src/math/format_impl.rs
# The extended trio.
pair extended-vs-extended2 norm_ext ferrodec-transcend/src/extended.rs ferrodec-transcend/src/extended2.rs
pair extended2-vs-extended_dyn norm_ext ferrodec-transcend/src/extended2.rs ferrodec-transcend/src/extended_dyn.rs
