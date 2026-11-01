# realapp/shared: PTY/shell experiments that need no GUI.
# Platform: any with python3 (+nu for nutest.py). Run: python3 FILE (see first comment). Restores: nothing persistent.
import os,sys,termios,fcntl,time,select,subprocess,tempfile,shutil
sys.path.insert(0,'.')
from exp import ioc,TIOCOUTQ,FIONREAD
# A: after overflow in cooked, flip slave to raw and drain: does outq decay?
m,s=os.openpty(); a=termios.tcgetattr(s); a[3]&=~termios.ECHO; termios.tcsetattr(s,termios.TCSANOW,a)
fcntl.fcntl(m,fcntl.F_SETFL,fcntl.fcntl(m,fcntl.F_GETFL)|os.O_NONBLOCK)
os.write(m,b'a'*3000); print(sys.platform,'A after 3000 cooked: outq',ioc(m,TIOCOUTQ))
a=termios.tcgetattr(s); a[3]&=~termios.ICANON; a[6][termios.VMIN]=1; termios.tcsetattr(s,termios.TCSANOW,a)
time.sleep(0.1); print(' after flip raw: outq',ioc(m,TIOCOUTQ),'slave fionread',ioc(s,FIONREAD))
fcntl.fcntl(s,fcntl.F_SETFL,fcntl.fcntl(s,fcntl.F_GETFL)|os.O_NONBLOCK)
n=0
try:
    while True: n+=len(os.read(s,65536))
except OSError: pass
print(' drained',n,'outq now',ioc(m,TIOCOUTQ))
# B: raw mode child not reading: outq as it fills
a=termios.tcgetattr(s); a[3]&=~(termios.ICANON); termios.tcsetattr(s,termios.TCSANOW,a)
log=[]
for i in range(6):
    try: k=os.write(m,b'b'*1000)
    except OSError as e: k='E%d'%e.errno
    log.append((k,ioc(m,TIOCOUTQ),ioc(s,FIONREAD)))
print(' B raw 1000B writes (ret,outq,slave_fionread):',log)
# C: shell interactive: send assignment of N bytes, then echo length
for sh in ('bash','zsh'):
    exe=shutil.which(sh)
    if not exe: print(sh,'missing'); continue
    for n in (512,1024,2048,4096,16384):
        res=[]
        for rep in range(5):
            m2,s2=os.openpty()
            args=[exe,'--noprofile','--norc','-i'] if sh=='bash' else [exe,'-f','-i']
            env=dict(os.environ,PS1='PROMPT> ',TERM='dumb',HOME=tempfile.gettempdir())
            p=subprocess.Popen(args,stdin=s2,stdout=s2,stderr=s2,start_new_session=True,env=env)
            os.close(s2)
            def rd(t):
                buf=b''
                while select.select([m2],[],[],t)[0]:
                    try: d=os.read(m2,65536)
                    except OSError: break
                    if not d: break
                    buf+=d
                return buf
            rd(0.8)
            fcntl.fcntl(m2,fcntl.F_SETFL,fcntl.fcntl(m2,fcntl.F_GETFL)|os.O_NONBLOCK)
            data=b'x=' + b'q'*n
            off=0;eag=0;t0=time.time()
            while off<len(data) and time.time()-t0<5:
                try: off+=os.write(m2,data[off:])
                except OSError: eag+=1; select.select([],[m2],[],0.05)
                rd(0.001)   # drain echo like a terminal would
            os.write(m2,b'\recho LEN=${#x}END\r')
            out=b''
            t0=time.time()
            while time.time()-t0<3 and b'END\r\n' not in out.split(b'echo LEN')[-1] :
                out+=rd(0.2)
                import re
                if re.search(rb'\nLEN=\d+END',out): break
            mm=re.search(rb'\nLEN=(\d+)END',out)
            res.append((int(mm.group(1)) if mm else None,eag))
            p.kill();p.wait();os.close(m2)
        print(sh,n,'expected',n,'got/eagain',res)
