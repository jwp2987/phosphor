<!--
Bug report for Phosphor, a community fork of Warp focused on BYOP (bring-your-own-provider) AI,
local credentials and a privacy-first terminal. Search existing issues first. For SSH-specific
failures use the 03_ssh_tmux or 04_ssh_legacy templates.

OUT OF SCOPE (do not file here): Warp cloud account / billing / AI quota / Warpify SSO / Warp Drive
cloud sync (upstream Warp's), and anything only reproducible with Warp's first-party cloud agent
(Phosphor ships it disabled).

Redact API keys, tokens and personal data before posting.
-->

/label ~bug

### Pre-submit checks
- [ ] I searched [existing bug issues](../../issues?label_name%5B%5D=bug) and found no duplicate.
- [ ] This bug is in Phosphor itself, not upstream Warp's cloud account, billing, Warpify SSO or cloud-only features.
- [ ] (optional) I attached relevant logs (`RUST_LOG=info` output, panic traces).
- [ ] (optional) If the bug is in the AI / BYOP path, I included the provider, base URL pattern (redacted) and model id.

### Describe the bug

### To reproduce
1.
2.
3.

### Expected behavior

### Screenshots, videos and logs

### Environment
- Operating system (macOS / Linux / Windows):
- OS version (e.g. macOS 14.5, Debian 12):
- Shell and version (`bash --version`, `zsh --version`, `$PSVersionTable`):
- **Phosphor version / commit** (Settings → About, or `git rev-parse --short HEAD`):
- AI / BYOP path involved? (No / Yes):
- BYOP provider, if applicable (OpenAI / Anthropic / Ollama / ... - never paste keys):

### Regression?
- [ ] No, it has always happened
- [ ] Yes, it started after a recent build / upstream sync
- [ ] Unknown

Last known good build or date, if a regression:

### Additional context
On Linux, say X11 or Wayland. For graphics problems run with `RUST_LOG=wgpu_core=info,wgpu_hal=info MESA_DEBUG=1 EGL_LOG_LEVEL=debug` and attach `eglinfo`.

### Impact
- Does this block your daily use of Phosphor? (No / Yes):
- Reproducible only in Phosphor? (Only Phosphor / also upstream Warp / also other terminals / not tested):
