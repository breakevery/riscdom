#!/usr/bin/env sh
# Remove RiscDom's temporary directories from the system temp directory.
#
#   scripts/clean-temp.sh                              # dry run (safe default)
#   scripts/clean-temp.sh --force                      # delete
#   scripts/clean-temp.sh --force --older-than-hours 24
#
# Safety: only entries whose name starts with `riscdom-` are considered, so the
# rest of the temp directory is never touched. `<tmp>/riscdom` itself (the
# fallback data directory: settings, sessions, toolchains) is excluded explicitly.
set -eu

force=0
older=0
while [ $# -gt 0 ]; do
  case "$1" in
    --force|-f) force=1 ;;
    --older-than-hours)
      shift
      older="${1:-0}"
      ;;
    *)
      echo "usage: scripts/clean-temp.sh [--force] [--older-than-hours N]" >&2
      exit 2
      ;;
  esac
  shift
done

tmp="${TMPDIR:-/tmp}"
if [ -z "$tmp" ] || [ "$tmp" = "/" ]; then
  echo "refusing to run: the temp directory looks wrong ($tmp)" >&2
  exit 2
fi

if [ "$older" -gt 0 ]; then
  targets="$(find "$tmp" -maxdepth 1 -name 'riscdom-*' ! -name 'riscdom' -mmin "+$((older * 60))" 2>/dev/null || true)"
else
  targets="$(find "$tmp" -maxdepth 1 -name 'riscdom-*' ! -name 'riscdom' 2>/dev/null || true)"
fi

count=0
[ -n "$targets" ] && count="$(printf '%s\n' "$targets" | grep -c . || true)"
if [ "$count" -eq 0 ]; then
  echo "clean-temp: nothing to clean under $tmp"
  exit 0
fi

kb="$(du -sk $(printf '%s\n' "$targets" | tr '\n' ' ') 2>/dev/null | awk '{s+=$1} END {print s+0}')"

if [ "$force" -ne 1 ]; then
  echo "clean-temp: DRY RUN — $count entry(ies), ${kb} KB under $tmp"
  printf '%s\n' "$targets" | head -n 20 | sed 's/^/  /'
  [ "$count" -gt 20 ] && echo "  … and $((count - 20)) more"
  echo "clean-temp: re-run with --force to delete"
  exit 0
fi

removed=0
printf '%s\n' "$targets" | while IFS= read -r path; do
  [ -z "$path" ] && continue
  if rm -rf -- "$path"; then
    removed=$((removed + 1))
  else
    echo "warning: could not remove $path" >&2
  fi
done
left="$(find "$tmp" -maxdepth 1 -name 'riscdom-*' ! -name 'riscdom' 2>/dev/null | grep -c . || true)"
echo "clean-temp: removed entries ($kb KB); $left riscdom-* entry(ies) left"
