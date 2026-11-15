import sys, random
from harness import *
from pools import *
rng=random.Random(int(sys.argv[1])); N=int(sys.argv[2])
def run(name, cases, refn=None):
    n,bad=compare(cases, refn)
    print(name,"cases",n,"mismatch",len(bad))
    for b in bad[:6]: print("  ",b)
# rem (GDA) via libmpdec
def remref(cs):
    op={"remnear":"remnear_mpd","remtrunc":"remtrunc_mpd"}[cs[0]]
    return ref(op, cs[1], cs[2:])
for op in ("remnear","remtrunc"):
    cases=[]
    for i in range(N):
        vs=values(rng,2)
        vs=[ (rng.choice(SPECIALS) if rng.random()<0.05 else v) for v in vs]
        if rng.random()<0.6 and vs[0][0]=="F" and vs[1][0]=="F":
            q=vs[0].split(","); p=vs[1].split(",")
            e=int(q[3])-rng.randrange(-40,40); e=max(-6176,min(6111,e)); vs[1]=F(p[1],p[2],e)
        cases.append((op,"ne",*vs))
    # tie families for rem_near: x = (2k+1)/2 * y
    for k in range(200):
        y=rng.choice([2,4,6,10,20,2*10**33-2 if False else 8])
        n_=rng.randrange(0,10**rng.randrange(1,33))
        x=(2*n_+1)*y//2
        if x<10**34:
            e=rng.choice([-6176,0,100,6111-1]); 
            for s in (0,1): cases.append((op,"ne",F(s,x,e),F(rng.randrange(2),y,e)))
    run(op,cases,remref)
# scaleb
cases=[]
for i in range(N):
    v=values(rng,1)[0] if rng.random()>0.05 else rng.choice(SPECIALS)
    n=rng.choice([0,1,-1,34,-34,6111,-6176,12287,-12287,12288,-12288,12356,-12356,2*(6144+34),-2*(6144+34)]) if rng.random()<0.5 else rng.randrange(-12356,12357)
    cases.append(("scaleb",rng.choice(list(MODES)),v,str(n)))
run("scaleb",cases)
# rotate / shift
cases=[]
for i in range(N):
    v=values(rng,1)[0] if rng.random()>0.05 else rng.choice(SPECIALS)
    k=rng.randrange(-36,37)
    sh=F(1 if k<0 else 0,abs(k),0) if rng.random()>0.05 else rng.choice(SPECIALS+[F(0,3,1),F(0,35,0),F(0,1,-1)])
    cases.append((rng.choice(["rotate","shift"]),"ne",v,sh))
run("rotate/shift",cases)
# logical
def lg():
    r=rng.random()
    if r<0.05: return rng.choice(SPECIALS+[F(0,2,0),F(1,1,0),F(0,1,1),F(0,1,-1)])
    n=rng.randrange(0,35)
    return F(0,int("".join(rng.choice("01") for _ in range(n)) or "0"),0)
cases=[]
for i in range(N):
    op=rng.choice(["and","or","xor","invert"])
    cases.append((op,"ne",lg(),lg()) if op!="invert" else (op,"ne",lg()))
run("logical",cases)
