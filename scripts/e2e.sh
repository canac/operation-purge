#!/bin/sh
# Run one scripted E2E session against a throwaway config.
# Usage: scripts/e2e.sh SCRIPT OUT_DIR [extra game args...]
set -eu
script=$1 out=$2
shift 2
mkdir -p "$out"
rm -f "$out/config.toml"
OP_CONFIG="$out/config.toml" exec ./target/release/operation-purge \
    --e2e "$script" --e2e-out "$out" "$@"
