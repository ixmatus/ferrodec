import itertools, random
from harness import *
rng=random.Random(11)
M=list(MODES)
cases=[]
cs=[1,7,99,10**10,10**33,10**34-1,123456789,5*10**32]
for m in M:
  for c1 in cs:
    for c2 in [1,3,10,10**5,10**20]:
      for e in range(6100,6112):
        for d in range(0,50,3):
          s=rng.randrange(2)
          cases.append(("mul",m,F(s,c1,e),F(0,c2,d)))
          cases.append(("div",m,F(s,c1,e),F(0,c2,-d)))
          cases.append(("fma",m,F(s,c1,e),F(0,c2,d),F(1,1,6000)))
          cases.append(("scaleb",m,F(s,c1,e),str(d)))
          cases.append(("add",m,F(s,c1,6111),F(0,c2,e)))
  for e in range(6070,6112):
    for c in [1,10**33,10**34-1,10**20,10**33+10**10]:
      cases.append(("reduce",m,F(0,c,e)))
      cases.append(("quantize",m,F(0,c,e-30),F(0,1,6111)))
      cases.append(("quantize",m,F(0,c,6111-(e-6070)),F(0,1,e)))
    for c in [1,9,10**33,10**34-1]:
      cases.append(("nextup",m,F(0,c,e))); cases.append(("nextdown",m,F(1,c,e)))
for m in M:
  for e in range(-6178,-6100):
    for c in [1,5,10**33,10**34-1,15,25]:
      cases.append(("mul",m,F(0,c,max(e,-6176)),F(0,1,-1)))
      cases.append(("div",m,F(1,c,max(e,-6176)),F(0,2,0)))
      cases.append(("div",m,F(1,c,max(e,-6176)),F(0,3,2)))
      cases.append(("fma",m,F(0,c,max(e,-6176)),F(0,1,-1),F(1,1,-6176)))
      cases.append(("scaleb",m,F(0,c,max(e,-6176)),"-3"))
      cases.append(("quantize",m,F(0,c,-6143),F(0,1,-6176)))
n,bad=compare(cases); print("clamp/edge",n,len(bad)); [print(" ",b) for b in bad[:20]]
open("clamp_bad.txt","w").write("\n".join(map(str,bad)))
