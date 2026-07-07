#!/usr/bin/env bash
#
# gen_quirks.sh — regenerate docs/quirks.md from the parity ledger comments.
#
# The port carries three greppable comment tags, each citing the C++ ground
# truth by `file:line` (see /Users/caleb/Downloads/programs/OperationPurge):
#
#   // PARITY:  behaviour deliberately reproduced bug-for-bug from the 2011 C++
#   // GUARD:   a hardening check the original lacked (validation the C++ omits)
#   // FIX:     a corrected divergence from the C++ (bug NOT reproduced)
#
# This script scans src/ (and tests/) for those tags, pulls the C++ citation
# out of each comment's text (best-effort: any `Something.cpp:NN` / `.h:NN`
# token on the tag line), and emits docs/quirks.md — one table per tag, sorted
# by Rust file:line. Run from the repo root:  ./scripts/gen_quirks.sh
#
# Portable: POSIX grep/awk/sort only; no ripgrep, no -P/PCRE, no external deps.

# Note: no `set -e`/`pipefail` — grep exits non-zero on "no match", which is a
# normal outcome here (e.g. zero FIX tags) and must not abort the script.

# Repo root = parent of this script's dir.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

OUT="docs/quirks.md"
SCAN_DIRS="src tests"

# Grep every tag occurrence (both `//` line comments and `///` doc comments)
# as   path:lineno:full-line   records.
RAW="$(grep -rn -E '(//|///) (PARITY|GUARD|FIX):' $SCAN_DIRS || true)"

TOTAL="$(printf '%s\n' "$RAW" | grep -c . || true)"

mkdir -p docs

# Header.
{
  echo "# Parity ledger (quirks registry)"
  echo
  echo "> **Generated file — do not edit by hand.** Regenerate with"
  echo "> \`./scripts/gen_quirks.sh\` after adding or moving any"
  echo "> \`// PARITY:\` / \`// GUARD:\` / \`// FIX:\` comment."
  echo
  echo "Every intentional divergence from — or bug-for-bug reproduction of —"
  echo "the 2011 C++/Direct3D9 original is pinned in the source with one of"
  echo "three tags, each citing the C++ ground truth by \`file:line\`"
  echo "(\`~/Downloads/programs/OperationPurge\`). This table is that ledger,"
  echo "harvested straight from those comments."
  echo
  echo "| Tag | Meaning |"
  echo "|---|---|"
  echo "| \`PARITY\` | Behaviour deliberately reproduced bug-for-bug from the C++. |"
  echo "| \`GUARD\`  | A hardening/validation check the original lacked. |"
  echo "| \`FIX\`    | A corrected divergence — a C++ bug **not** reproduced. |"
  echo
  echo "**$TOTAL** ledger entries found across \`$SCAN_DIRS\`."
  echo
} > "$OUT"

emit_section() {
  tag="$1"; title="$2"
  count="$(printf '%s\n' "$RAW" | grep -E "(//|///) $tag:" | grep -c . || true)"
  {
    echo "## $title (\`$tag\`) — $count"
    echo
    echo "| Rust \`file:line\` | C++ citation | Note |"
    echo "|---|---|---|"
  } >> "$OUT"

  printf '%s\n' "$RAW" \
    | grep -E "(//|///) $tag:" \
    | sort -t: -k1,1 -k2,2n \
    | while IFS= read -r line; do
        # Split off  path:lineno:  prefix.
        rust_loc="$(printf '%s' "$line" | awk -F: '{print $1":"$2}')"
        body="$(printf '%s' "$line" | cut -d: -f3-)"

        # The comment text after the tag.
        note="$(printf '%s' "$body" | sed -E "s#.*(//|///) $tag: *##")"

        # Best-effort C++ citation: first `Word.cpp:NN` / `Word.h:NN[-NN]`
        # token on the tag line (many comments cite design §refs instead;
        # those show a dash here and carry the ref inline in the note).
        cite="$(printf '%s' "$body" \
                 | grep -oE '[A-Za-z_][A-Za-z0-9_]*\.(cpp|h):[0-9]+(-[0-9]+)?' \
                 | head -n1 || true)"
        [ -z "$cite" ] && cite="—"

        # Escape pipes so the note never breaks the table.
        note="$(printf '%s' "$note" | sed 's/|/\\|/g')"

        printf '| `%s` | `%s` | %s |\n' "$rust_loc" "$cite" "$note" >> "$OUT"
      done
  echo >> "$OUT"
}

emit_section PARITY "Parity quirks"
emit_section GUARD  "Hardening guards"
emit_section FIX    "Corrected divergences"

echo "wrote $OUT ($TOTAL entries)"
