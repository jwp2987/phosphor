#!/usr/bin/env bash
set -uo pipefail
TRIVIAL='\{\s*\}|\{\s*(unimplemented!|todo!|panic!)'
echo "--- check_stub_coverage empty-input, WITH the guard (repaired form) ---"
real_defs=""
if [[ -n "$real_defs" ]] && grep -qvE "$TRIVIAL" <<<"$real_defs"; then echo "  -> continue (stub SKIPPED)"; else echo "  -> falls through (stub REPORTED)  [correct]"; fi
echo "--- same, WITHOUT the -n guard (what the fix would have been without it) ---"
if grep -qvE "$TRIVIAL" <<<"$real_defs"; then echo "  -> continue (stub SKIPPED)  [WRONG]"; else echo "  -> falls through"; fi
echo "--- is_test_bearing empty-input ---"
content=""
if [[ -n "$content" ]] && grep -qE '#\[test\]' <<<"$content"; then echo "  -> test-bearing"; else echo "  -> not test-bearing  [correct]"; fi
grep -qE '#\[test\]' <<<""; echo "  unguarded status=$? (1 = also correct)"
echo "--- precheck rustfmt empty-output ---"
fmt_out=""
if grep -qE '^error: (expected|unexpected)' <<<"$fmt_out"; then echo "  -> flagged  [WRONG]"; else echo "  -> clean  [correct]"; fi
echo "--- precheck rustfmt with a real parse error captured ---"
fmt_out=$'error: expected one of `!` or `::`, found `x`\n  --> src/x.rs:1:1'
if grep -qE '^error: (expected|unexpected)' <<<"$fmt_out"; then echo "  -> flagged  [correct]"; else echo "  -> clean  [WRONG]"; fi
echo "--- the OLD inverted form, for contrast ---"
printf '%s\n' "$fmt_out" | grep -qE '^error: (expected|unexpected)'; echo "  old-form status=$? (0 here only because the input is tiny)"
