import itertools, random
from harness import *
rng=random.Random(5); M=list(MODES); P=10**34
cases=[]
for m in M:
  for s in (0,1):
    for big in [P-1,P-2,10**33,5*10**33]:
      for sm,es in [(5,6110),(4,6110),(6,6110),(1,6110),(1,6000),(1,-6176),(5,6077),(49,6109),(50,6109),(51,6109),(P-1,6077),(P-1,6076)]:
        for s2 in (0,1):
          cases.append(("add",m,F(s,big,6111),F(s2,sm,es)))
          cases.append(("fma",m,F(s,big,6111),F(0,1,0),F(s2,sm,es)))
          cases.append(("fma",m,F(s,big,6100),F(0,1,11),F(s2,sm,es)))
    # products exactly 10^6145 and near with addends of each gap
    for a,b in [(1,1),(10**17,10**17),(10,10**33),(2,5),(25,4),(P-1,1)]:
      for ea in [6100,6111,6000]:
        eb=6145-ea-(len(str(a*b))-1)
        for gap in [1,2,33,34,35,60,75,76,77,100,109,110,111,150,200,6000,12000]:
          ec=6145-gap
          if ec<-6176: continue
          for s2 in (0,1):
            for cc in (1,5,P-1):
              cases.append(("fma",m,F(s,a,ea),F(0,b,eb),F(s2,cc,max(-6176,ec-len(str(cc))+1))))
def ok(o):
    p=o.split(",")
    return p[0]!="F" or (-6176<=int(p[3])<=6111)
cases=[c for c in cases if all(ok(o) for o in c[2:])]
n,bad=compare(cases); print(n,len(bad))
import collections
cnt=collections.Counter()
for b in bad:
    f=b[0].split(); cnt[(f[0],f[1],b[1].split()[1],b[2].split()[1])]+=1
for k,v in sorted(cnt.items()): print(v,k)
open("emax_bad.txt","w").write("\n".join(map(str,bad)))
