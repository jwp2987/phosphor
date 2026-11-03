#!/bin/bash
# realapp/shared: PTY/shell experiments that need no GUI.
# Platform: any with python3 (+nu for nutest.py). Run: python3 FILE (see first comment). Restores: nothing persistent.
# render.sh FILE : print FILE with the @WINUSER@ placeholder replaced by $WINUSER (from your local
# config, see ../config.example.sh). Windows scripts carry the placeholder instead of the VM's profile
# name; render before uploading, e.g.  shared/render.sh windows/lib/lib.ps1 | ssh "$PHOSPHOR_WIN_SSH" 'powershell -Command -'
# or  shared/render.sh X.ps1 > /tmp/X.ps1 && scp /tmp/X.ps1 "$PHOSPHOR_WIN_SSH":dir/
: "${WINUSER:?source your realapp config first}"
sed "s/@WINUSER@/$WINUSER/g" "$1"
