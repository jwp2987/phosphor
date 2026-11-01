#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# Fake sshd on 127.0.0.1:PORT for the r18bi ssh-typing tests. Pubkey auth for clientkey only; passwords always rejected.
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import socket,threading,os,pty,select,subprocess,sys,struct,fcntl,termios,signal,time
import paramiko
D=RUN
HK=paramiko.Ed25519Key(filename=D+'/fakessh/hostkey')
CK=paramiko.Ed25519Key(filename=D+'/fakessh/clientkey')
PORT=int(sys.argv[1]) if len(sys.argv)>1 else 2222
DROP=float(os.environ.get('FAKE_DROP','0'))
LAT=float(os.environ.get('FAKE_LAT','0'))   # artificial connect delay before auth (s)
LOG=open(D+'/raw/fakesshd.log','a',buffering=1)
def log(*a): print(time.strftime('%H:%M:%S'),*a,file=LOG)
class Srv(paramiko.ServerInterface):
    def __init__(s): s.ptys={}; s.envs={}; s.evs={}; s.reqs={}; s.fds={}
    def ev(s,ch): return s.evs.setdefault(ch.chanid,threading.Event())
    def check_channel_request(s,kind,cid): return paramiko.OPEN_SUCCEEDED if kind=='session' else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
    def get_allowed_auths(s,u): return 'publickey,password'
    def check_auth_publickey(s,u,key):
        return paramiko.AUTH_SUCCESSFUL if key==CK else paramiko.AUTH_FAILED
    def check_auth_password(s,u,p): log('password attempt rejected (len %d)'%len(p)); return paramiko.AUTH_FAILED
    def check_channel_pty_request(s,ch,term,w,h,pw,ph,modes): s.ptys[ch.chanid]=(term.decode() if isinstance(term,bytes) else term,w,h); return True
    def check_channel_shell_request(s,ch): s.reqs[ch.chanid]=('shell',None); s.ev(ch).set(); return True
    def check_channel_exec_request(s,ch,cmd): s.reqs[ch.chanid]=('exec',cmd.decode()); s.ev(ch).set(); return True
    def check_channel_env_request(s,ch,n,v): s.envs.setdefault(ch.chanid,{})[n.decode() if isinstance(n,bytes) else n]=v.decode() if isinstance(v,bytes) else v; return True
    def check_channel_window_change_request(s,ch,w,h,pw,ph):
        if ch.chanid in s.fds: fcntl.ioctl(s.fds[ch.chanid],termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0))
        return True
def serve(sock):
    try:
        t=paramiko.Transport(sock); t.add_server_key(HK)
        if LAT: time.sleep(LAT)
        srv=Srv(); t.start_server(server=srv)
        while True:
            ch=t.accept(30)
            if ch is None:
                if not t.is_active(): break
                continue
            threading.Thread(target=handle,args=(ch,srv),daemon=True).start()
    except Exception as e: log('serve err',repr(e))
def handle(ch,srv):
    srv.ev(ch).wait(10); kind,cmd=srv.reqs.get(ch.chanid,(None,None)); spty=srv.ptys.get(ch.chanid)
    env=dict(os.environ); env.update(HOME=D+'/remote_home',USER=os.environ.get('USER','tester'),LOGNAME=os.environ.get('USER','tester'),SHELL='/bin/bash',TERM=(spty[0] if spty else 'dumb'),PATH=os.environ.get('PATH',''))
    for k in ('PHOSPHOR_X',): env.pop(k,None)
    env.update(srv.envs.get(ch.chanid,{}))
    argv=['/bin/bash','-l','-i'] if kind=='shell' else ['/bin/bash','-c',cmd]
    log('session',kind,cmd,'pty' if spty else 'nopty')
    if DROP and kind=='shell':
        def dropper():
            time.sleep(DROP); log('dropping connection'); 
            try: ch.get_transport().close()
            except Exception as e: log('drop err',e)
        threading.Thread(target=dropper,daemon=True).start()
    if spty:
        pid,fd=pty.fork()
        if pid==0:
            os.chdir(D+'/remote_home'); os.execvpe(argv[0],argv,env)
        srv.fds[ch.chanid]=fd; fcntl.ioctl(fd,termios.TIOCSWINSZ,struct.pack('HHHH',spty[2],spty[1],0,0))
        while True:
            r,_,_=select.select([fd,ch],[],[],1)
            if fd in r:
                try: d=os.read(fd,65536)
                except OSError: d=b''
                if not d: break
                ch.sendall(d)
            if ch in r:
                d=ch.recv(65536)
                if not d: break
                os.write(fd,d)
            if ch.closed: break
        try: _,st=os.waitpid(pid,os.WNOHANG)
        except: st=0
        ch.send_exit_status(0)
    else:
        p=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,cwd=D+'/remote_home')
        def pump(src,sendf):
            for chunk in iter(lambda:src.read1(65536) if hasattr(src,'read1') else src.read(65536),b''): sendf(chunk)
        t1=threading.Thread(target=pump,args=(p.stdout,ch.sendall)); t2=threading.Thread(target=pump,args=(p.stderr,ch.sendall_stderr)); t1.start(); t2.start()
        def feed():
            try:
                while True:
                    d=ch.recv(65536)
                    if not d: break
                    p.stdin.write(d); p.stdin.flush()
            except Exception: pass
            try: p.stdin.close()
            except: pass
        threading.Thread(target=feed,daemon=True).start()
        p.wait(); t1.join(); t2.join(); ch.send_exit_status(p.returncode)
    try: ch.close()
    except: pass
s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',PORT)); s.listen(50)
log('listening',PORT)
while True:
    c,_=s.accept(); threading.Thread(target=serve,args=(c,),daemon=True).start()
