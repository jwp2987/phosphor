# realapp/shared: PTY/shell experiments that need no GUI.
# Platform: any with python3 (+nu for nutest.py). Run: python3 FILE (see first comment). Restores: nothing persistent.
import os,sys,termios,fcntl,struct,time,subprocess,errno,select,tempfile
sys.path.insert(0,'.')
from exp import ioc,TIOCOUTQ,FIONREAD,CHILD
# chunked writes into cooked tty, child not reading; outq/FIONREAD after each chunk
for chunk in (256,512,1000,1024):
    m,s=os.openpty(); a=termios.tcgetattr(s); a[3]&=~termios.ECHO; termios.tcsetattr(s,termios.TCSANOW,a)
    fcntl.fcntl(m,fcntl.F_SETFL,fcntl.fcntl(m,fcntl.F_GETFL)|os.O_NONBLOCK)
    log=[]; tot=0
    for i in range(8):
        try: k=os.write(m,b'a'*chunk)
        except OSError as e: k='E%d'%e.errno
        else: tot+=k
        log.append((k,ioc(m,TIOCOUTQ),ioc(s,FIONREAD)))
    print(sys.platform,'cooked-nonl chunk',chunk,'(ret,outq,slave_fionread):',log)
    # now send newline and see what slave can read
    try: os.write(m,b'\n')
    except OSError as e: print(' nl err',e)
    time.sleep(0.1); print('  after NL: slave fionread',ioc(s,FIONREAD),'outq',ioc(m,TIOCOUTQ))
    os.close(m);os.close(s)
