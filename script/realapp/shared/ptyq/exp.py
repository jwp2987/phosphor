# realapp/shared: PTY/shell experiments that need no GUI.
# Platform: any with python3 (+nu for nutest.py). Run: python3 FILE (see first comment). Restores: nothing persistent.
import os, sys, pty, termios, tty, fcntl, struct, time, select, subprocess, json, errno, hashlib, tempfile
LINUX = sys.platform.startswith('linux')
TIOCOUTQ = 0x5411 if LINUX else 0x40047473
FIONREAD = 0x541B if LINUX else 0x4004667f
def ioc(fd, req):
    try:
        return struct.unpack('i', fcntl.ioctl(fd, req, b'\0\0\0\0'))[0]
    except OSError as e:
        return -e.errno
CHILD = r'''
import os,sys,termios,tty,time,select,hashlib
mode,delay,out=sys.argv[1],float(sys.argv[2]),sys.argv[3]
fd=0
if mode=='raw': tty.setraw(fd)
elif mode=='cooked_flip': pass   # stay cooked, flip to raw after delay
open(out+'.ready','w').write('1')
time.sleep(delay)
if mode=='cooked_flip':
    a=termios.tcgetattr(fd); a[3]&=~(termios.ICANON|termios.ECHO); a[6][termios.VMIN]=1; a[6][termios.VTIME]=0
    termios.tcsetattr(fd,termios.TCSANOW,a)
if mode=='cooked_nl':
    a=termios.tcgetattr(fd); a[3]&=~termios.ECHO; termios.tcsetattr(fd,termios.TCSANOW,a)
buf=b''
while True:
    r,_,_=select.select([fd],[],[],0.7)
    if not r: break
    try: d=os.read(fd,65536)
    except OSError: break
    if not d: break
    buf+=d
open(out,'w').write("%d %s"%(len(buf),hashlib.md5(buf).hexdigest()))
'''
def payload(n, nl):
    if not nl: return (b'abcdefghijklmnopqrstuvwxyz0123456789'*(n//36+1))[:n]
    line=b'x'*63+b'\n'
    return (line*(n//64+1))[:n]
def run(mode, n, nl, delay):
    d=tempfile.mkdtemp(); out=d+'/o'
    m,s=os.openpty()
    a=termios.tcgetattr(s); a[3]&=~termios.ECHO; termios.tcsetattr(s,termios.TCSANOW,a)
    p=subprocess.Popen([sys.executable,'-c',CHILD,mode,str(delay),out],stdin=s,stdout=s,stderr=s,start_new_session=True,close_fds=True)
    while not os.path.exists(out+'.ready'): time.sleep(0.01)
    time.sleep(0.05)
    fcntl.fcntl(m,fcntl.F_SETFL,fcntl.fcntl(m,fcntl.F_GETFL)|os.O_NONBLOCK)
    data=payload(n,nl); off=0; rets=[]; eag=0; pollout_false=0; t0=time.time()
    while off<len(data) and time.time()-t0<3:
        try:
            k=os.write(m,data[off:]); rets.append(k); off+=k
        except OSError as e:
            if e.errno==errno.EAGAIN: eag+=1; rets.append('EAGAIN')
            else: rets.append('E%d'%e.errno); break
        pf=select.poll(); pf.register(m,select.POLLOUT)
        if not pf.poll(0): pollout_false+=1
        if rets and rets[-1]=='EAGAIN': pf.poll(50)
    outq=ioc(m,TIOCOUTQ); inq_s=ioc(s,FIONREAD)
    # drain master echo-less
    p.wait(timeout=delay+5)
    try: got=open(out).read().split()
    except: got=['-1','']
    try:
        while select.select([m],[],[],0)[0]: os.read(m,65536)
    except OSError: pass
    os.close(m); os.close(s)
    return dict(sent=off,rets=rets[:6],nrets=len(rets),eagain=eag,pollout_false=pollout_false,outq=outq,slave_fionread=inq_s,
                recv=int(got[0]),hash_ok=(len(got)>1 and got[1]==hashlib.md5(data[:int(got[0])]).hexdigest()) if got[0]!='-1' else None)
if __name__=='__main__':
    reps=int(sys.argv[1]); res=[]
    cases=[('cooked_flip',0),('cooked_nl',1),('raw',0),('raw',1)]
    for mode,nl in cases:
        for delay in (1.0,0.0):
            for n in (512,1024,2048,4096,16384):
                rows=[run(mode,n,nl,delay) for _ in range(reps)]
                res.append(dict(mode=mode,nl=nl,child_reads_after_s=delay,n=n,rows=rows))
                print(mode,nl,delay,n,'recv',sorted(set(r['recv'] for r in rows)),'sent',sorted(set(r['sent'] for r in rows)),
                  'eagain',sum(r['eagain'] for r in rows),'pofalse',sum(r['pollout_false'] for r in rows),
                  'outq',sorted(set(r['outq'] for r in rows)),'slaveq',sorted(set(r['slave_fionread'] for r in rows)),flush=True)
    json.dump(res,open(sys.argv[2],'w'))
