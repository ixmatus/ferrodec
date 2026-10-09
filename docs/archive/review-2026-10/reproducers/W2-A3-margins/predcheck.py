import subprocess, random, decimal
from decimal import Decimal, Context
probe="/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-A3/release/a3probe"
FMT={"d128":(34,6144,-6143),"d64":(16,384,-383),"d32":(7,96,-95)}
MODES=[decimal.ROUND_HALF_EVEN,decimal.ROUND_HALF_UP,decimal.ROUND_CEILING,decimal.ROUND_FLOOR,decimal.ROUND_DOWN]
def rnd(fmt,v):
    P,Emax,Emin=FMT[fmt]; res=[]
    for m in MODES:
        c=Context(prec=P,Emax=Emax,Emin=Emin,rounding=m,traps=[])
        r=c.plus(v)
        f=c.flags
        res.append((r if r.is_finite() else str(r), r.is_zero() and 0 or 0, bool(f[decimal.Inexact]),bool(f[decimal.Underflow]),bool(f[decimal.Overflow])))
        # normalize value comparison by numeric value
    return [(x[0] if isinstance(x[0],str) else x[0].normalize(Context(prec=60)),)+x[2:] for x in res]
def model(fmt,coef,exp,B):
    if coef==0: return True
    d=len(str(coef))
    if d>50:
        # emulate predicate's normalization: round half even to 50 digits
        c=Context(prec=50,rounding=decimal.ROUND_HALF_EVEN,Emax=999999,Emin=-999999)
        v=c.plus(Decimal(coef).scaleb(exp)); t=v.as_tuple(); coef=int("".join(map(str,t.digits))); exp=t.exponent; d=len(str(coef))
    cw=coef*10**(50-d); ew=exp-(50-d)
    big=Context(prec=200,Emax=999999,Emin=-999999)
    lo=big.subtract(Decimal(cw).scaleb(ew,big), Decimal(10*B+1).scaleb(ew-1,big))
    hi=big.add(Decimal(cw).scaleb(ew,big), Decimal(10*B+1).scaleb(ew-1,big))
    return rnd(fmt,lo)!=rnd(fmt,hi)
cases=[]
random.seed(11)
def add(fmt,coef,exp,B): cases.append((fmt,coef,exp,B))
for fmt,(P,Emax,Emin) in FMT.items():
    ex=50-P
    for _ in range(400):
        B=random.choice([1,10,1000,250000,10**14,3*10**14])
        if 4*B>=10**ex: B=10**(ex-2)
        kept=random.randrange(10**(P-1),10**P)
        e=random.randrange(Emin-P-20,Emax-P)  # exponent of kept grid (normal or not)
        for tail in [B,B+1,10**ex-B,10**ex-B-1,5*10**(ex-1)-B,5*10**(ex-1)-B-1,5*10**(ex-1)+B,5*10**(ex-1)+B+1,random.randrange(10**ex)]:
            add(fmt,kept*10**ex+tail, e-ex, B)
    qmin=Emin-P+1
    # subnormal: exponents placing the 50-digit value below 10^Emin
    for _ in range(400):
        B=random.choice([1,1000,250000])
        c=random.randrange(10**49,10**50)
        e=random.randrange(qmin-60,Emin-49+2)
        add(fmt,c,e,B)
    # near 10^Emin both sides
    for k in range(1,40):
        for B in (1,1000,10**6):
            add(fmt,10**50-k,Emin-50,B); add(fmt,10**49+k,Emin-49,B)
            add(fmt,10**50-B,Emin-50,B); add(fmt,10**50-B-1,Emin-50,B)
    # overflow edge: near Nmax and 10^(Emax+1)
    for B in (1,1000,10**6,10**14):
        if 4*B>=10**ex: continue
        nmax=(10**P-1)*10**ex
        for t in (0,1,B,B+1,10**ex-1,10**ex-B,10**ex-B-1,5*10**(ex-1),5*10**(ex-1)-B,5*10**(ex-1)-B-1):
            add(fmt,nmax+t,Emax-P+1-ex,B)
        add(fmt,10**49+B,Emax+1-49,B); add(fmt,10**49+B+1,Emax+1-49,B); add(fmt,10**50-B,Emax-49,B)
    # all nines, short, overwide
    for B in (1,1000):
        add(fmt,10**50-1,0,B); add(fmt,7,0,B); add(fmt,7,Emin-3,B)
        add(fmt,int("1"+"0"*(P-1)+"5"+"0"*(54-P)),0,B)  # 55-digit exact midpoint
        add(fmt,int("1"+"0"*(P-1)+"4"+"9"*(54-P)),0,B)
    add(fmt,0,0,1)
out=subprocess.run([probe],input="\n".join("pred %s %d %d %d"%c for c in cases)+"\n",capture_output=True,text=True)
res=out.stdout.split("\n")
bad=0
for c,r in zip(cases,res):
    m=model(*c); rr=(r.strip()=="true")
    if m!=rr:
        bad+=1
        if bad<=15: print("MISMATCH",c,"rust",rr,"model",m)
print("cases",len(cases),"mismatches",bad, "rust stderr:",out.stderr[-300:])
