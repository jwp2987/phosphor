// realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
// Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
// Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
// Restores: scratch profile only; the owner's config is only read (copied) and never printed.
// kseq: scripted XTest keystroke sender with ms timing. Tokens in argv (or stdin lines):
//   dNN      set inter-key delay (ms) for following keys (default 30)
//   wNN      wait NN ms
//   'text    type literal text (rest of token; use _ for space? no: use S for space key token) 
//   <Name>   press+release keysym Name (Right, Tab, BackSpace, Return, End, Escape, Down, Up, Left)
//   <C-x>    ctrl+x   <A-f> alt+f  <S-Tab> shift+tab
//   xN<..>   repeat: use rN token before: rN applies to next key/text token N times
//   roll     toggle rollover mode (press next before releasing previous)
//   mark:L   print marker with timestamp
// Output: "K <label> <realtime_ns>" at each press.
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <stdint.h>
#include <pthread.h>
#include <unistd.h>
static int prx=0,pry=0,prw=0,prh=0; static volatile int pstop=0;
static pthread_t pth;
extern int XTestFakeKeyEvent(Display*, unsigned int, int, unsigned long);
static Display*d; static int roll=0; static KeyCode pend=0, pendsh=0;
static uint64_t now(void){struct timespec t;clock_gettime(CLOCK_REALTIME,&t);return (uint64_t)t.tv_sec*1000000000ull+t.tv_nsec;}
static void sleep_ms(double ms){struct timespec t,r;uint64_t ns=(uint64_t)(ms*1e6);t.tv_sec=ns/1000000000ull;t.tv_nsec=ns%1000000000ull;nanosleep(&t,&r);}
static void relpend(void){ if(pend){XTestFakeKeyEvent(d,pend,0,0); if(pendsh)XTestFakeKeyEvent(d,pendsh,0,0); XFlush(d); pend=0;pendsh=0;} }
static void press(KeySym ks,int mods,const char*label){
  KeyCode kc=XKeysymToKeycode(d,ks); if(!kc){fprintf(stderr,"nokey %s\n",label);return;}
  int shift=(mods&1); if(XKeycodeToKeysym(d,kc,0)!=ks && XKeycodeToKeysym(d,kc,1)==ks) shift=1;
  KeyCode sk=XKeysymToKeycode(d,XK_Shift_L), ck=XKeysymToKeycode(d,XK_Control_L), ak=XKeysymToKeycode(d,XK_Alt_L);
  int rollhere=roll && !shift && !(mods&6);
  if(!rollhere) relpend(); else if(pend==kc){ relpend(); usleep(6000);}
  if(mods&2)XTestFakeKeyEvent(d,ck,1,0); if(mods&4)XTestFakeKeyEvent(d,ak,1,0);
  if(shift)XTestFakeKeyEvent(d,sk,1,0);
  XTestFakeKeyEvent(d,kc,1,0); XFlush(d);
  printf("K %s %llu\n",label,(unsigned long long)now()); fflush(stdout);
  if(rollhere){ relpend(); pend=kc; pendsh=shift?sk:0; }
  else { XTestFakeKeyEvent(d,kc,0,0); if(shift)XTestFakeKeyEvent(d,sk,0,0);
    if(mods&2)XTestFakeKeyEvent(d,ck,0,0); if(mods&4)XTestFakeKeyEvent(d,ak,0,0); XFlush(d);} }
static void textkey(unsigned char c,double dl){ KeySym ks; char s[2]={c,0}; char lab[8];
  if(c==' ')ks=XK_space; else if(c=='\n')ks=XK_Return; else { ks=XStringToKeysym(s); if(ks==NoSymbol){char nm[16];snprintf(nm,16,"U%04X",c);ks=XStringToKeysym(nm);} }
  snprintf(lab,8,"%c",c<32?'?':c); press(ks,0,lab); sleep_ms(dl); }
static void run(char*tok,double*dl,int*rep){
  if(tok[0]=='d'&&tok[1]>='0'&&tok[1]<='9'){*dl=atof(tok+1);return;}
  if(tok[0]=='w'&&tok[1]>='0'&&tok[1]<='9'){relpend();sleep_ms(atof(tok+1));return;}
  if(tok[0]=='r'&&tok[1]>='0'&&tok[1]<='9'){*rep=atoi(tok+1);return;}
  if(!strcmp(tok,"roll")){roll=!roll;relpend();return;}
  if(!strncmp(tok,"mark:",5)){printf("M %s %llu\n",tok+5,(unsigned long long)now());fflush(stdout);return;}
  int n=*rep; *rep=1;
  if(tok[0]=='\''){ for(int k=0;k<n;k++) for(char*p=tok+1;*p;p++) textkey((unsigned char)*p,*dl); return; }
  if(tok[0]=='<'){ char nm[64]; strncpy(nm,tok+1,63); nm[63]=0; char*e=strchr(nm,'>'); if(e)*e=0; int mods=0; char*k=nm;
    while(k[1]=='-'){ if(k[0]=='C')mods|=2; else if(k[0]=='A')mods|=4; else if(k[0]=='S')mods|=1; k+=2; }
    KeySym ks=XStringToKeysym(k); for(int i=0;i<n;i++){ press(ks,mods,k); sleep_ms(*dl);} return; }
  fprintf(stderr,"bad token %s\n",tok);
}
static void*poller(void*a){ Display*pd=XOpenDisplay(NULL); Window root=DefaultRootWindow(pd); uint64_t last=0;
  while(!pstop){ XImage*im=XGetImage(pd,root,prx,pry,prw,prh,AllPlanes,ZPixmap); if(!im)continue;
    uint64_t h=1469598103934665603ull; size_t n=(size_t)im->bytes_per_line*im->height; unsigned char*p=(unsigned char*)im->data;
    for(size_t i=0;i<n;i+=4){h^=*(uint32_t*)(p+i);h*=1099511628211ull;} XDestroyImage(im);
    if(h!=last){ printf("P %llu %llx\n",(unsigned long long)now(),(unsigned long long)h); fflush(stdout); last=h; } }
  return 0; }
int main(int argc,char**argv){ int a0=1;
  if(argc>2&&!strcmp(argv[1],"-P")){ sscanf(argv[2],"%d,%d,%d,%d",&prx,&pry,&prw,&prh); a0=3; }
  d=XOpenDisplay(NULL); if(!d){fprintf(stderr,"no display\n");return 1;}
  if(a0==3){ pthread_create(&pth,0,poller,0); usleep(50000); }
  double dl=30; int rep=1;
  for(int i=a0;i<argc;i++){ char*t=strdup(argv[i]); run(t,&dl,&rep); }
  relpend(); XSync(d,False); if(a0==3){ usleep(300000); pstop=1; pthread_join(pth,0);} return 0; }
