import sys, random, itertools
from harness import *
rng=random.Random(7)
M=list(MODES)
P34=10**34
bigs=[10**33, P34-1, 10**33+1, P34-2, 5*10**33, 10**32, 10**32-1+0*1, 10**34-10, 9999999999999999999999999999999995]
smalls=[1,5,9,4,6,10**33,P34-1,5*10**33,10**17]
def clampe(e): return max(-6176,min(6111,e))
cases=[]
# add/sub: big +- far small
for c in bigs:
  for e in [-6176,-6175,-6143,-6142,-6110,-100,0,100,6077,6078,6100,6110,6111]:
    for gap in list(range(0,40))+[60,68,69,70,100,200,300]:
      for sc in rng.sample(smalls,3):
        e2=clampe(e-gap- len(str(sc))+1)
        for s1,s2 in [(0,1),(0,0),(1,0),(1,1)]:
          m=rng.choice(M)
          cases.append(("add",m,F(s1,c,e),F(s2,sc,e2)))
          cases.append(("sub",rng.choice(M),F(s2,sc,e2),F(s1,c,e)))
pass
# mul: products of 34/35 digits at boundary
facs=[]
for a,b in [(10**17,10**17),(10**17+1,10**17-1),(2**40,5**40),(2**56,5**47),(2**100,5**40),(3,3333333333333333333333333333333333),(9,1111111111111111111111111111111111),(11,909090909090909090909090909090909),(99999999999999999,100000000000000001),(10**33,10),(P34-1,9),(P34-1,P34-1),(5*10**33,2),(5,2*10**33),(25,4*10**32)]:
  if a<P34 and b<P34: facs.append((a,b))
cases=[]
for a,b in facs:
  for ea in [-6176,-3100,-34,0,34,3000,6111]:
    for eb in [-6176,-3100,-34,-1,0,1,34,3000,6111]:
      for s in itertools.product((0,1),repeat=2):
        for m in M: cases.append(("mul",m,F(s[0],a,ea),F(s[1],b,eb)))
n,bad=compare(cases); print("mul boundary",n,len(bad)); [print(" ",b) for b in bad[:8]]; open("famfma.txt","w").write("\n".join(map(str,bad)))
# div: 1/2^k, 1/5^k, (10^34-1)/2^k , exact quotients
cases=[]
for k in range(1,112):
  for num in [1,3,P34-1,10**33,7]:
    for den in [2**k, 5**k if 5**k<P34 else None]:
      if den is None: continue
      for ea,eb in [(0,0),(-6176,0),(0,6111),(-6000,100),(6111,-6176),(-100,-6176),(6000,-100)]:
        for m in M: cases.append(("div",m,F(rng.randrange(2),num,ea),F(rng.randrange(2),den,eb)))
n,bad=compare(cases); print("div boundary",n,len(bad)); [print(" ",b) for b in bad[:8]]; open("famfma.txt","w").write("\n".join(map(str,bad)))
# fma with zero product / zero addend / exact cancel
cases=[]
for m in M:
  for ea in [-6176,0,6111]:
    for ec in [-6176,-5,0,6111]:
      for s in itertools.product((0,1),repeat=3):
        cases.append(("fma",m,F(s[0],0,ea),F(s[1],7,ea),F(s[2],0,ec)))
        cases.append(("fma",m,F(s[0],3,ea),F(s[1],7,0),F(s[2],21,ea)))
        cases.append(("fma",m,F(s[0],0,ea),F(s[1],0,ec),F(s[2],5,ec)))
        cases.append(("fma",m,F(s[0],10**17,ea),F(s[1],10**17,0),F(s[2],1,ec)))
        cases.append(("fma",m,F(s[0],P34-1,ea),F(s[1],P34-1,-ea if -ea<=6111 else 6111),F(s[2],1,ec)))
n,bad=compare(cases); print("fma zeros",n,len(bad)); [print(" ",b) for b in bad[:8]]; open("famfma.txt","w").write("\n".join(map(str,bad)))
