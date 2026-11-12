# Release notes draft

Release bodies are generated from the commit log (`git-cliff`, `cliff.toml`, the
`release-meta` job in `.gitlab-ci.yml`). The lines below are the hand-written part: things a
user has to know that a commit subject does not say. Whoever cuts the release pastes the
section for that version above the generated list and empties it here. Add a line when a change
needs one; keep each to what the user sees and what to do.

## v0.2.0 (not released; was planned as v0.1.18)

### Known limits

- **Linux, no keyring: stored credentials do not survive a downgrade.** From this version the
  encrypted fallback files for stored credentials (API keys, MCP tokens, the proxy password) are
  sealed with a key that belongs to your install instead of one built into the program (#1932).
  Versions before 0.2.0 cannot read them: after going back to an older version, a machine with
  no Secret Service keyring shows those credentials as not set. Nothing is deleted, and they
  read again after upgrading. Machines with a working keyring are not affected.

- **ssh to a host whose login shell is dash or ash (or sh, ksh): a plain session, no agent tools
  on that host** (#2120). Phosphor's ssh sets up its own session only for a bash or zsh login
  shell. On a host with another POSIX login shell the pane works like a plain terminal, as it
  already does for fish and nushell (#2019): what you type goes to ssh as typed, there are no
  command blocks or app completions in the remote shell, and the agent cannot read, search, edit
  or run commands on that host. To get them, make bash or zsh the login shell on that host
  (`chsh`).

### Changed behaviour

- **A stored credential that cannot be opened is now said, once per launch** (#2095). If a
  fallback file exists but cannot be opened (missing key file, a keyring that does not answer,
  a damaged file), a notice says so instead of the credential silently showing as not set.
- **In ssh sessions the model is no longer asked for next-command or prompt suggestions unless
  you turn that on** (#2094): Settings > AI > "Next Command in SSH Sessions" and "Prompt
  Suggestions in SSH Sessions" (both off by default). Suggestions from your own history still
  appear.
