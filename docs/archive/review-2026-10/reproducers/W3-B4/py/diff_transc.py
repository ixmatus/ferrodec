import random, subprocess, sys, collections, time
import mpmath
from decimal import *
random.seed(int(sys.argv[1])); PRECS=[int(x) for x in sys.argv[2].split(',')]; N=int(sys.argv[3]); BIN=sys.argv[4]
MODES=[ROUND_HALF_EVEN,ROUND_HALF_UP,ROUND_HALF_DOWN,ROUND_DOWN,ROUND_UP,ROUND_CEILING,ROUND_FLOOR,ROUND_05UP]
def rnum(lo,hi,digs):
    c=random.randint(1,10**digs); e=random.randint(lo,hi); return f"{c}E{e:+d}"
cases=[]
for p in PRECS:
    for i in range(N):
        op=random.choice(['exp','ln','log10','power'])
        m=ROUND_HALF_EVEN if op!='power' else random.choice(MODES)
        d=random.randint(1,min(p+5,40))
        if op=='exp': x=rnum(-d-3,-d+3,d); x=('-' if random.random()<.5 else '')+x; ops=[x]
        elif op in('ln','log10'): ops=[rnum(-d-5,-d+5,d)]
        else:
            ops=[rnum(-d-1,-d+2,d), ('-' if random.random()<.5 else '')+rnum(-4,0,random.randint(1,6))]
        cases.append((op,p,m,ops))
lines=[f"{op} {p} 999999 -999999 {m} 0 "+" ".join(ops) for op,p,m,ops in cases]
t=time.time()
out=subprocess.run([BIN],input="\n".join(lines)+"\n",capture_output=True,text=True).stdout.splitlines()
el=time.time()-t
bad=collections.Counter(); cnt=collections.Counter(); hard=0
for (op,p,m,ops),o in zip(cases,out):
    got,gb=o.rsplit(' ',1)
    mpmath.mp.dps=p+60
    a=mpmath.mpf(ops[0])
    if op=='exp': v=mpmath.exp(a)
    elif op=='ln': v=mpmath.log(a)
    elif op=='log10': v=mpmath.log10(a)
    else:
        b=mpmath.mpf(ops[1]); v=mpmath.power(a,b) if a>0 else None
    if v is None: continue
    s=mpmath.nstr(v,p+50,min_fixed=1,max_fixed=0)
    dig=s.replace('-','').replace('.','').split('e')[0].lstrip('0')
    tail=dig[p:p+45]
    if tail.startswith('0'*40) or tail.startswith('9'*40) or tail.startswith('5'+'0'*39) or tail.startswith('4'+'9'*39): hard+=1; continue
    want=Context(prec=p,rounding=m,Emax=999999,Emin=-999999,traps=[]).create_decimal(s)
    cnt[(op,p)]+=1
    if Decimal(got)!=want:
        bad[(op,p)]+=1
        if bad[(op,p)]<=3: print("BAD",op,p,m,ops,"got",got,"want",want)
print("elapsed",round(el,2),"s for",len(cases),"hard-skipped",hard)
print("COUNTS",dict(cnt)); print("BAD",dict(bad))
