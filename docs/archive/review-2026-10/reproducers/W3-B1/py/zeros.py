import itertools, random
from harness import *
rng=random.Random(3)
M=list(MODES); E=[-6176,-6175,-6143,-100,-1,0,1,100,6110,6111]
cases=[]
for m in M:
  for e1 in E:
    for e2 in E:
      for s1,s2 in itertools.product((0,1),repeat=2):
        cases.append(("add",m,F(s1,0,e1),F(s2,0,e2)))
        cases.append(("sub",m,F(s1,0,e1),F(s2,0,e2)))
        cases.append(("mul",m,F(s1,0,e1),F(s2,0,e2)))
        cases.append(("mul",m,F(s1,0,e1),F(s2,12345,e2)))
        cases.append(("div",m,F(s1,0,e1),F(s2,7,e2)))
        cases.append(("div",m,F(s1,7,e1),F(s2,0,e2)))
        cases.append(("div",m,F(s1,0,e1),F(s2,0,e2)))
        cases.append(("quantize",m,F(s1,1234,e1),F(s2,0,e2)))
        cases.append(("quantize",m,F(s1,0,e1),F(s2,1,e2)))
        cases.append(("remnear",m,F(s1,0,e1),F(s2,3,e2)))
        cases.append(("remtrunc",m,F(s1,6,e1),F(s2,3,e2)))
        cases.append(("remnear",m,F(s1,6,e1),F(s2,3,e2)))
        cases.append(("divint",m,F(s1,0,e1),F(s2,3,e2)))
        cases.append(("divint",m,F(s1,1,e1),F(s2,3,e2)))
        c=rng.choice([1,7,10**33,10**34-1,5]); 
        cases.append(("add",m,F(s1,c,e1),F(1-s1,c,e1)))
        cases.append(("add",m,F(s1,c*10 if c*10<10**34 else c,e1-1 if e1>-6176 else e1),F(1-s1,c,e1)))
        cases.append(("fma",m,F(s1,c,0),F(s2,1,e1),F(1-(s1^s2),c,e1)))
      cases.append(("sqrt",m,F(1,0,e1))); cases.append(("sqrt",m,F(0,0,e1)))
      cases.append(("reduce",m,F(rng.randrange(2),0,e1)))
      for s in (0,1):
        for c in (1,4,5,6,9,49,50,51,99,1000):
          cases.append(("rti",m,F(s,c,e1 if e1<0 else -len(str(c)))))
          cases.append(("rtie",m,F(s,c,-len(str(c)))))
          cases.append(("rtie",m,F(s,c,-len(str(c))-1)))
def r2(cs):
    op=cs[0]
    if op in("remnear","remtrunc"): return ref(op+"_mpd",cs[1],cs[2:])
    return ref(op,cs[1],cs[2:])
n,bad=compare(cases,r2); print("zeros",n,len(bad)); [print(" ",b) for b in bad[:20]]
