import sys, random
from harness import *
from pools import *
rng=random.Random(int(sys.argv[2]) if len(sys.argv)>2 else 1)
op=sys.argv[1]; N=int(sys.argv[3]) if len(sys.argv)>3 else 40000
ar={"add":2,"sub":2,"mul":2,"div":2,"fma":3,"sqrt":1,"divint":2,"quantize":2,"rti":1,"rtie":1,"reduce":1,"min":2,"max":2,"minmag":2,"maxmag":2,"logb":1,"nextup":1,"nextdown":1,"nexttoward":2,"totcmp":2,"totmag":2,"samequantum":2,"cmp":2,"cmpsig":2,"abs":1}[op]
cases=[]
for i in range(N):
    m=rng.choice(list(MODES))
    vs=values(rng,ar)
    vs=[ (rng.choice(SPECIALS) if rng.random()<0.05 else v) for v in vs]
    # make some operands close: same exponent region
    if ar>=2 and rng.random()<0.4:
        p=vs[1].split(",")
        if p[0]=="F":
            q=vs[0].split(",")
            if q[0]=="F":
                e=int(q[3])-rng.randrange(-3,40)
                e=max(-6176,min(6111,e)); vs[1]=F(p[1],p[2],e)
    cases.append((op,m,*vs))
n,bad=compare(cases)
print(op,"cases",n,"mismatch",len(bad))
for b in bad[:int(sys.argv[4]) if len(sys.argv)>4 else 8]: print(" ",b)
