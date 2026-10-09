import subprocess, mpmath
from fractions import Fraction
mpmath.mp.dps=120
LN10_55=Fraction(2302585092994045684017991454684364207601101488628772976, 10**54)
ln10=mpmath.log(10)
def round50(f):
    # round positive Fraction to 50 sig digits half-even
    from decimal import Decimal, getcontext, ROUND_HALF_EVEN
    getcontext().prec=200
    d=Decimal(f.numerator)/Decimal(f.denominator)
    getcontext().prec=50
    return Fraction(+d)
best=[]
for k in range(1,6200):
    p=round50(k*LN10_55)
    err=float(mpmath.mpf(p.numerator)/p.denominator - k*ln10)
    best.append((abs(err),k,err))
best.sort(reverse=True)
print("top k by |round50(k ln10)-k ln10|:", best[:3])
probe="/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-A3/release/a3probe"
cmds=[]; xs=[]
for _,k,_ in best[:20]:
    for off in ("0","0.37","-0.41","1.05"):
        x=mpmath.mpf(k)*ln10+mpmath.mpf(off)
        xs_=mpmath.nstr(x,34,strip_zeros=False,min_fixed=-1,max_fixed=10)
        xs.append(xs_); cmds.append("exp "+xs_)
out=subprocess.run([probe],input="\n".join(cmds)+"\n",capture_output=True,text=True).stdout.split("\n")
worst=0
for xs_,o in zip(xs,out):
    c,e=o.split("E"); c=int(c); e=int(e)
    d=len(str(c)); cw=c*10**(50-d); ew=e-(50-d)
    tv=mpmath.exp(mpmath.mpf(xs_))
    units=abs(mpmath.mpf(cw)*mpmath.mpf(10)**ew - tv)/mpmath.mpf(10)**ew
    worst=max(worst,float(units))
print("max rung-1 exp error over probes: %.0f predicate units (catalog EXP sum 22,400; budget 250,000)"%worst)
