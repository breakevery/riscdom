#!/usr/bin/env sh
# Bilingual documentation check (reports only -- never rewrites files).
#
# Every *.md in the repository must carry a language switcher on its first line and have a
# counterpart in the other language:
#   X.md        -> first line `[中文](X.zh-CN.md) | English`, and X.zh-CN.md must exist
#   X.zh-CN.md  -> first line `[English](X.md) | 中文`, and X.md must exist
#
# Excluded: LICENSE (kept in English legal text by design) and the agent-workspace identity
# notes (IDENTITY.md / SOUL.md / USER.md), which are single-language by design.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

find . -type f -name '*.md' \
    -not -path './node_modules/*' -not -path './target/*' -not -path './.git/*' \
    -not -path './.cowork-temp/*' -not -path './dist/*' \
    -not -path '*/node_modules/*' -not -path '*/target/*' -not -path '*/dist/*' \
    -not -name 'LICENSE' -not -name 'LICENSE.md' -not -name 'LICENSE.txt' \
    -not -name 'IDENTITY.md' -not -name 'SOUL.md' -not -name 'USER.md' \
    | sed 's|^\./||' | sort > "$tmp"

problems=0
checked=0

while IFS= read -r f; do
    [ -z "$f" ] && continue
    first="$(head -n 1 "$f")"
    case "$f" in
        *.zh-CN.md)
            en="${f%.zh-CN.md}.md"
            want="[English]($(basename "$en")) | 中文"
            if [ ! -f "$en" ]; then
                echo "  $f:1: missing English counterpart $(basename "$en")"
                problems=$((problems + 1))
                continue
            fi
            ;;
        *)
            zh="${f%.md}.zh-CN.md"
            want="[中文]($(basename "$zh")) | English"
            if [ ! -f "$zh" ]; then
                echo "  $f:1: missing Chinese counterpart $(basename "$zh")"
                problems=$((problems + 1))
                continue
            fi
            ;;
    esac
    if [ "$first" != "$want" ]; then
        echo "  $f:1: expected '$want', found '$first'"
        problems=$((problems + 1))
    else
        checked=$((checked + 1))
    fi
done < "$tmp"

if [ "$problems" -gt 0 ]; then
    echo "bilingual links: FAILED"
    exit 1
fi

echo "bilingual links: OK ($checked files checked)"
