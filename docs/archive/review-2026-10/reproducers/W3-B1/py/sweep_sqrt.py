import sys, random
from harness import *
from pools import *
rng=random.Random(int(sys.argv[1]))
cases=[]
for i in range(int(sys.argv[2])):
    v=values(rng,1)[0]; p=v.split(","); v=F(0,p[2],p[3])
    cases.append(("sqrt",rng.choice(list(MODES)),v))
# perfect squares at the boundary
for r in [10**17-1,10**17,10**16+1,3*10**16,31622776601683793]:
    for e in [-6176,-6175,-34,-1,0,1,6110,6111]:
        c=r*r
        if c<10**34:
            for m in MODES: cases.append(("sqrt",m,F(0,c,e)))
n,bad=compare_orc(cases)
print("sqrt cases",n,"mismatch",len(bad))
for b in bad[:8]: print(" ",b)
