  # fish doesn't allow setting variables with `=`, so the easiest cross-shell compatible expression is
  # this oneliner.
  # Making this cross-shell compatible is a bit complicated: we need the surrounding curly braces to
  # fix precedence issues between `||` and `|`, but curly braces are processed differently in fish.
  # Thankfully, we don't need curly braces around the first expression, so we can put the fish check
  # first and it early exits. This runs correctly in sh, bash, zsh, and fish.
  # Replace `HOOK_NAME` with the appropriate hook name.
set | command grep "^WARP_BOOTSTRAPPED[= ]" >/dev/null || printf "\\e]9278;f;{\"hook\": \"HOOK_NAME\", \"value\": { \"shell\": \"%s\", \"uname\": \"%s\", \"session_id\": @@WARP_SESSION_ID@@ }}\\a" $(set | command grep "^FISH_VERSION[= ]" >/dev/null && echo "fish" || { echo $0 | command -p grep -q zsh && echo "zsh"; } || { echo $0 | command -p grep -q bash && echo "bash"; } || echo "unknown") $(uname)
