# Copy to ~/.config/phosphor-realapp.sh (outside the repo) and fill in. Source it before
# running any harness under script/realapp/. The real values never go in the repo: they are
# the owner's local config. Mac-side scripts need the BOX/MAC variables exported on the Mac too.
export PHOSPHOR_BOX_SSH="user@build-box.example"     # Linux build/remote-target box (ssh target)
export PHOSPHOR_BOX_HOST="build-box.example"         # same host, no user
export PHOSPHOR_MAC_SSH="user@mac-box.example"       # macOS test box
export PHOSPHOR_WIN_SSH="user@win-vm.example"        # Windows 10 test VM (standard user)
export WINUSER="WindowsUser"                         # Windows profile name, replaces @WINUSER@ (see shared/render.sh)
export PHOSPHOR_WORK="$HOME/phosphor-realapp-work"   # local workspace for harness copies, logs, shots (never committed)
export PHOSPHOR_LINUX_RUN="$PHOSPHOR_WORK/linux-run" # per-run dir used by linux/ scripts (home/, out/, raw/, dist/)
export PHOSPHOR_DEB_DIR="$HOME/phosphor-deb"         # extracted release .deb (opt/phosphor/phosphor-oss), for linux/torture
mkdir -p /tmp/phosphor-realapp "$PHOSPHOR_WORK" "$PHOSPHOR_LINUX_RUN"/{out,raw,evidence} 2>/dev/null
