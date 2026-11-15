import random
from harness import F
P=34
def coefs():
    s=set([0,1,2,3,5,7,9])
    for k in range(0,35):
        p=10**k
        for c in (p, p-1, p+1, 5*p, 5*p-1, 5*p+1, 2*p, 25*p if k<33 else p):
            if 0<=c<10**34: s.add(c)
    s.add(10**34-1); s.add(10**34-2); s.add(10**33); s.add(10**33+1)
    return sorted(s)
EXPS=[-6176,-6175,-6174,-6143-33,-6143,-6142,-6110,-200,-100,-68,-35,-34,-33,-17,-2,-1,0,1,2,17,33,34,100,6000,6077,6078,6079,6110,6111]
def values(rng, n):
    cs=coefs()
    out=[]
    for _ in range(n):
        c=rng.choice(cs) if rng.random()<0.8 else rng.randrange(10**rng.randrange(1,35))
        e=rng.choice(EXPS) if rng.random()<0.7 else rng.randrange(-6176,6112)
        out.append(F(rng.randrange(2),c,e))
    return out
SPECIALS=["I,0","I,1","Q,0,0","Q,1,0","Q,0,123","Q,1,77","S,0,0","S,1,5","S,0,999999999999999999999999999999999"]
