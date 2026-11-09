# sshlab: the fake ssh lab

A podman container that is a fake REMOTE machine for testing Phosphor's ssh features: two OpenSSH servers, one login user per shell, scenario users
(banner, slow rc, MFA, read-only home, no tty, ...), a network fault shaper (latency, jitter, loss, rate, stall, rst, blackhole) and an HTTP control API.
The design and the cases that use it are in `docs/testing.md` section 2.15; this file is how to build, run and operate it, so that anybody can rebuild it.
No key, token, hostname or IP is in the repo: tokens and the box address come from machine-local files or masked CI variables.

## What it is made of

| piece | tool |
|---|---|
| container runtime | rootless **podman** >= 5 with **netavark** (a bridge network named `sshlab`; the default rootless `pasta` path can splice loopback traffic around the container's tc, so a "150 ms" arm could silently run at 0 ms) |
| image | `Containerfile`: Debian trixie-slim pinned by digest, apt from a `snapshot.debian.org` timestamp, nu and pwsh tarballs pinned by sha256 (`versions.env`). Targets `full` and `bare` (bare = bash + coreutils minus od/tr/base64, no scp/sftp/curl/wget, read-only root, noexec /tmp) |
| servers | OpenSSH `sshd` on container :22 (host key A) and :2222 (host key B, the "changed host key" case) |
| fault shaper | `tc` with `netem`, `ifb`, `prio` and `u32`, applied inside the container's own network namespace to ports 22/2222/7 only; `nftables` blocks new outbound connections for the lab users |
| control daemon | `rootfs/usr/local/sbin/labd.py` (PID 1 under `--init`): stdlib HTTP on :7000 with a bearer token: `/health /lease /release /fault /drop /file /ctl /tree /helper /complete /sshdlog` |
| users | `rootfs/usr/local/lib/sshlab/users.tsv` (`user shell profile sshd-Match`): `u_bash u_zsh u_fish u_nu u_pwsh u_dash u_ash u_rbash` plus scenario users `p_*`. `usersdb.py` turns it into useradd commands, `/etc/shells` and sshd `Match User` blocks |
| host tool | `labctl.py`, the ONLY caller of podman: `build start stop gc list serve selftest` (and the legacy `remote`) |
| client | `script/realapp/appci/lab.py` (modes `local`, `fixed`, legacy `remote`) and the `lab_*` cases in `appci/cases_lab.py` |

## Host requirements and one-time setup (Linux box)

Run as the account that owns the lab (on the Phosphor build box: the gitlab-runner user).

```sh
# 1. podman >= 5, rootless, with netavark and passt/pasta; plus the tools the selftest uses
sudo apt install podman netavark passt uidmap openssh-client python3
podman --version                                   # >= 5
podman info --format '{{.Host.NetworkBackend}}'   # netavark

# 2. subuid/subgid ranges for the runner user (rootless containers need them)
grep "^$USER:" /etc/subuid /etc/subgid || sudo usermod --add-subuids 100000-165535 --add-subgids 100000-165535 "$USER"

# 3. the user's systemd instance must survive logout (the slot supervisor is a --user service)
sudo loginctl enable-linger "$USER"

# 4. kernel modules for the fault shaper: a rootless container cannot load them itself.
#    Exactly these lines (installed on the build box 2026-11-09):
printf '%s\n' sch_netem sch_prio cls_u32 sch_ingress act_mirred ifb sch_tbf | sudo tee /etc/modules-load.d/sshlab.conf
for m in sch_netem sch_prio cls_u32 sch_ingress act_mirred ifb sch_tbf; do sudo modprobe "$m"; done   # once, so no reboot is needed

# 5. the control-API token: any random hex, mode 600, machine-local. NEVER print or commit it.
mkdir -p ~/.phosphor-sshlab && chmod 700 ~/.phosphor-sshlab
umask 077 && python3 -c 'import secrets;print(secrets.token_hex(32))' > ~/.phosphor-sshlab/token
```

For the Mac/Windows slots the SAME token value goes into the GitLab project variable `SSHLAB_TOKEN` (masked + protected), and the box's address into
`SSHLAB_BOX` (protected). Copy the value without displaying it, for example `glab variable set SSHLAB_TOKEN < ~/.phosphor-sshlab/token`.

**Network.** The slots publish on all interfaces, so the Mac/Windows machines must be able to reach these TCP ports on the box (allow them from the LAN only,
never from the internet): per slot ssh S, host-key-B sshd S+10, echo probe C+700 and control C. Default table: `mac 22202/22212/7702/7002`,
`windows 22203/22213/7703/7003` (ssh / host-key-B ssh / echo / control). Example: `sudo ufw allow from 192.0.2.0/24 to any port 22202:22213,7002:7003,7702:7703 proto tcp`.
The control API is plain HTTP; the token is the only protection, so keep these ports LAN-only.

## Standalone use (no CI)

```sh
cd script/realapp/sshlab
python3 labctl.py build            # image tag = sha256 of this tree; skipped when present; refuses PLACEHOLDER/malformed pins
python3 labctl.py selftest         # per user login, RTT gate at 150 ms, MFA, onesess refusal; then asserts no lab container is left
python3 labctl.py start --run mine --flavor full --ttl 3600    # JSON: cid, ports (random host ports), token, host keys
python3 labctl.py list ; python3 labctl.py stop --id <cid> ; python3 labctl.py gc
python3 labctl.py serve --slots mac:22202:7002,windows:22203:7003 --token-file ~/.phosphor-sshlab/token [--once]
```

`serve` keeps ONE persistent container per slot with the fixed ports above: it replaces a slot's container when the image tag (recorded by `build` in
`~/.phosphor-sshlab/images.json`) changes, when its 24 h TTL expires or when it died, and never while a client holds the lease (it retries later). It
reconciles on podman container events (died/remove) with a 5-minute safety re-check, and `--once` reconciles and exits. The service unit
`sshlab-serve.service` (`systemctl --user enable --now sshlab-serve`; the `sshlab-image` job installs it) runs it from `~/.phosphor-sshlab/`, where the job
also installs `labctl.py`. Slot ports must be in 1024..32767 so they never collide with the random ports of `start`.

Leasing by hand with curl (the token stays in a shell variable and is never echoed; no `set -x`):

```sh
T=$(cat ~/.phosphor-sshlab/token); H=http://127.0.0.1:7002          # the slot's control port
curl -s -H "Authorization: Bearer $T" $H/health
curl -s -H "Authorization: Bearer $T" -d "{\"run\":\"hand\",\"owner\":\"me\",\"pubkeys\":[\"$(cat ~/.ssh/id_ed25519.pub)\"]}" $H/lease   # returns users + host keys A/B
curl -s -H "Authorization: Bearer $T" -d '{"rtt_ms":150}' $H/fault ; curl -s -X DELETE -H "Authorization: Bearer $T" $H/fault
curl -s -H "Authorization: Bearer $T" -d '{"owner":"me"}' $H/release
```

A fixed slot allows ONE lease owner at a time. Pin the host keys from the lease response (`hostkeys.A.public`) in a `known_hosts` file with
`StrictHostKeyChecking yes`; never trust on first use.

Running the lab cases locally against the lab:

```sh
APPCI_LAB=local APPCI_LAB_MATRIX=quick python3 script/realapp/appci/run.py --cases 'lab_*' ...   # run.py's usual backend/binary flags
APPCI_LAB=fixed SSHLAB_BOX=<host> SSHLAB_TOKEN=<token> APPCI_LAB_SLOT=mac ...                    # attach to a slot (token from ci.env is better)
```

Settings come from the environment first, then the machine-local `~/.config/phosphor-ci/ci.env`. `APPCI_LAB=off|local|fixed|remote` (`remote` = legacy
forced-command ssh mode, kept working, not recommended); unset = `local` when podman exists, else `fixed` when `SSHLAB_BOX` and `SSHLAB_TOKEN` are set, else the
lab cases SKIP with the reason. `APPCI_LAB_SLOT` (default mac on macOS, windows on Windows), `APPCI_LAB_SLOTS` (slot table, same syntax as `serve --slots`),
`APPCI_LAB_MATRIX=quick|full|<spec>` (quick = bash/zsh/fish x LAN/150 ms x nohelper; full = 8 shells x LAN/150/300 x nohelper/helper + faults + scenario users).

## CI wiring

- `sshlab-image` (manual, box runner, `resource_group: little-thing-sshlab`): `build`, `selftest`, install `labctl.py` and the unit into the runner account, `labctl.py serve --once`, `systemctl --user daemon-reload && enable --now && restart sshlab-serve`.
- `app-test-lab-linux` (APPCI_LAB=local), `app-test-lab-macos` / `-windows` (APPCI_LAB=fixed, slot mac / windows): manual, `--cases 'lab_*'`, in their platform's GUI resource group.
- Project variables, by NAME only: `SSHLAB_BOX` (the box address), `SSHLAB_TOKEN` (masked + protected). Protected means they exist only in jobs on protected refs (main, `build/*`, `v*`); elsewhere the lab cases SKIP with "SSHLAB_TOKEN is not set".

## Rebuilding and updating

- Pins live in `versions.env`. To bump one: edit the value (look it up with network: `podman pull <image> && podman image inspect --format '{{.Digest}}'`; the nu/pwsh sha256 from the release's checksum file; the snapshot timestamp must return 200 from snapshot.debian.org), run `labctl.py build` (it refuses PLACEHOLDER or malformed values), then `labctl.py selftest`.
- The image tag is `localhost/phosphor-sshlab:<full|bare>-<first 16 hex of the tree hash>`. The hash covers every file under `sshlab/` except `labctl.py`, this README and the service unit (they are not in the image), LF-normalised, so editing the Containerfile, rootfs or pins yields a new tag and anything else does not. `build` keeps the newest 3 tags per flavor and prunes the rest by label.
- After a rebuild, `labctl.py serve --once` (the `sshlab-image` job does it) swaps the slot containers to the new tag.

## Troubleshooting

- **"tc/ifb unavailable"** in the job log / `/health` `tc:false`: the kernel modules are not loaded; do step 4 above.
- **RTT gate UNVERIFIABLE** (the arm's status is `noref`, never a pass): the measured echo RTT is not within 15 ms of the target. Usually the container is on the rootless `pasta` network instead of the `sshlab` bridge (`podman network ls`), or the ifb/netem modules are missing.
- **409 on lease / "slot is busy"**: another client holds the slot (`GET /health` shows `leased` and `lease_left`). The case SKIPs rather than share the container. The lease ends on `POST /release`, at teardown, or after 3 h; `labctl.py stop --id` of the slot container (or `serve` after its TTL) also frees it.
- **401 "bad token" / "SSHLAB_TOKEN ... rejected"**: the project variable and `~/.phosphor-sshlab/token` hold different values. Re-set one from the other; the running slot containers must be recreated to pick up a changed file (`labctl.py stop --id <slot cid>`, then `serve --once`).
- **slot unreachable**: `systemctl --user status sshlab-serve`, `labctl.py list`, the firewall ports above, and `podman logs sshlab-slot-<name>`.
