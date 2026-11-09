<!-- Fable 5.2 read-only audit, 2026-11-09: helper vs no-helper remote backends. Saved verbatim by the coordinator; one issue per finding. -->

# Remote backend parity audit (merge/w0, read-only)

Scope read: `crates/remote_server/src/backend/{mod,router,features,executor,refresh,shell_git_runner}.rs`, `backend/shell/{mod,write,read,buffer,tree,index,git_write,diff}.rs`, `shellcmd/{mod,frame,shell_kind,write,read,buffer,tree,index,agent_context}.rs`, `shellcmd/git/{runner,write,ops,parse}.rs`, `daemon/{fs_ops,file_context,git_write,buffers}.rs`, `capability.rs`, `client/mod.rs` (timeouts), `app/src/terminal/model/session/command_executor{,/remote_command_executor}.rs`, `app/src/remote_server/server_model.rs` (WriteFile/DeleteFile), `crates/warp_files/src/lib.rs` (the #1837 caller), `docs/design/remote-helper-parity.md`, lab cases.

Verified equal / not a problem (so you can skip them): no shell injection found — every path/arg goes through `sh_quote` (`shellcmd/mod.rs:42`) or `pwsh_quote`; section names and the nonce are validated (`frame.rs:50-108`); `git_script` quotes every argv element (`shell_git_runner.rs:56-63`). ReadFileContext caps/ranges/CRLF/binary are one shared `assemble` (`shellcmd/read.rs:367`) on both arms. Git write validation/guards are shared (`shellcmd/git/write.rs`). The API key never appears in any script or host request (`index_sync.rs:9-11`; `shellcmd/index.rs` scan has none). Nothing is typed into the pty: the only production executor is `ControlMasterChannel` (`remote_command_executor.rs:324`). `AfterOwnWrite` fires on both arms for every mutating call (`features.rs:114-117`, `router.rs:537,582,639`). Mutating calls are not retried on connection loss (`features.rs:97-104`).

## Ranked findings

**1. SECRET LEAK (shell only) — file contents, patches and commit text travel in process argv.**
`run_over_master` appends the whole rendered script to the ssh argument list (`remote_command_executor.rs:229` `ssh_args.push(command_str)`), so the base64 of every written chunk (`shellcmd/write.rs:236-239`), every staged patch (`shell_git_runner.rs:143-150` `wrap_stdin` base64s stdin into `printf %s '<b64>'`) and commit messages are visible in `ps`/`/proc/*/cmdline` on the user's machine and, on the host, as the login shell's `-c` argument (and `sh -c` again for fish/nu, `shell_kind.rs:54-57`) for the command's lifetime; auditd/execve logging captures it permanently. Helper: payload goes over the protocol stream (`daemon/fs_ops.rs:214`), never argv. Trigger: accept an agent edit of a file containing a credential on a shared host; `ps -o args` on the host shows its base64. Fix: `ExecRequest.stdin` already exists and `ControlMasterChannel` pipes it (`remote_command_executor.rs:246-263`); make the write command `base64 -d >> "$tmp"` / `cat >> "$tmp"` read stdin, and `wrap_stdin` pass the patch as stdin; pwsh: `[Console]::OpenStandardInput()`.

**2. CLOBBER — buffer save after a helper→shell fallback is unguarded.**
Open a buffer while the helper serves `Buffers` (state lives in the helper), the helper dies or answers UNSUPPORTED, then save: `router.rs:630-637` routes to `shell.save_buffer_content`, which finds no remembered base and writes unconditionally with only a warn (`backend/shell/buffer.rs:237-241`). Helper path: conflict detection via its own copy. Trigger: `c_helper_killed`-style kill while an editor tab is open, edit the file on the host, Ctrl-S. Fix: seed `ShellBuffers` with `Expect::Sha256(sha256(content))` on every successful `open_buffer` whichever arm served it (router `open_buffer` wrapper), or refuse the save ("reload first") when no base is known. (Reverse case — opened on shell, helper connects, save → helper has no buffer → error, not clobber.)

**3. WRONG RESULT / CLOBBER — shell write to a path that is a directory "succeeds".**
`shellcmd/write.rs:250-252`: `mv -f -- "$tmp" "$f"` with `$f` a directory moves the temp file INTO it (`dir/<name>.phosphor-tmp-<nonce>`), exit 0; no `Expect` (plain `write_file`) → reported written. Helper: `std::fs::write` fails EISDIR (`fs_ops.rs:236`). With `Expect::Absent`/`Stamp`, `-e` is true for a dir so it conflicts; with no expect it is silent. Trigger: agent proposes creating `src/foo` where `src/foo/` exists. Fix: before mv `[ -d "$f" ] && { rm -f -- "$tmp"; echo ISDIR; exit 7; }` (pwsh `Test-Path -PathType Container`), new exit code in `write_failure_message`.

**4. DATA LOSS (low probability) — helper `WriteFile` is not atomic.**
`fs_ops.rs:230-237` does `create_dir_all` + `std::fs::write` (truncate then write): a crash/ENOSPC mid-write leaves a truncated file and a watcher/editor sees a half file. The shell arm is temp+rename (`write.rs:227-252`) and the helper's own buffer save is temp+rename with mode kept (`daemon/buffers.rs:119-130`). Fix: reuse the `buffers.rs` temp+rename for `write_file`.

**5. WRONG RESULT — stale `*.phosphor-tmp-<nonce>` left in the repo on a drop/timeout mid multi-chunk write (shell only).**
Chunks are separate round trips (`backend/shell/write.rs:83-109`); a connection drop, a 60 s timeout (`remote_command_executor.rs:45,266-273`, `kill_on_drop`) or a cancelled future between chunks leaves the temp file beside the target, visible in the file tree, `git status` (untracked) and the diff panel; nothing cleans it (`grep phosphor-tmp` finds no sweeper). Helper: request arrives whole or not at all. Fix: best-effort `rm -f -- "$tmp"` on any non-OK/err path; and use a hidden name `.<name>.phosphor-tmp-*` like `buffers.rs:119`.

**6. WRONG RESULT (Windows/pwsh hosts) — writes over ~6 KiB cannot run.**
`WRITE_CHUNK_BYTES` 64 KiB (`write.rs:20`) → 88 KiB base64 → `render_pwsh` → UTF-16 → base64 `-EncodedCommand` ≈ 235 K chars (`shell_kind.rs:59-67`) against Windows' 32 767-char command line. Every pwsh write >~6 KiB fails (EXIT unknown / spawn error). Fix: pwsh chunk ≤ 6 KiB raw, or stdin (see 1).

**7. MISSING FEATURE — shell git stage/hunk with a patch > ~90 KiB fails (E2BIG).**
`wrap_stdin` puts the whole base64 patch in one `sh -c` argument (`shell_git_runner.rs:143-150`), Linux `MAX_ARG_STRLEN` 128 KiB. Helper: real stdin (`daemon/git_runner.rs`). Trigger: stage a large generated-file hunk from the diff panel in shell mode → "Failed to execute git command: Argument list too long". Fix: `ExecRequest.stdin`.

**8. MISSING FEATURE — codebase scan dies on large repos (shell only).**
One command hashes the whole repo (`shellcmd/index.rs:37-56`); output over `MAX_OUTPUT_BYTES` 10 MiB (~100 k files) → `run_script` returns `Unavailable("remote output over the ... cap")` (`backend/shell/mod.rs:197-202`), and >60 s of `sha256sum` → timeout. Helper streams via FrameChunk, no cap. Fix: page the listing (`git ls-files -z | sed -n`), hash per page.

**9. MISSING FEATURE — `get_branches` / `get_committed_branch_files` are helper-only.**
Both are `unimplemented` on the shell (`features.rs:139-140`) AND their callers bypass the router, calling the client directly (`app/src/code_review/diff_state_remote.rs:709`, `app/src/code_review/git_dialog/pr.rs:228`), so without a helper the branch picker/PR file list is empty with no route line. Fix: route through `RoutedBackend` and add `git for-each-ref` / `git diff --name-status` builders.

**10. MISSING FEATURE — 7 MiB read cap on the shell.**
`read_file_bytes_shell` (`backend/shell/write.rs:18,194-196`) and `open_buffer_shell` (`buffer.rs:33`) refuse files > 7 MiB ("too large to read in shell mode"); helper's chunked `ReadFileChunk` loops unbounded (`fs_ops.rs:129-159`). Consequence: agent edits (which pre-read via `read_file_bytes`, `warp_files/src/lib.rs:1406`) and editor opens of big files fail only in shell mode. Fix: chunked `dd skip=` sections.

**11. MISSING FEATURE (Windows) — on pwsh hosts there is no git at all in shell mode** (`shell_git_runner.rs:118-121`), no index (`backend/shell/index.rs:121-126`), no skills/rules (`shellcmd/agent_context.rs:154-156,179-181,190-192`); helper is not available on Windows either, so status chip, diff panel, PR chip, git writes, index and skills are absent. Also pwsh ranged reads re-decode and re-encode text as UTF-8 (`read.rs:569-572`) so non-UTF-8 files come back altered.

**12. WRONG RESULT — different timeouts and kill semantics.** Shell: 60 s per command, local ssh client killed on drop → remote command gets SIGHUP mid-run (`remote_command_executor.rs:45,266-273`). Helper: 120 s request timeout (`client/mod.rs:46`), job keeps running then Abort. A slow `git push`/`pull` or scan fails only on the shell arm, and a push killed mid-transfer is reported as a generic failure.

**13. WRONG RESULT — `mv` replaces the inode (shell only).** Hard links break, ownership/ACLs/xattrs are lost; only the mode is copied (`write.rs:251`). Helper writes in place (`fs_ops.rs:236`), keeping the inode. Shared-group repos see files change owner after an agent edit.

**14. COSMETIC — `~` handling differs.** Shell write/delete expand `~`/`~/` on the host (`write.rs:203-215,315-323`); helper `WriteFile`/`DeleteFile` use the path verbatim (`fs_ops.rs:212-216,265`) → `./~/x` relative to the daemon's cwd. Shell git ops pass `repo_path` raw (`backend/shell/git_write.rs:46`) → `cd '~/r'` fails with "Git command failed"; helper expands+canonicalizes and says "Invalid repo_path" (`daemon/git_write.rs:27-30`). App callers send absolute paths today, so low impact.

**15. COSMETIC — `ListDirectory` on the shell has no sizes, mtimes or `Symlink` kind** (`backend/shell/tree.rs:338-344`) vs helper (`fs_ops.rs:51-91`).

**16. COSMETIC — symlink edge cases.** `Expect::Absent` on the shell is `[ -e ]` (dangling symlink = absent, then `mv` replaces the link); helper uses `symlink_metadata` (dangling link = present → conflict) (`write.rs:143` vs `fs_ops.rs:202-204`). `readlink -f` missing (macOS < 12.3) → `t` empty → `mv` replaces the symlink instead of writing through (`write.rs:213`).

**17. COSMETIC / platform — unknown login shells.** `shell_kind_for_remote` maps only Bash|Zsh|Fish|Nu|PowerShell (`command_executor.rs:387-398`); csh/tcsh get POSIX scripts and every call fails "unreadable remote output" instead of the designed `Unavailable`. dash/ash/busybox: the scripts are POSIX-clean (checked `$((..))`, `${n%/}`, `printf '\0'`, `head -c`, `stat -c`→`stat -f` fallback, `find -printf`→`ls -1Ap` fallback). nu wrapping needs raw-string support (`r#'...'#`, `shell_kind.rs:56-57`) — older nu fails.

**18. BY DESIGN but a parity consequence — index store location.** Shell arm stores app-side keyed `user@host\nrepo` (`backend/shell/index.rs:71-73`, design §g "No-helper ... store app-side"); helper stores on the host. Installing/removing the helper switches store → a full re-embed (API cost) and until then different search results. The owner's "vectors on the host" holds only for the helper arm.

## Lab coverage (what would / would not catch each)

- `c_diff_apply_remote` (cases_lab_remote.py:71): create+delete, route line, pty untouched. Misses 1, 3, 5. Cheapest extension: during the write, host `ps -o args` must not contain the base64 (1); write to a path that is a directory must fail (3); `ls` the dir after a `lab.drop` mid-write (5).
- `c_lab_agent_edit_conflict` / `c_lab_agent_delete_conflict` / `c_lab_agent_edit_endings` / `c_lab_agent_edit_touch_only` (cases_agent_file_edits.py:417-471): guard, sha, CRLF on both arms — good for #1837; miss 2 (no helper kill between open and save), 3, 4, 10 (small files only).
- `c_remote_edit` (cases_lab_buffers.py:38): editor layer explicitly UNVERIFIABLE → misses 2.
- `c_helper_killed` / `c_route_unsupported` (cases_lab_helper.py:457-473): only read families after the kill → miss 2.
- `c_agent_remote_drop` (cases_agent_remote.py:200): drop during `run_shell_command` only → misses 5, 12.
- `c_git_write_remote` (cases_lab_git.py:50): small commit/push/pull → misses 7, 12, 9 (no branch picker).
- `c_codebase_index_remote` (cases_lab_index.py:45): small repo, checks the fake provider log (good evidence for "key stays local") → misses 8, 18.
- `c_diff_panel_remote` / `c_gh_chip_remote`, `c_file_tree_remote`, `c_skills_remote`, `c_read_files_remote`: cover their families' happy paths; `c_file_tree_remote` does not look at `ListDirectory` fields (15).
- No pwsh/nu/dash/ash arm exists (lab.py:200: `"fish" if user == "u_fish" else "bash"`) → 6, 11, 17 are entirely untested; `frame.rs:137-139` itself says pwsh is "best effort until the lab's u_pwsh shell proves it".

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/crates/remote_server/src/shellcmd/write.rs
- /home/winters/git/phosphor-merge/crates/remote_server/src/backend/shell/write.rs
- /home/winters/git/phosphor-merge/crates/remote_server/src/backend/shell/buffer.rs
- /home/winters/git/phosphor-merge/crates/remote_server/src/backend/shell_git_runner.rs
- /home/winters/git/phosphor-merge/app/src/terminal/model/session/command_executor/remote_command_executor.rs
