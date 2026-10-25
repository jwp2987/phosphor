# Issues filed during testing — 2026-10-24

Testing of Phosphor **v0.1.14** on a MacBook Pro (M1 Pro, macOS 26.4.1), over ssh from the Mac to the build box, and from a Linux client on a headless display. All issues below are open.

## macOS local testing (latency and torture)

| # | Title |
|---|---|
| [#961](https://github.com/jwp2987/phosphor/issues/961) | macOS: typing lag is noticeable — keystroke-to-paint ~38 ms median vs ~29 ms in Terminal.app |
| [#962](https://github.com/jwp2987/phosphor/issues/962) | macOS: Phosphorizing a zsh subshell from a bash parent echoes a truncated bootstrap and leaves zsh at a continuation prompt |
| [#963](https://github.com/jwp2987/phosphor/issues/963) | Agent-started pager (less) can't be operated from the keyboard; `q` goes to the hidden agent input |
| [#964](https://github.com/jwp2987/phosphor/issues/964) | Routing: real commands whose arguments are English words go to the agent; bare `fish` goes to the agent even when installed |
| [#965](https://github.com/jwp2987/phosphor/issues/965) | macOS: prose starting with a macOS command word runs in the shell (`say hello` speaks aloud) |
| [#966](https://github.com/jwp2987/phosphor/issues/966) | fish: first command after Phosphorizing runs but its block never appears |
| [#967](https://github.com/jwp2987/phosphor/issues/967) | Subshell bootstrap commands are written to the user's persistent shell history (bash, fish, pwsh) |
| [#968](https://github.com/jwp2987/phosphor/issues/968) | Large paste (1 MB) makes the input editor slow, keeps memory after clearing, and autodetects as an agent prompt |
| [#969](https://github.com/jwp2987/phosphor/issues/969) | Conversation sidebar shows every conversation's directory as the pane's current cwd |
| [#970](https://github.com/jwp2987/phosphor/issues/970) | macOS: UriService IPC server fails to start on every launch (relative socket path, cwd `/`) |
| [#971](https://github.com/jwp2987/phosphor/issues/971) | Locked database at runtime/startup is only logged; saves fail silently |
| [#972](https://github.com/jwp2987/phosphor/issues/972) | Generated conversation titles reverted to the raw prompt after a SIGKILL restart |
| [#973](https://github.com/jwp2987/phosphor/issues/973) | Unicode rendering gaps: ZWJ emoji, flags, bidi, combining marks, a box-drawing glyph |
| [#974](https://github.com/jwp2987/phosphor/issues/974) | zsh: PRINT_EXIT_VALUE prints `zsh: exit 1` from Phosphor's bootstrap; user preexec stderr drawn inside the command header |

## Remote, unmanaged and root shells over ssh (Mac → build box)

| # | Title |
|---|---|
| [#975](https://github.com/jwp2987/phosphor/issues/975) | Remote shells (bash --rcfile + sudo chains) outlive the ssh connection and Phosphor quitting |
| [#976](https://github.com/jwp2987/phosphor/issues/976) | Cmd-Enter is sent to the shell as Enter while the "Phosphorize subshell" banner is shown; following prose runs as a command |
| [#977](https://github.com/jwp2987/phosphor/issues/977) | No Tab completion or folder suggestions in Phosphorized remote subshells |
| [#978](https://github.com/jwp2987/phosphor/issues/978) | Remote fish subshell: completion generator command leaks into the input and fish history |
| [#979](https://github.com/jwp2987/phosphor/issues/979) | Agent in an unmanaged shell: drops first character, phantom tool call hangs, auto-sends empty lines, later writes skip approval |
| [#980](https://github.com/jwp2987/phosphor/issues/980) | Closed ssh tabs keep their local shell and ssh ControlMaster alive until the app quits; stale ControlPath sockets |
| [#981](https://github.com/jwp2987/phosphor/issues/981) | Remote server leaves an empty /tmp/.tmpXXXXXX directory per ssh session |
| [#982](https://github.com/jwp2987/phosphor/issues/982) | Tab title stays root@host after leaving sudo -i in an unmanaged shell |
| [#983](https://github.com/jwp2987/phosphor/issues/983) | Typing while ssh connects splits keystrokes between the local and remote shell |
| [#984](https://github.com/jwp2987/phosphor/issues/984) | nushell: Tab completion escapes spaces bash-style (`dir\ with\ spaces/`) |

## ksu typing lag and copy/paste (Linux client → build box)

| # | Title |
|---|---|
| [#985](https://github.com/jwp2987/phosphor/issues/985) | ssh: keystrokes to raw-input programs arrive 40–100 ms late because the remote-server proxy shares the interactive ssh connection |
| [#986](https://github.com/jwp2987/phosphor/issues/986) | ssh: `clear; ./ksu` (or `clear; cat`) leaves the running block completely blank; input still reaches the shell blind |
| [#987](https://github.com/jwp2987/phosphor/issues/987) | Can't select or copy text in a running block when the program has mouse reporting on: Shift-drag selects blocks instead of text |
| [#988](https://github.com/jwp2987/phosphor/issues/988) | Can't drag-select text in a conversation opened from the sidebar; a running command blocks reaching the agent response in the pane |

## Comments added to existing issues

| # | Comment |
|---|---|
| [#955](https://github.com/jwp2987/phosphor/issues/955#issuecomment-6385185707) | macOS: a 3 MB single-word paste freezes the app for 5+ minutes (CoreText line breaking); timings by length |
| [#960](https://github.com/jwp2987/phosphor/issues/960#issuecomment-6386452969) | The root-subshell hang also happens over ssh; stuck root shells survive closing the tab |
| [#964](https://github.com/jwp2987/phosphor/issues/964#issuecomment-6386453493) | Routing differs between macOS and the Linux remote (`cat this that`, bare `fish`) |
| [#966](https://github.com/jwp2987/phosphor/issues/966#issuecomment-6386453215) | The missing first fish block also happens on a remote host |
| [#961](https://github.com/jwp2987/phosphor/issues/961#issuecomment-6391242347) | The lag felt over ssh is mostly key delivery — tracked in #985 |

## Reports

- macOS and remote-shell report (Parts 1–4): `~/phosphor-profile/torture/REPORT.md` on the Mac
- ksu latency and copy/paste report (Parts 5–7): `REPORT-linux-ksu.md` in the session scratchpad (`ksu-linux/`)
