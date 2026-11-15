import random, subprocess, sys, collections
from decimal import *
random.seed(int(sys.argv[1]) if len(sys.argv)>1 else 1)
N=int(sys.argv[2]) if len(sys.argv)>2 else 50000
BIN=sys.argv[3]
MODES=[ROUND_HALF_EVEN,ROUND_HALF_UP,ROUND_HALF_DOWN,ROUND_DOWN,ROUND_UP,ROUND_CEILING,ROUND_FLOOR,ROUND_05UP]
def coeff(p):
    k=random.randint(0,12)
    n=random.randint(1,p+3)
    if k==0: return '0'
    if k==1: return '1'+'0'*random.randint(0,n)
    if k==2: return '9'*n
    if k==3: return '5'+'0'*random.randint(0,n)
    if k==4: return '1'+'0'*max(0,n-2)+'1'
    if k==5: return '9'*max(1,n-1)+'5'
    if k==6: return str(random.randint(1,9))
    if k==7: return '4'+'9'*max(0,n-1)
    if k==8: return '25'
    return str(random.randint(1,10**n))
def operand(p,emax,emin):
    r=random.random()
    if r<0.03: return random.choice(['Inf','-Inf','NaN','-NaN','NaN12','sNaN','sNaN7','-sNaN34'])
    c=coeff(p)
    et=emin-p+1
    k=random.random()
    if k<0.2: e=random.randint(et-p-3,et+3)
    elif k<0.4: e=random.randint(emax-2*p-2,emax+2)
    elif k<0.5: e=random.choice([0,-1,1])
    else: e=random.randint(et-3,emax+2)
    s='-' if random.random()<0.4 else ''
    return f"{s}{c}E{e:+d}"
BIN_OPS=['add','subtract','multiply','divide','divide_int','remainder','remainder_near','quantize','max','min','compare','next_toward','scaleb']
UN_OPS=['plus','minus','sqrt','next_plus','next_minus','to_integral_value','to_integral_exact','normalize','logb']
def flags(c):
    b=0
    f=c.flags
    if f[InvalidOperation]: b|=1
    if f[DivisionByZero]: b|=2
    if f[Overflow]: b|=4|16
    if f[Underflow]: b|=8|16
    if f[Inexact]: b|=16
    if f[Clamped]: b|=32
    return b
cases=[];lines=[]
for i in range(N):
    p=random.choice([1,2,3,4,5,7,9,16,34])
    emax=random.choice([p+2,9,99,384,6144])
    emin=random.choice([-emax+1,-emax, -emax//2-1])
    m=random.choice(MODES); cl=random.choice([0,1])
    r=random.random()
    if r<0.12: op='fma'; ops=[operand(p,emax,emin) for _ in range(3)]
    elif r<0.45: op=random.choice(UN_OPS); ops=[operand(p,emax,emin)]
    else:
        op=random.choice(BIN_OPS); ops=[operand(p,emax,emin) for _ in range(2)]
        if op=='scaleb': ops[1]=str(random.randint(-2*(emax+p)-2,2*(emax+p)+2))
    c=Context(prec=p,Emax=emax,Emin=emin,rounding=m,clamp=cl,traps=[])
    ds=[Decimal(o) for o in ops]
    try:
        res=getattr(c,op)(*ds)
    except Exception as e:
        continue
    cases.append((op,p,emax,emin,m,cl,ops,str(res),flags(c)))
    lines.append(f"{op} {p} {emax} {emin} {m} {cl} "+" ".join(ops))
out=subprocess.run([BIN],input="\n".join(lines)+"\n",capture_output=True,text=True).stdout.splitlines()
cnt=collections.Counter(); bad=collections.Counter(); ex=collections.defaultdict(list)
for cs,o in zip(cases,out):
    op=cs[0]; cnt[op]+=1
    got,gb=o.rsplit(' ',1)
    if got!=cs[7] or int(gb)!=cs[8]:
        # strip NaN payload sign nuance? keep strict
        kind='value' if got!=cs[7] else 'flags'
        if got.lstrip('-').startswith('NaN') and cs[7].lstrip('-').startswith('NaN'): kind='nanpayload'
        elif cs[5]==1: kind+='-clamp1'
        bad[(op,kind)]+=1
        if len(ex[(op,kind)])<6: ex[(op,kind)].append((cs,got,gb))
print("TOTAL",sum(cnt.values()),dict(cnt))
print("MISMATCH",dict(bad))
for k,v in ex.items():
    print("==",k)
    for cs,got,gb in v: print("  ",cs[0],"p=%d emax=%d emin=%d %s clamp=%d"%cs[1:6],cs[6],"want",cs[7],cs[8],"got",got,gb)
