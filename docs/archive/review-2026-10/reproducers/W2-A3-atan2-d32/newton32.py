import subprocess, random, mpmath
from decimal import Decimal, getcontext, ROUND_HALF_EVEN, Context
mpmath.mp.dps=150
probe="/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-A3/release/a3probe"
random.seed(7)
def run(cmds):
    return subprocess.run([probe],input="\n".join(cmds)+"\n",capture_output=True,text=True).stdout.split("\n")
def val(o):
    c,e=o.split("E"); return mpmath.mpf(int(c))*mpmath.mpf(10)**int(e)
# 1) erecip of random 50-digit operands in [1,10)
N=20000
ops=[str(random.randrange(10**49,10**50)) for _ in range(N)]
for fmt in ("d32","d64","d128"):
    out=run(["erecip %s %s -49"%(fmt,c) for c in ops])
    worst=0
    for c,o in zip(ops,out):
        b=mpmath.mpf(int(c))/mpmath.mpf(10)**49
        r=val(o); e=abs(r*b-1); worst=max(worst,e)
    print("erecip",fmt,"max rel err %.3e"%worst)
