# realapp/shared: PTY/shell experiments that need no GUI.
# Platform: any with python3 (+nu for nutest.py). Run: python3 FILE (see first comment). Restores: nothing persistent.
import pty, os, sys, time, select
ESC = sys.argv[1]  # marker esc expr
lines = [
 " $env.__wbh = $env.config.history.ignore_space_prefixed; $env.config.history.ignore_space_prefixed = true",
 " $env._wbs = r#'print \"hello-from-script\"@@NL@@$env.config.history.ignore_space_prefixed = $env.__wbh'#",
 " const _wbf = ($nu.cache-dir | path join 'phosphor-nu-1.nu'); mkdir $nu.cache-dir; (if ($env.WARP_BOOTSTRAPPED? | default '' | is-not-empty) { '' } else if (($env._wbs? | default '' | str length) == 85) { $env._wbs | str replace -a '@@NL@@' (char nl) | str replace -a '@@CR@@' (char cr) } else { print \"skipped\" }) | save -f $_wbf",
 " source $_wbf",
 " rm -f $_wbf; hide-env -i _wbs",
]
mk = lambda i: f'; print -n $"({ESC}{"" if False else ""})]7771;42;{i}(char bel)"'
pid, fd = pty.fork()
if pid == 0:
    os.execv("/opt/homebrew/bin/nu", ["nu", "--no-config-file"])
out = b""
def rd(t):
    global out
    end = time.time()+t
    while time.time() < end:
        r,_,_ = select.select([fd],[],[],0.1)
        if r:
            try: out += os.read(fd, 65536)
            except OSError: return
rd(2)
for i,l in enumerate(lines):
    n = len(lines)-1
    os.write(fd, (l + (mk(i) if i<n else "") + "\r").encode())
    rd(4)
os.write(fd, b"print $env.config.history.ignore_space_prefixed\r"); rd(1)
os.write(fd, b"exit\r"); rd(1)
import re
t=out.decode(errors='replace');t=re.sub(r'\x1b\[[0-9;?]*[A-Za-z]','',t);print(t[-1800:].replace('\x07','<BEL>').replace('\x1b','<ESC>'))
