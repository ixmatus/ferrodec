import random
from harness import *
from pools import *
rng=random.Random(9)
mm={"floor":"tn","ceil":"tp","trunc":"tz","round":"na","roundeven":"ne"}
cases=[]
for i in range(40000):
    op=rng.choice(list(mm)); v=values(rng,1)[0] if rng.random()>0.05 else rng.choice(SPECIALS)
    cases.append((op,"ne",v))
def r(cs):
    w=ref("rti",mm[cs[0]],cs[2:])
    return w.rsplit(" ",1)[0]+" 0"
n,bad=compare(cases,r); print(n,len(bad)); [print(b) for b in bad[:10]]
