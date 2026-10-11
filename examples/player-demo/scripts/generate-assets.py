#!/usr/bin/env python3
"""Generate the small, deterministic art/audio pack for Player Demo."""
import math, os, random, struct, wave, zlib
ROOT=os.path.dirname(os.path.dirname(__file__)); A=os.path.join(ROOT,'assets')
def png(path,w,h,fn,alpha=False):
    rows=[]
    for y in range(h):
        row=bytearray([0]);
        for x in range(w):
            p=fn(x,y); row.extend(p if alpha else p[:3])
        rows.append(bytes(row))
    def chunk(t,d): return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
    raw=b''.join(rows); out=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6 if alpha else 2,0,0,0))+chunk(b'IDAT',zlib.compress(raw,9))+chunk(b'IEND',b'')
    os.makedirs(os.path.dirname(path),exist_ok=True); open(path,'wb').write(out)
def bg(name,base,accent):
    def f(x,y):
        t=y/720; n=((x*17+y*31)%23)-11; return tuple(max(0,min(255,int(base[i]*(1-t)+accent[i]*t+n))) for i in range(3))+(255,)
    png(os.path.join(A,'bg',name+'.png'),1280,720,f)
def sprite(name,color,emotion):
    def f(x,y):
        cx=300
        if y<120 or y>570:return (0,0,0,0)
        if (y<220 and (x-cx)**2+(y-170)**2<95**2) or (y>=220 and abs(x-cx)<110):
            if y<220 and emotion in ('worried','sad') and 180<y<195 and (x-cx)**2<45**2:return (30,20,40,255)
            if y<220 and emotion in ('happy','hopeful') and 190<y<205 and abs(x-cx)<55:return (240,190,180,255)
            return color+(255,)
        return (0,0,0,0)
    png(os.path.join(A,'char',name+'.png'),600,600,f,True)
def wav(path,notes,seconds=12):
    rate=22000; n=int(rate*seconds); os.makedirs(os.path.dirname(path),exist_ok=True)
    with wave.open(path,'wb') as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
        for i in range(n):
            t=i/rate; s=sum(math.sin(2*math.pi*f*t) for f in notes)/len(notes); w.writeframes(struct.pack('<h',int(7000*s)))
def main():
    for n,b,c in [('harbor-dawn',(35,75,105),(190,165,120)),('harbor-noon',(55,110,145),(230,210,155)),('harbor-night',(12,30,58),(65,90,140)),('lighthouse-dawn',(95,125,155),(220,185,135)),('lighthouse-noon',(115,145,170),(235,220,170)),('lighthouse-night',(18,28,62),(90,70,125)),('title',(20,24,55),(100,70,130))]: bg(n,b,c)
    for who,col in [('mara',(170,75,85)),('elio',(60,120,180)),('liora',(165,130,210))]:
        for e in ['neutral','happy','worried','angry','hopeful','sad','surprised']:
            sprite(who+'-'+e,col,e)
    for who,col in [('mara',(170,75,85)),('elio',(60,120,180)),('liora',(165,130,210))]:
        for costume,shift in [('plain',(0,0,0)),('coat',(20,20,20)),('raincloak',(-25,15,30)),('uniform',(25,-15,-20))]:
            c=tuple(max(0,min(255,v+shift[i])) for i,v in enumerate(col))
            sprite(who+'-'+costume+'-neutral',c,'neutral')
    png(os.path.join(A,'cg','reunion.png'),1280,720,lambda x,y:(210,170,115,255))
    for mood,notes in [('calm',[220,277]),('tense',[146,233]),('hopeful',[196,247])]: wav(os.path.join(A,'audio',mood+'.wav'),notes)
    wav(os.path.join(A,'sfx','bell.wav'),[880,1320],1); wav(os.path.join(A,'sfx','glass.wav'),[1200,1700],1)
    png(os.path.join(A,'ui','title.png'),1280,720,lambda x,y:(20+int(80*y/720),24+int(50*y/720),55+int(90*y/720),255))
    wav(os.path.join(A,'ui','title.wav'),[110,165],16)
if __name__=='__main__': main()
