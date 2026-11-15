import subprocess, mpmath, decimal
from decimal import Decimal, Context
mpmath.mp.dps=80
probe="/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-A3/release/a3probe"
MODES=[("NE",decimal.ROUND_HALF_EVEN),("NA",decimal.ROUND_HALF_UP),("TP",decimal.ROUND_CEILING),("TN",decimal.ROUND_FLOOR),("TZ",decimal.ROUND_DOWN)]
rows=[l.split() for l in open("search_hits_all.txt")]
cmds=[];meta=[]
for cx,cy,off,d,eps in rows:
    y="%sE86"%cy; x="%sE90"%cx
    for m,_ in MODES: cmds.append("a2 d32 %s %s %s"%(y,x,m)); meta.append((y,x,m))
out=subprocess.run([probe],input="\n".join(cmds)+"\n",capture_output=True,text=True).stdout.split("\n")
bad=0
for (y,x,m),o in zip(meta,out):
    R=mpmath.atan(mpmath.mpf(y)/mpmath.mpf(x))
    rs=mpmath.nstr(R,70,strip_zeros=False)
    rm=dict(MODES)[m]
    want=Context(prec=7,rounding=rm,Emin=-95,Emax=96).plus(Decimal(rs))
    got=Decimal(o.split()[0])
    flag="OK" if got==want else "MISROUND"
    if flag!="OK": bad+=1
    print("atan2(%s, %s) %s: got %s want %s true %s %s"%(y,x,m,o.split()[0],want,mpmath.nstr(R,25),flag))
print("misrounds:",bad)
