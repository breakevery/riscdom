#!/usr/bin/env sh
# Gated commit wrapper: runs scripts/gate first and commits only when it is
# green. Use this instead of `git commit`.
#
#   scripts/commit.sh "feat: something"
set -eu

msg="${1:-}"
if [ -z "$msg" ]; then
  echo "usage: scripts/commit.sh \"<commit message>\""
  exit 2
fi

cd "$(cd "$(dirname "$0")/.." && pwd)"

echo "== commit: running gate =="
sh scripts/gate.sh || {
  echo "commit: gate FAILED -- nothing committed"
  exit 1
}

git add -A
git commit -m "$msg"
echo "commit: ok ($msg)"
