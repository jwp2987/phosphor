#!/bin/bash
# Usage: check_commit.sh <hash>
HASH="$1"
echo "###### $HASH ######"
git log -1 --format="%s" "$HASH"
FILES=$(git diff-tree --no-commit-id --name-only -r "$HASH" -- crates/warpui_core)
for f in $FILES; do
  target="$f"
  if [ ! -f "$target" ]; then
    # try stripping a /gui/ path segment (fork keeps GUI elements flat, no gui/ dir)
    alt=$(echo "$f" | sed 's#/gui/#/#')
    if [ -f "$alt" ]; then
      target="$alt"
    fi
  fi
  if [ ! -f "$target" ]; then
    echo "-- $f : FILE ABSENT (also tried $alt) --"
    continue
  fi
  echo "-- $f  =>  $target --"
  git show "$HASH" -- "$f" | grep -E '^\+' | grep -vE '^\+\+\+|^\+\s*//|^\+\s*$|^\+use ' | \
    grep -E 'fn |struct |enum |impl |pub |const |static ' | sed 's/^\+//' | \
    awk '!seen[$0]++' | head -5 | while IFS= read -r line; do
      trimmed=$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
      short=$(echo "$trimmed" | cut -c1-90)
      if [ -z "$trimmed" ]; then continue; fi
      if grep -qF -- "$trimmed" "$target" 2>/dev/null; then
        echo "   PRESENT: $short"
      else
        echo "   MISSING: $short"
      fi
    done
done
echo
