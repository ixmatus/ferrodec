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
