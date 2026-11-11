# r16-countedboot: `test_local_counted_bootstrap` fails since #2051 (#2098)

## What failed

`./script/test_local_counted_bootstrap` on integ/r14:
`1 FAILED: bash: the bundled script has the here-document the rewrite replaces`.
Passes on integ/r13, fails from fix/r13-gbssh (85def6917, #2051) on.

## Cause

The check (`script/test_local_counted_bootstrap`, the line after the rewrite) requires that the
counted form of the assembled `bash.sh` has no `<< 'EOM'` left and has a `head -c`.

#2051 added `warp_ssh_msys_bootstrap_text` to `app/assets/bundled/bootstrap/bash_body.sh`. Its
last guard was

    if [[ "$text" != *"WARP_BOOTSTRAP_VAR << 'EOM'"* ]]; then

so the body, which is the content of the here-document, now spelled `<< 'EOM'` itself. The
rewrite (`counted_read_rewrite`, `app/src/terminal/bootstrap.rs`) takes the first opener line
(`BASH_BOOTSTRAP_HEREDOC_OPEN`, a whole line of `bash.sh`) and the first `\nEOM\n` after it; the
guard's line is neither, so the rewrite is unchanged. The same failing run shows it: "counted
form bootstraps", "same hook sequence", "echoes less than half as much", and both timing checks
are ok.

## Verdict

Not the app regression that was feared: the local bash bootstrap is still sent in the counted
form, the paced/relayed latency gain (#1226, #1172) is intact.

It is still a fault of the #2051 change and not of the test:

- The Rust test `bash_counted_read_bootstrap_applies_to_the_bundled_bash_script`
  (`app/src/terminal/bootstrap_test.rs`) asserts the same `!text.contains("<< 'EOM'")` on the
  real bundled script. Nothing has been compiled since #2051; it would have failed on the box.
- The invariant is the right one. "No opener text is left" is the only cheap way to tell a
  rewrite that replaced the here-document from one that did not, and a body that spells its own
  opener takes that away.
- The guard was also weaker than it read: the kept lines it inspects include the guard's own
  line, so in a real Git Bash shell it matched itself and could never refuse.

## Fix

`bash_body.sh`: the guard's pattern is spelled in two adjacent quoted pieces
(`*"WARP_BOOTSTRAP_VAR <<"" 'EOM'"*`). Bash joins them: the pattern is the same text, it still
matches the opener line of `bash.sh` (checked in bash 5.3; adjacent quoted strings in a `[[ ]]`
pattern are plain POSIX-shell quoting, the same in 3.2), and the line no longer contains the
text. The guard now matches only a real opener line. No Rust changed; neither test's
expectation changed.

`script/test_local_counted_bootstrap`:

- new check per shell: the text `<< 'EOM'` is in the assembled script exactly once. A repeat of
  this fault now fails with a message that names it instead of the misleading "has the
  here-document the rewrite replaces".
- the Python mirror of the rewrite was still the pre-#1549 form (two lines sent after the
  body). It now mirrors the app: bash runs `stty sane` and the `eval` from the command line,
  nothing follows the body, a script with anything else after the here-document is not
  rewritten (new check), and the counted body is the end of what is sent (new check). So the
  script now replays what the app really sends a local bash.

`script/precheck`: `test_local_counted_bootstrap` was already in the list.
`script/test_reader_marker` (#2052) was in no list; it is added (it passes, see the report).

## Rejected

- Narrowing the two tests to "the opener LINE is gone": weakens both, and leaves the
  self-matching guard.
- Removing the guard: #2051's rule is that a text which is not provably the bootstrap is never
  sent.
- Wiring the six harnesses that #2055 lists as run by hand: that issue owns the decision
  (`test_hook_forks` alone takes minutes).

## Risk

One shell line, under `WARP_IN_MSYS2` and the off-by-default `PHOSPHOR_MSYS_MANAGED_SSH=1`.
Not run on Windows/msys or on bash 3.2. What is sent grows by two bytes (the added lines are
comments, which are not sent).
