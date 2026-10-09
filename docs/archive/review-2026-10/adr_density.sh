#!/usr/bin/env bash
# ADR citation density in production source, for D3's sample of ten
# earlier ADRs (read only). Run from the workspace root.
set -euo pipefail
cd "$(dirname "$0")/../../.."
grep -rhoE 'ADR-00[0-9]{2}' src ferrodec-*/src | sort | uniq -c | sort -rn
