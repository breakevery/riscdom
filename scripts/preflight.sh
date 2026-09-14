#!/usr/bin/env sh
# Pre-flight checks (read-only). Run before committing.
#
# Checks:
#   1. git tracks no sensitive files (.db / .db-journal / .sqlite / *.jsonl / .env)
#   2. no secret-shaped strings in tracked files
#      (sk-..., ghp_, github_pat_, AKIA..., AIza...)
#   3. the live $DEEPSEEK_API_KEY value never appears in tracked files
#
# Exit code 0 = OK, 1 = something was found.
# Only file:line is printed — never the matched value.
set -u

repo="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -z "${repo:-}" ]; then
  echo "preflight: not a git repository"
  exit 1
fi
cd "$repo" || exit 1

out="$(mktemp)"
trap 'rm -f "$out"' EXIT INT TERM

scan() {
  label="$1"
  shift
  git grep -n -I "$@" -- . 2>/dev/null | while IFS= read -r line; do
    file="${line%%:*}"
    rest="${line#*:}"
    lineno="${rest%%:*}"
    echo "$label -> $file:$lineno"
  done >>"$out"
}

# 1. sensitive files must not be tracked
for f in $(git ls-files); do
  name="${f##*/}"
  case "$f" in
    *.db | *.db-journal | *.sqlite | *.jsonl)
      echo "tracked sensitive file -> $f" >>"$out"
      ;;
  esac
  if [ "$name" = ".env" ]; then
    echo "tracked sensitive file -> $f" >>"$out"
  fi
done

# 2. secret-shaped strings
scan 'api-key-shaped (sk-)' -E 'sk-[A-Za-z0-9]{16,}'
scan 'github-token (ghp_)' -E 'ghp_[A-Za-z0-9]{20,}'
scan 'github-pat (github_pat_)' -E 'github_pat_[A-Za-z0-9_]{20,}'
scan 'aws-access-key (AKIA)' -E 'AKIA[0-9A-Z]{16}'
scan 'google-api-key (AIza)' -E 'AIza[0-9A-Za-z_-]{30,}'

# 3. the live environment key must never appear in the tree
if [ -n "${DEEPSEEK_API_KEY:-}" ]; then
  scan 'live DEEPSEEK_API_KEY value' -F -e "$DEEPSEEK_API_KEY"
fi

if [ -s "$out" ]; then
  cat "$out"
  echo "preflight: FAILED ($(wc -l <"$out" | tr -d ' '))"
  exit 1
fi

echo "preflight: OK"
exit 0
