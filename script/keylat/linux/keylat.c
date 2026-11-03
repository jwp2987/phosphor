// keylat: type text via XTest at human-ish delays while polling a screen region for changes.
// keylat -r x,y,w,h -d min,max -s TEXT [-e] [-w settle_ms] [-p prewait_ms] [-seed n]  (\n in TEXT = Return)
// Output (stdout): "K idx char realtime_ns" per key press, "P realtime_ns hash" per region change, "B realtime_ns hash" baseline.
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <stdint.h>
extern int XTestFakeKeyEvent(Display*, unsigned int, int, unsigned long);
static uint64_t now_rt(void){struct timespec t;clock_gettime(CLOCK_REALTIME,&t);return (uint64_t)t.tv_sec*1000000000ull+t.tv_nsec;}
static int roll=0; static KeyCode pk=0,psk=0; static Display*d; static int rx=0,ry=0,rw=100,rh=30; static uint64_t lasthash=0; static Window root;
static uint64_t grab(void){ XImage*im=XGetImage(d,root,rx,ry,rw,rh,AllPlanes,ZPixmap); if(!im)return 0;
  uint64_t h=1469598103934665603ull; size_t n=(size_t)im->bytes_per_line*im->height; unsigned char*p=(unsigned char*)im->data;
  for(size_t i=0;i<n;i+=1){h^=p[i];h*=1099511628211ull;} XDestroyImage(im); return h;}
static void poll(void){uint64_t h=grab(); if(h!=lasthash){printf("P %llu %llx\n",(unsigned long long)now_rt(),(unsigned long long)h);lasthash=h;}}
static void sleep_poll(uint64_t until_ns){ while(now_rt()<until_ns) poll(); }
static void sendkey(unsigned char c,int idx){
  KeySym ks; int shift=0;
  if(c=='\n')ks=XK_Return; else if(c=='\t')ks=XK_Tab; else if(c==3)ks=XK_c; else {char s[2]={c,0}; ks=XStringToKeysym(s); if(c==' ')ks=XK_space;
    if(ks==NoSymbol){ char nm[8]; snprintf(nm,8,"U%04X",c); ks=XStringToKeysym(nm);} }
  KeyCode kc=XKeysymToKeycode(d,ks); if(!kc){fprintf(stderr,"nokey %c\n",c);return;}
  if(XKeycodeToKeysym(d,kc,0)!=ks) shift=1;
  KeyCode sk=XKeysymToKeycode(d,XK_Shift_L), ck=XKeysymToKeycode(d,XK_Control_L);
  if(c==3) XTestFakeKeyEvent(d,ck,1,0);
  if(shift)XTestFakeKeyEvent(d,sk,1,0);
  if(roll && pk==kc){XTestFakeKeyEvent(d,pk,0,0); if(psk)XTestFakeKeyEvent(d,psk,0,0); pk=0; psk=0; XFlush(d);}
  if(roll && psk && !shift){XTestFakeKeyEvent(d,psk,0,0); psk=0; XFlush(d);}
  if(shift && roll && !psk){} 
  XTestFakeKeyEvent(d,kc,1,0); XFlush(d);
  uint64_t t=now_rt(); printf("K %d %d %llu\n",idx,c,(unsigned long long)t);
  if(roll){ if(pk){XTestFakeKeyEvent(d,pk,0,0); if(psk)XTestFakeKeyEvent(d,psk,0,0);} pk=kc; psk=shift?sk:0; XFlush(d); return; }
  XTestFakeKeyEvent(d,kc,0,0); if(shift)XTestFakeKeyEvent(d,sk,0,0); if(c==3)XTestFakeKeyEvent(d,ck,0,0); XFlush(d);
}
int main(int argc,char**argv){ int dmin=80,dmax=150,settle=1500,pre=500; const char*text="";
  for(int i=1;i<argc;i++){ if(!strcmp(argv[i],"-r"))sscanf(argv[++i],"%d,%d,%d,%d",&rx,&ry,&rw,&rh); else if(!strcmp(argv[i],"-d"))sscanf(argv[++i],"%d,%d",&dmin,&dmax);
    else if(!strcmp(argv[i],"-s"))text=argv[++i]; else if(!strcmp(argv[i],"-w"))settle=atoi(argv[++i]); else if(!strcmp(argv[i],"-p"))pre=atoi(argv[++i]); else if(!strcmp(argv[i],"-R"))roll=1; else if(!strcmp(argv[i],"-seed"))srand(atoi(argv[++i])); }
  d=XOpenDisplay(NULL); if(!d){fprintf(stderr,"no display\n");return 1;} root=DefaultRootWindow(d);
  lasthash=grab(); printf("B %llu %llx\n",(unsigned long long)now_rt(),(unsigned long long)lasthash);
  sleep_poll(now_rt()+(uint64_t)pre*1000000ull);
  size_t n=strlen(text); uint64_t next=now_rt();
  for(size_t i=0;i<n;i++){ sleep_poll(next); sendkey((unsigned char)text[i],(int)i); int dl=dmin+(dmax>dmin?rand()%(dmax-dmin+1):0); next=now_rt()+(uint64_t)dl*1000000ull; }
  if(pk){XTestFakeKeyEvent(d,pk,0,0);if(psk)XTestFakeKeyEvent(d,psk,0,0);XFlush(d);}
  sleep_poll(now_rt()+(uint64_t)settle*1000000ull); fflush(stdout); return 0; }
