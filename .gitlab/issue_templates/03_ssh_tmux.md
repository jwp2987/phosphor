<!--
Use this only when SSH Warpification (subshell bootstrap over SSH with tmux) fails in Phosphor.
Cloud-account or Warpify-SSO issues belong to upstream Warp. For non-tmux SSH bootstrap failures
use 04_ssh_legacy instead. Redact hostnames, IPs, usernames and secrets.
-->

/label ~bug ~"area:ssh"

### Pre-submit checks
- [ ] I searched [existing SSH issues](../../issues?search=ssh) and found no duplicate.
- [ ] This reproduces in **Phosphor**, not only in upstream Warp.
- [ ] (optional) I attached relevant logs (`RUST_LOG=info`, SSH/tmux session output).

### Shell output
Copy the terminal block (top-right icon -> Copy) and paste it here.

```shell
```

### Screenshots, videos and logs

### Environment
- Local operating system and version (e.g. macOS 14.5):
- Remote OS and version, the SSH target (e.g. Ubuntu 22.04):
- tmux version on the remote (`tmux -V`):
- **Phosphor version / commit** (Settings → About, or `git rev-parse --short HEAD`):
