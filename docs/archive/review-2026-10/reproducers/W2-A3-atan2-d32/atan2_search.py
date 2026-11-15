# Search d32 atan2(y, x) inputs where the subnormal Newton seed for 1/x
# (x in (1e96, 1e97)) leaves q = y/x low by e2 ~ 1e-18 and the true
# atan(y/x) sits just above a rounding boundary, so rung 1 delivers the
# wrong side. Truth: mpmath at 80 digits; candidate generation: modular.
import subprocess, mpmath
from fractions import Fraction
mpmath.mp.dps=80
probe="/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-A3/release/a3probe"
def run(cmds):
    return subprocess.run([probe],input="\n".join(cmds)+"\n",capture_output=True,text=True).stdout.split("\n")
xs=[c for c in range(9999001,9999999,2) if c%5!=0][:120]
# e2 per x from the probe's rung-1 recip
outs=run(["recip d32 %dE90"%c for c in xs])
cands=[]
for cx,o in zip(xs,outs):
    c,e=o.split("E"); r=Fraction(int(c))*Fraction(10)**int(e)
    e2=1-Fraction(cx)*10**90*r
    if e2<=0: continue
    e2f=float(e2)
    for D in (-5,-6):              # q decade
        ey=97+D-7                   # y = c_y*10^ey, q ~ c_y*10^(ey-90)/cx
        M=ey-84-D
        inv=pow(10**M, -1, cx)
        ulp=Fraction(10)**(D-6)
        lo_cy,hi_cy=10**6,10**7
        # C range across c_y with q in decade D
        qmin=Fraction(lo_cy)*Fraction(10)**(ey-90)/cx
        qmax=Fraction(hi_cy)*Fraction(10)**(ey-90)/cx
        Cmax=float(qmax**3/3/ulp)
        eps_max=float(qmax*e2/ulp)
        for fam in (0,1):
            off=Fraction(1,2) if fam else 0
            nlo=int((0+float(off))*cx)-2
            nhi=int((Cmax+eps_max+float(off))*cx)+2
            for n in range(nlo,nhi+1):
                base=(n%cx)*inv%cx
                for cy in (base, base+cx):
                    if not (lo_cy<=cy<hi_cy): continue
                    q=Fraction(cy)*Fraction(10)**(ey-90)/cx
                    if not (Fraction(10)**D<=q<Fraction(10)**(D+1)): continue
                    R=mpmath.atan(mpmath.mpf(q.numerator)/q.denominator)
                    f=R/mpmath.mpf(ulp.numerator)*ulp.denominator
                    fr=f-mpmath.floor(f)-mpmath.mpf(off)
                    if fr<0: fr+=1
                    eps=mpmath.mpf(q.numerator)/q.denominator*e2f/(mpmath.mpf(ulp.numerator)/ulp.denominator)
                    if 0<fr<=eps:
                        cands.append((cy,ey,cx,fam,float(fr),float(eps)))
print("candidates:",len(cands))
for c in cands[:20]: print(c)
import json; json.dump(cands,open("atan2_cands.json","w"))
