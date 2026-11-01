// realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
// Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
// Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
// Restores: scratch profile only; the owner's config is only read (copied) and never printed.
#include <stdio.h>
#include <time.h>
#include <stdlib.h>
#include <string.h>
int main(int c,char**v){struct timespec t;clock_gettime(CLOCK_REALTIME,&t);
 const char*f=getenv("REC_LOG");if(!f)return 1;FILE*o=fopen(f,"a");
 fprintf(o,"R %llu",(unsigned long long)t.tv_sec*1000000000ull+t.tv_nsec);
 for(int i=1;i<c;i++)fprintf(o,"\t%s",v[i]);fprintf(o,"\n");fclose(o);puts("ok");return 0;}
