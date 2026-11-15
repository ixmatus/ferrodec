# Compare the frozen S2 corpus's recorded (sweep / mirror) margins against
# the TRUE margin of f(x) computed in Arb at high precision.
import sys, re
from fractions import Fraction
from flint import arb, ctx, fmpq
ROOT="/Users/parnell/Development/ferrodec/tests/vectors/transcend/campaign/"
FN={"sin":arb.sin,"cos":arb.cos,"tan":arb.tan,"exp":arb.exp,"ln":arb.log,
    "log10":lambda x: x.log()/arb(10).log(),"sinh":arb.sinh,"cosh":arb.cosh}
def q(tok):
    neg=tok.startswith("-"); t=tok.lstrip("-")
    m,_,e=t.partition("E"); e=int(e)
    if "." in m:
        a,_,b=m.partition("."); e-=len(b); m=a+b
    c=int(m)
    v=fmpq(c)*fmpq(10)**e if e>=0 else fmpq(c,10**(-e))
    return -v if neg else v
def margins(v, p):
    # v: Fraction (exact midpoint of a tiny ball); return (grid,tie) frac ULP
    a=abs(v)
    E=len(str(a.numerator))-len(str(a.denominator))-(p-1)
    for _ in range(6):
        s=a/(Fraction(10)**E) if E>=0 else a*Fraction(10)**(-E)
        k=s.numerator//s.denominator
        if k>=10**p: E+=1; continue
        if k<10**(p-1): E-=1; continue
        fr=s-k
        return float(min(fr,1-fr)), float(abs(fr-Fraction(1,2)))
def tofrac(x):
    m,e=x.mid().man_exp(); m=int(m); e=int(e)
    return Fraction(m)*(Fraction(2)**e if e>=0 else Fraction(1,2**(-e)))
fn=sys.argv[1]
rows={}
for line in open(ROOT+fn+".prov"):
    if line.startswith("#") or not line.strip(): continue
    parts=line.split()
    if parts[1]!="NearestEven": continue
    p=int(parts[0]); x=parts[2]
    m=float(re.search(r"margin=(\S+)",line).group(1))
    rows[(p,x)]=m
worst=0
for (p,x),lab in rows.items():
    xv=q(x)
    exp10=int(x.split("E")[1])
    bits=int(abs(exp10)*3.33)+400
    ctx.prec=bits
    y=FN[fn](arb(xv.p)/arb(xv.q))
    g,t=margins(tofrac(y),p)
    tm=min(g,t)
    ratio = tm/lab if lab>0 else float('inf')
    print("%d %s label=%.3e true_grid=%.3e true_tie=%.3e true_min=%.3e rad_rel=%.1e"%(p,x,lab,g,t,tm,float(y.rad()/abs(y.mid()))))
