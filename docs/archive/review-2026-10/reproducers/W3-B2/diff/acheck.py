import sys, decimal
from decimal import Decimal as D
RM = {'NE':decimal.ROUND_HALF_EVEN,'NA':decimal.ROUND_HALF_UP,'TZ':decimal.ROUND_DOWN,'TP':decimal.ROUND_CEILING,'TN':decimal.ROUND_FLOOR}
FMT = {'d64':(16,384,-383),'d32':(7,96,-95)}
FL = [(decimal.InvalidOperation,1),(decimal.DivisionByZero,2),(decimal.Overflow,4),(decimal.Underflow,8),(decimal.Inexact,16),(decimal.Clamped,32)]
def norm(x):
    if x.is_nan(): return 'NaN'
    if x.is_infinite(): return '-Infinity' if x.is_signed() else 'Infinity'
    s,dg,e = x.as_tuple(); c = int(''.join(map(str,dg))) if dg else 0
    return ('-' if s else '')+str(c)+'E'+str(e)
cnt={}; bad={}; badc={}; shown={}
for line in open(sys.argv[1]):
    t,op,m,a,b,c,r,st = line.split()
    p,emax,emin = FMT[t]
    ctx = decimal.Context(prec=p, Emax=emax, Emin=emin, clamp=1, rounding=RM[m], traps=[])
    A=D(a)
    if op=='add': y=ctx.add(A,D(b))
    elif op=='mul': y=ctx.multiply(A,D(b))
    elif op=='div': y=ctx.divide(A,D(b))
    elif op=='fma': y=ctx.fma(A,D(b),D(c))
    elif op=='sqrt':
        if A.is_signed() and not A.is_zero(): y=D('NaN'); ctx.flags[decimal.InvalidOperation]=True
        else: y=ctx.sqrt(A)
    elif op=='remnear': y=ctx.remainder_near(A,D(b))
    elif op=='remtrunc': y=ctx.remainder(A,D(b))
    elif op=='divint': y=ctx.divide_int(A,D(b))
    f=0
    for cls,bit in FL:
        if ctx.flags[cls]: f|=bit
    # libmpdec raises DivisionUndefined/DivisionImpossible as InvalidOperation subclasses: already covered
    want=norm(y); key=(t,op); cnt[key]=cnt.get(key,0)+1; st=int(st)
    if want!=r or (f&31)!=(st&31):
        bad[key]=bad.get(key,0)+1
        if shown.get(key,0)<6: print('MISMATCH',line.strip(),'want',want,f); shown[key]=shown.get(key,0)+1
    elif f!=st:
        badc[key]=badc.get(key,0)+1
        if shown.get(key,0)<6: print('CLAMPONLY',line.strip(),'want',want,f); shown[key]=shown.get(key,0)+1
for k in sorted(cnt): print(k,'cases',cnt[k],'mismatch',bad.get(k,0),'clamped-only',badc.get(k,0))
