#!/usr/bin/env bash
# Registers the openWarp custom merge driver + enables rerere.
# Run this once after the first clone; afterward, merging upstream (merge /
# cherry-pick / rebase) will:
# 1. Automatically keep the local version for paths marked merge=zap-ours in .gitattributes
# 2. Have rerere record each conflict resolution, automatically reusing it for the same conflict next time
set -euo pipefail

git config merge.zap-ours.name "Always keep openWarp version (custom driver)"
git config merge.zap-ours.driver true
git config rerere.enabled true
git config rerere.autoupdate true

echo "openWarp merge drivers + rerere configured."
echo "  rerere.enabled        = $(git config --get rerere.enabled)"
echo "  rerere.autoupdate     = $(git config --get rerere.autoupdate)"
echo "  merge.zap-ours   = $(git config --get merge.zap-ours.driver)"
