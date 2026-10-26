<!--
Use this when Phosphor fails to bootstrap a subshell over SSH (the legacy, non-tmux Warpify path).
Cloud-account or Warpify-SSO issues belong to upstream Warp. For tmux Warpification failures use
03_ssh_tmux instead. Redact hostnames, IPs, usernames and secrets.
-->

/label ~bug ~"area:ssh"

### Pre-submit checks
- [ ] I searched [existing SSH issues](../../issues?search=ssh) and found no duplicate.
- [ ] This reproduces in **Phosphor**, not only in upstream Warp.
- [ ] (optional) I attached relevant logs (`RUST_LOG=info`, SSH session output).
- [ ] I tried commenting out my system and user rc files to isolate the problem.

### Environment
- Local operating system and version:
- Local shell version (`echo $BASH_VERSION`, `echo $ZSH_VERSION`, `$PSVersionTable`):
- Remote shell version, on the SSH target:
- **Phosphor version / commit** (Settings → About, or `git rev-parse --short HEAD`):

### Shell xtrace output
Run `WARP_DEBUG_MODE=1 ssh YOUR-HOSTNAME-HERE` inside Phosphor and paste the full output from the SSH command block.

```shell
```

### Screenshots, videos and logs

### Impact
- Does this block your daily use of Phosphor? (No / Yes):
- Reproducible only in Phosphor? (Only Phosphor / also upstream Warp / also other terminals / not tested):
