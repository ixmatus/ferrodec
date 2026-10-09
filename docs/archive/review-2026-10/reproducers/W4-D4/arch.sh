#!/bin/bash
set -o pipefail
D=$1
while read slug url; do
  ts=$(echo "$url" | sed -E 's#https://web.archive.org/web/([0-9]+)/.*#\1#')
  orig=$(echo "$url" | sed -E 's#https://web.archive.org/web/[0-9]+/##')
  raw="https://web.archive.org/web/${ts}id_/${orig}"
  out=$(curl -s -L --max-time 120 -o "$D/$slug.bin" -w '%{http_code}|%{url_effective}|%{content_type}|%{size_download}' "$raw")
  sha=$(shasum -a 256 "$D/$slug.bin" | cut -c1-64)
  echo "$slug|$out|$sha"
done
